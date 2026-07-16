# Migrations Manager — spec de la prochaine release

> Livrée en `v0.1.34`, à deux exceptions près, reportées faute d'être nécessaires au flux principal :
>
> - **Timeline des migrations** (Phase 4) : le panneau affiche une liste avec badges de statut
>   (appliquée / en attente / annulée / échouée), pas un historique chronologique.
> - **« Ajouter à une migration » depuis le Sandbox** (§ Levée d'ambiguïté) : `MigrationPreview`
>   sait toujours seulement copier, télécharger ou appliquer son script DML.
>
> Ajouts non prévus par la spec, imposés par un audit : état `failed` dans la table d'historique
> pour les drivers dont le DDL n'est pas transactionnel, et splitter SQL dédié préservant le texte
> original (`qore-sql/src/migration_split.rs`).

Cible : desktop `v0.1.34` (cadence `v0.1.x`). Thème : cycle de vie du schéma.
Approche retenue : hybride — socle de migrations par fichiers SQL versionnés (façon dbmate/Flyway), plus une couche légère de schema-diff pour générer les migrations et comparer les environnements.

## Objectif

Donner à QoreDB le maillon manquant de son workflow développeur : un gestionnaire de migrations de schéma versionnées, appliquées et réversibles, partagées par l'équipe via Git. Aujourd'hui les briques existent (génération DDL/ALTER, introspection, workspace `.qoredb/`, comparaison multi-connexion) mais rien ne les relie en un flux versionné.

Critère de succès global : depuis un projet `.qoredb/`, un développeur peut créer une migration (à la main ou générée depuis un changement de schéma), l'appliquer sur une connexion avec garde-fous transactionnels, voir l'état appliqué/en attente par environnement, faire un rollback, et comparer le schéma de deux connexions (Prod↔Staging) en un clic.

## Levée d'ambiguïté sur le vocabulaire

Dans le code actuel, « migration » désigne un script **DML** (INSERT/UPDATE/DELETE) généré par le Sandbox (`qore-sql/generator.rs`, `components/Sandbox/MigrationPreview.tsx`). Le DDL/ALTER est une brique séparée, en TypeScript pur (`src/lib/ddl/`), sans lien avec ce flux.

Décision : la nouvelle feature s'appelle **Migrations** (migrations de schéma versionnées). Une migration est une paire de scripts SQL arbitraires (`up`/`down`) pouvant contenir du DDL et/ou du DML. Le Sandbox reste inchangé ; son script DML devient une des sources possibles pour peupler une migration (« Ajouter à une migration »). On ne renomme rien d'existant (changement chirurgical).

## Périmètre v1 (borné)

Inclus :

- Drivers SQL avec ALTER supporté : PostgreSQL, CockroachDB, MySQL, MariaDB, SQLite, DuckDB, SQL Server (les 5 dialectes couverts par `src/lib/ddl/`).
- Schema-diff au niveau de ce que `describe_table` retourne fiablement : colonnes (nom, type, nullable, default), clé primaire, clés étrangères mono-colonne, index.

Hors périmètre v1 (mêmes limites que la DDL UI actuelle, donc cohérent) :

- Contraintes CHECK (jamais introspectées aujourd'hui — `AlterTableModal.tsx` force `setChecks([])`).
- Actions référentielles FK (`ON DELETE/UPDATE`), FK composites côté introspection, length/precision/scale, colonnes générées/identity.
- Migrations pour les drivers non-SQL (Mongo, Redis, Elasticsearch, ClickHouse) — le runner exécute du SQL brut ; le schema-diff est SQL-DDL-centré.

Ces limites sont documentées dans l'UI (warnings driver-specific réutilisés de `src/lib/ddl/driverCapabilities.ts` et `warnings.ts`).

## Modèle & stockage

### Fichiers de migration

Répertoire `.qoredb/migrations/`, un fichier par migration, format dbmate (une migration = un fichier, sections `up`/`down`), Git-friendly et portable vers les outils de l'écosystème :

```
.qoredb/migrations/0001_create_users.sql
```

```sql
-- migrate:up
CREATE TABLE users (id serial primary key, email text not null);

-- migrate:down
DROP TABLE users;
```

Convention de nom : `<version>_<slug>.sql`, `version` = entier zéro-paddé monotone (`0001`, `0002`…). Le slug est validé avec `validate_connection_id` (`workspace/connection_store.rs:71`, rejette `..`, `/`, `\`, espaces).

### Table d'historique (dans la base cible)

L'état appliqué vit dans la base cible, pas dans `.qoredb/` (comme Flyway) — chaque environnement suit ses propres migrations appliquées.

Table `qoredb_migrations` (nom volontairement préfixé pour éviter la collision avec un `schema_migrations` / `flyway_schema_history` existant) :

| Colonne | Rôle |
| --- | --- |
| `version` | PK, identifiant de la migration |
| `name` | slug lisible |
| `checksum` | SHA-256 du contenu `up` au moment de l'application (détection de dérive du fichier) |
| `applied_at` | horodatage |
| `applied_by` | session/utilisateur |
| `execution_ms` | durée |
| `rolled_back_at` | non-null si annulée |

La table est créée à la première application (DDL minimal par dialecte).

### Workspace — ajustements requis

- `workspace/manager.rs:135` : ajouter `migrations/` à la liste des sous-dossiers créés par `create_workspace`.
- `workspace/watcher.rs:76` (`is_json_file`) : autoriser aussi `.sql` **sous `migrations/`** (aujourd'hui le watcher ne forwarde que `.json`, donc les `.sql` ne déclencheraient aucun événement de live-reload).
- `workspace/watcher.rs:40` (`classify_path`) : ajouter un bras `migrations`, une constante d'événement `workspace_fs:migrations`, un bras dans `category_event_name`.
- Nouvelles commandes `commands/workspace_migrations.rs`, calquées sur `commands/workspace_queries.rs` : `ws_list_migrations`, `ws_read_migration`, `ws_write_migration`, `ws_delete_migration`. Respecter le no-op si `WorkspaceSource::Default` (pattern `workspace_queries.rs:30`), l'écriture via `write_registry.register_with_auto_unregister`, et l'enregistrement dans `lib.rs`.
- Frontend : listener `workspace_fs:migrations` dans `WorkspaceProvider.tsx` (recharge la liste des migrations).

## Chemin d'exécution unifié (transactionnel + preflight)

Aujourd'hui trois chemins d'apply coexistent (sandbox row-ops transactionnel · ALTER via `execute_query` séquentiel non transactionnel · mutation preflight). Le Migrations Manager impose **un seul** chemin :

Nouvelle commande `apply_migration(session_id, version, direction)` :

1. Preflight via `qore_service::mutation::preflight` (read-only, capabilities, interceptor safety).
2. `driver.begin_transaction()` (méthode déjà utilisée par le sandbox, `commands/sandbox.rs:172`).
3. Exécution ordonnée des statements du script (`up` ou `down`) via l'exécution SQL du driver.
4. Écriture de la ligne d'historique dans `qoredb_migrations`.
5. `commit()` — ou `rollback()` + remontée d'erreur détaillée si un statement échoue.

Limite driver connue à surfacer : MySQL/MariaDB **committent implicitement le DDL** — un échec au milieu d'une migration DDL n'est pas annulable. PostgreSQL, SQLite, SQL Server supportent le DDL transactionnel. Warning explicite par driver (réutiliser `getDdlCapabilities`).

## Couche schema-diff (Pro)

Génération de migrations depuis un delta de schéma, en réutilisant le moteur DDL existant.

- Mapping `TableSchema` (introspection) → `TableDefinition` (`src/lib/ddl/types.ts`) : la conversion existe déjà partiellement dans `src/components/Table/loadAlterTable.ts` (`tableSchemaToColumns`, `...ForeignKeys`, `...Indexes`).
- Diff par table via `diffTableDefinitions(before, after, options)` (`src/lib/ddl/alterTable.ts:21`) → `AlterOp[]`.
- Génération SQL via `buildAlterTableSQL(table, ops, driver)` (`alterTable.ts:140`, dispatch par driver).
- Diff niveau base : énumérer les tables des deux côtés (`listNamespaces` + cache `useSchemaCache`), `describeTable` par table, produire tables ajoutées/supprimées/modifiées.
- Génération `down` : nouveau helper `invertOps(ops)` (add-column ↔ drop-column, etc.). Les opérations à perte (drop-column, drop-table) sont marquées **irréversibles** avec warning ; le `down` correspondant est laissé en commentaire à compléter manuellement.

Deux usages :

1. « Générer une migration » = diff base live vs baseline capturée → up + down proposés.
2. « Diff Prod↔Staging » = diff schéma entre deux connexions (réutilise la plomberie UI `CompareTargets` de `TableContextMenu.tsx`, aujourd'hui limitée aux données).

## Drift detection (Pro, pragmatique)

v1 basé sur snapshot, pas sur replay de migrations (replay dans une base scratch = hors périmètre) :

- « Marquer le schéma courant comme baseline » capture un snapshot structuré (nouvelle sérialisation de `TableSchema` pour toutes les tables).
- « Vérifier le drift » = schema-diff live vs baseline → liste des changements hors-bande.

Honnête sur ce que ça détecte (modifications faites hors QoreDB depuis la dernière baseline) et ce que ça ne fait pas (déduire l'état attendu depuis les fichiers de migration).

## Découpage Core / Pro

Ligne proposée (cohérente avec l'open core « la plomberie est Core, l'intelligence est Pro » — cf. EXPLAIN Core / Index Suggestions Pro) :

- Core (`Apache-2.0`) : runner de migrations par fichiers — créer/éditer/lister les migrations, appliquer/rollback avec l'historique, état appliqué/en attente par connexion. Un runner de migrations complet et fonctionnel, moteur d'adoption.
- Pro (`BUSL-1.1`) : la couche intelligence — génération auto de migration depuis un delta de schéma, drift detection, diff de schéma Prod↔Staging.

Divergence assumée avec la fiche initiale (qui plaçait apply/rollback en Pro) : capper l'apply pénaliserait l'adoption ; la valeur payante est l'automatisation, pas l'exécution. À valider.

SPDX à poser à la création :

- `src-tauri/src/commands/workspace_migrations.rs`, runner backend, store frontend, UI runner → `Apache-2.0`.
- Modules schema-diff / génération / drift / diff Prod↔Staging → `BUSL-1.1`. Ajouter ces chemins à la section « Current Premium scope » du `CLAUDE.md`.

## Phases (chacune avec critère de vérification)

### Phase 1 — Stockage & modèle (Core)

Dossier `.qoredb/migrations/`, format de fichier, watcher `.sql`, commandes `ws_*_migration`, store + provider frontend, panneau de liste.

Vérif : créer un fichier de migration depuis l'UI le fait apparaître dans la liste ; une édition externe du fichier déclenche un live-reload (événement `workspace_fs:migrations`) ; les fichiers sont diffables en Git ; no-op propre hors workspace (`WorkspaceSource::Default`).

### Phase 2 — Historique & runner (Core)

Table `qoredb_migrations`, `apply_migration(up/down)` transactionnel + preflight, statut appliqué/en attente par connexion.

Vérif : appliquer une migration `CREATE TABLE` sur PG + MySQL + SQLite crée la table et insère une ligne d'historique ; rollback supprime la table et marque `rolled_back_at` ; un statement en erreur au milieu annule la transaction (PG/SQLite) ; le caveat DDL MySQL est affiché ; le checksum détecte un fichier modifié après application.

### Phase 3 — Génération par schema-diff (Pro)

Mapping `TableSchema`→`TableDefinition`, diff live vs baseline, génération up/down via les builders DDL, `invertOps`, marquage des opérations irréversibles.

Vérif : ajouter une colonne en base puis « générer une migration » produit l'`ALTER … ADD COLUMN` correct en up et son inverse en down ; supprimer une colonne produit un up correct et un down marqué irréversible (commenté) ; testé PG + MySQL + SQLite.

### Phase 4 — Drift & diff Prod↔Staging (Pro)

Capture de baseline, vérification de drift, entrée « Diff schéma » depuis `CompareTargets`, timeline des migrations.

Vérif : un `ALTER TABLE` fait hors QoreDB est détecté comme drift ; le diff Prod↔Staging liste les différences structurelles (table présente d'un seul côté, colonne ajoutée/supprimée, type changé) et non les données ; un clic depuis le menu contextuel d'une table ouvre le diff de schéma.

## i18n

Nouvelles clés ajoutées aux 9 locales (`src/locales/*`), français accentué et concis. Bloc `migrations` couvrant : liste, éditeur, statuts (appliquée/en attente/annulée/drift), boutons apply/rollback, warnings driver-specific, diff de schéma.

## Documentation & release

- `doc/FEATURES.csv` : ajouter les lignes (runner Core, schema-diff/drift Pro).
- README : mentionner le Migrations Manager.
- `doc/todo/v3.md` : cocher/mettre à jour la section « Environment-Aware Workspaces » (le diff Prod↔Staging schéma en un clic, dernier point non coché).
- Cette spec est déplacée en `doc/archive/` une fois la feature livrée.
- Checklist de release standard : SPDX corrects, i18n 9 locales, gating Pro vérifié, `cargo check` / `tsc --noEmit` / `biome check` / `pnpm build` clean, bump version `package.json` + `Cargo.toml`.

## Risques

- Introspection incomplète (CHECK, actions FK, précision) : le schema-diff peut sous- ou sur-signaler. Mitigation : périmètre v1 borné + warnings, identiques aux limites déjà assumées par la DDL UI.
- DDL non transactionnel MySQL/MariaDB : rollback partiel impossible. Mitigation : warning explicite, recommandation de migrations DDL atomiques (un objet par migration) sur ces drivers.
- Collision du nom de table d'historique : mitigée par le préfixe `qoredb_`. Détection d'un Flyway/dbmate existant → hors v1.
- `down` généré non fiable pour les opérations à perte : marqué irréversible, jamais exécuté silencieusement.
- Dispersion : Phases 1-2 (Core) livrent déjà un produit utile et autonome. Si le temps manque, Phases 3-4 (Pro) peuvent glisser à `v0.1.35` sans casser la cohérence.
