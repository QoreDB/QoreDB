# v0.1.39 — Périmètre étendu

Fil conducteur : « tous vos moteurs, exposés en sécurité aux agents IA ». La release
garde les drivers déjà livrés sur `feat/v0.1.39` (9 wire-compatibles, Cassandra/ScyllaDB,
Snowflake, BigQuery) et y ajoute quatre lots, ordonnés par rapport valeur/effort.

| Lot | Contenu | Tier | Effort |
| --- | --- | --- | --- |
| A | MCP v2 : outils, ressources, gouvernance, écran Settings, packaging | Core | 3-4 j |
| B | Performance des requêtes : tendances, régressions, N+1, alertes | Core + Pro | 2-3 j |
| C | Masking de colonnes : grille, exports, IA, MCP | Pro | 3 j |
| D | Drivers dérivés : Amazon Keyspaces, Redshift, Turso | Core | 2 j |

Chaque lot est livrable seul. Si le temps manque, couper D puis C.

Hors périmètre, reporté sur un tag `qore-server` séparé : SAML, SCIM, hash-chain d'audit
côté serveur, licence offline. L'écriture MCP sous approbation humaine est également
reportée : elle demande un canal du binaire `qore-mcp` vers l'app desktop qui n'existe pas.

---

## Lot A — MCP v2

### État actuel

`crates/qore-mcp/src/main.rs` (291 lignes) expose 5 outils en lecture seule via
`qore_service::agent_tools` : `list_connections`, `run_query`, `list_namespaces`,
`list_tables`, `describe_table`. Constats :

- Toutes les connexions du vault sont exposées, production comprise, sans opt-in.
- `run_query` n'accepte ni base ni schéma cible : la requête part sur la base par défaut
  de la connexion.
- Les sessions ouvertes dans `sessions` ne sont jamais fermées.
- Le texte `instructions` renvoyé à `initialize` ne liste que 2 outils sur 5.
- Ni ressources ni prompts MCP.
- Absent du README, de `doc/FEATURES.csv`, des Settings, et des workflows de release
  (`build-core.yml` ne construit que l'app Tauri).

### A1. Gouvernance par connexion

- Nouveau champ `expose_to_agents: bool` sur `SavedConnection` (`vault/credentials.rs`),
  `#[serde(default)]`, faux par défaut. Case à cocher dans le formulaire de connexion,
  section Sécurité, avec un texte d'aide : « Visible par les agents IA via MCP et CLI ».
- `list_connections` ne renvoie que les connexions exposées. `ensure_session` refuse une
  connexion non exposée avec un message explicite, même si l'agent connaît l'id.
- Une connexion `production` exposée reste en lecture seule forcée (déjà le cas) et
  passe par `SafetyPolicy` : `max_result_rows`, `max_query_duration_ms`, rate limit.
- Fermeture des sessions inactives depuis plus de 10 minutes, vérifiée à chaque appel
  d'outil. Pas de tâche de fond.
- Migration : les connexions existantes ne sont pas exposées après mise à jour. Le
  changelog le dit explicitement, c'est un changement de comportement volontaire.

Vérification : test unitaire sur le filtre `list_connections` ; test sur le refus
d'`ensure_session` ; smoke manuel avec Claude Code où une connexion non cochée
n'apparaît pas.

### A2. Nouveaux outils

Tous dans `qore_service::agent_tools` pour rester partagés avec l'agent desktop, puis
câblés dans `qore-mcp`.

| Outil | Paramètres | Comportement |
| --- | --- | --- |
| `run_query` | + `database`, `schema` optionnels | Cible un namespace explicite ; sinon inchangé |
| `preview_table` | connection, database, schema, table, `limit` ≤ 100 | Échantillon via le chemin `preview_table` du moteur, donc gratuit sur BigQuery et cache-friendly |
| `explain_query` | connection, database, query | Préfixe EXPLAIN selon le dialecte (`qore-query` dialects) ; refuse sur les moteurs sans EXPLAIN avec la liste des capacités |
| `search_schema` | connection, database, `pattern` | Cherche `pattern` dans les noms de tables et de colonnes du namespace ; renvoie table, colonne, type. Pas de recherche dans les données |
| `run_federated_query` | connections[], query | Réutilise l'outil déjà présent dans l'agent desktop (`src-tauri/src/ai/agent/tools.rs`) ; Pro |
| `list_saved_queries` / `run_saved_query` | workspace, `id`, `variables` | Lit la Query Library du workspace (`ws_get_query_library`) ; substitue les variables ; refuse si la requête n'est pas en lecture |

`run_federated_query` et `run_saved_query` demandent d'extraire vers `qore-service` la
lecture du fichier de workspace et l'accès à la fédération, aujourd'hui dans le crate
Tauri. Si l'extraction dépasse une journée, livrer sans ces deux outils.

Vérification : un test par outil sur SQLite en mémoire dans `qore-service` ; smoke
manuel sur PostgreSQL et MongoDB via Claude Code.

### A3. Ressources et prompts

- Ressources `qore://{connection_id}/{database}/{table}` renvoyant le `describe_table`
  en JSON, listées par `resources/list` sur les connexions exposées. Capacité
  `enable_resources()` dans `get_info`.
- Trois prompts : `audit_table` (structure, index, volumétrie, colonnes sensibles),
  `explain_slow_query` (plan et suggestions), `document_schema` (Markdown du namespace).
- `instructions` réécrit pour lister tous les outils et rappeler la lecture seule.

Vérification : `resources/list` et `prompts/list` inspectés avec l'inspecteur MCP.

### A4. Écran Settings « Agents IA »

Nouvelle section `src/components/Settings/sections/AgentsSection.tsx` :

- État : chemin du binaire `qore-mcp` détecté, version.
- Snippet de configuration prêt à copier pour Claude Desktop, Claude Code et Cursor,
  avec le chemin absolu résolu.
- Liste des connexions exposées avec lien vers le formulaire.
- Rappel des limites appliquées (lecture seule, timeout, lignes max).

i18n dans les 9 fichiers de locale. Composants `ui/` uniquement.

### A5. Packaging et documentation

- `build-core.yml`, `build-pro.yml`, `release.yml` : construire `qore-mcp` et `qore-cli`
  et les joindre aux assets de release pour les trois OS. Sur macOS et Windows, inclure
  les binaires dans le bundle à côté de l'exécutable principal pour que l'écran Settings
  puisse en résoudre le chemin.
- README : section « Agents IA et MCP » avec le snippet.
- `doc/FEATURES.csv` : ligne MCP server mise à jour, ligne Settings Agents.
- `doc/tests/` : procédure de smoke MCP.

---

## Lot B — Performance des requêtes

### État actuel

`ProfilingPanel.tsx` affiche déjà un aperçu (total, taux de succès, P50/P95/P99) et la
liste des requêtes lentes. Les données sont en mémoire dans `ProfilingStore` et perdues
au redémarrage. `audit.jsonl` sur disque contient par entrée `timestamp`, `fingerprint`,
`execution_time_ms`, `driver_id`, `database`, `success` : c'est la source pour les
tendances. L'item « Alerting » de `doc/todo/v3.md` est le seul point ouvert de la
section Audit Log.

### B1. Agrégation par empreinte (Core)

- Nouveau module `interceptor/trends.rs` : lit `audit.jsonl`, regroupe par
  `fingerprint` et par fenêtre (jour), calcule count, P50, P95, taux d'erreur.
- Commande `get_query_trends(days, driver_id?, database?)`.
- Onglet « Tendances » dans `ProfilingPanel` : tableau des 50 empreintes les plus
  fréquentes avec sparkline P95 sur 14 jours (Recharts, déjà présent).

Vérification : test sur un `audit.jsonl` de fixture ; le tableau affiche les mêmes
chiffres que le fixture.

### B2. Régressions et N+1 (Pro)

- Régression : P95 des dernières 24 h supérieur à 2 fois le P95 des 7 jours précédents
  sur au moins 20 exécutions. Marqueur dans le tableau et badge dans la barre d'état.
- N+1 : dans une même session, plus de 20 exécutions de la même empreinte en moins de
  2 secondes. Détection dans `pipeline.rs` au moment de l'enregistrement, entrée
  d'audit dédiée `safety_rule = "n_plus_one"`, notification unique par session.

Vérification : test unitaire des deux règles ; script de reproduction N+1 sur SQLite.

### B3. Alertes de seuil (Pro)

- Deux seuils dans `InterceptorSettingsPanel` : taux d'erreur sur 15 minutes et nombre
  de requêtes lentes sur 15 minutes.
- Dépassement : notification in-app, entrée d'audit, une seule fois par fenêtre.

---

## Lot C — Masking de colonnes

### État actuel

`src-tauri/src/redaction.rs` détecte les colonnes sensibles par nom
(`is_sensitive_column`) et sert Time-Travel et le contexte IA. Rien ne masque les
valeurs dans la grille, les exports ou les réponses MCP. Le Jalon 6 de
`doc/private/QORE_PLATFORM_ROADMAP.md` prévoit le masking côté serveur ; ce lot le
livre côté desktop, sur la même primitive.

### C1. Primitive partagée

- Déplacer `is_sensitive_column` et la liste des tokens vers
  `qore-service::redaction` pour que `qore-mcp` et `qore-cli` y aient accès.
- Nouveau type `MaskingRule { table, column, mode }` avec `mode` parmi `hidden`,
  `partial` (3 premiers caractères puis `***`), `hash` (SHA-256 tronqué, stable pour
  les jointures).
- Stockage par connexion dans `SavedConnection.masking: Vec<MaskingRule>`, plus un
  booléen `mask_detected_columns` qui applique `partial` sur les colonnes détectées par
  nom.

### C2. Application

Le masque s'applique dans `qore-service::query` après exécution, avant tout retour :
grille, exports, snapshots, Data API, agent IA, MCP, CLI. Un seul point d'application,
donc pas de chemin oublié. Les colonnes masquées portent un drapeau dans `ColumnInfo`
pour que la grille affiche une icône et refuse l'édition inline.

### C3. UI

- Onglet « Masquage » dans le formulaire de connexion : tableau des règles, ajout par
  sélection table puis colonne, mode.
- Menu contextuel de colonne dans la grille : « Masquer cette colonne ».
- En production, retirer un masque demande la confirmation habituelle.

Vérification : test `qore-service` sur les trois modes ; test d'intégration PostgreSQL
où une colonne `email` masquée revient masquée par la commande query, l'export CSV et
`qore-mcp run_query`.

Licence : fichiers nouveaux en BUSL-1.1, à ajouter à la liste Premium de `CLAUDE.md`.

---

## Lot D — Drivers dérivés

Même approche que les 9 wire-compatibles : identité déclarée, capacités restreintes,
DSN reconnu, logo, tests d'intégration gated sur credentials.

| Driver | Dérive de | Spécificités |
| --- | --- | --- |
| Amazon Keyspaces | `cql` | TLS obligatoire, port 9142, authentification SigV4 ou identifiants de service ; pas de `ALLOW FILTERING`, pagination stricte |
| Amazon Redshift | `postgresql` | Introspection via `svv_*` et `pg_table_def` ; pas de `pg_class.reltuples` fiable ; types `SUPER` ; refuser les migrations ALTER non supportées |
| Turso / libSQL | `sqlite` | URL `libsql://` avec token ; protocole HTTP Hrana plutôt que fichier local ; capacités lecture/écriture, pas de WAL local |

Turso est le plus coûteux car il change le transport. Le livrer en dernier.

---

## Ordre de livraison

1. A1, A2, A3 : cœur MCP, testable sans UI.
2. A4, A5 : Settings, packaging, docs.
3. B1 puis B2, B3.
4. C1, C2, C3.
5. D dans l'ordre Keyspaces, Redshift, Turso.

Après chaque lot : `pnpm lint:fix`, `pnpm test`, entrée dans `src/data/changelog.ts`,
ligne dans `doc/FEATURES.csv`, `doc/tests/DRIVER_LIMITATIONS.md` pour le lot D.
