# Plan — Snowflake, BigQuery, Cassandra/ScyllaDB (+ batch wire-compatible)

> Plan de livraison pour trois nouvelles familles de moteurs et un lot de drivers
> dérivés. Complète `doc/todo/DATABASES.md`, qui reste la liste de référence.

---

## Périmètre

| Lot | Contenu | Nature |
| --- | ------- | ------ |
| A | Batch wire-compatible (TiDB, YugabyteDB, StarRocks/Doris, KeyDB/Garnet, Azure SQL, Redshift, QuestDB, Quickwit) | Dérivés de drivers existants |
| B | Snowflake, puis BigQuery | Nouvelle famille : entrepôt cloud sur HTTPS |
| C | Cassandra + ScyllaDB (+ Amazon Keyspaces) | Nouvelle famille : wide-column |

---

## Constat de départ

Le coût d'un driver est très asymétrique, et le facteur discriminant n'est pas
le protocole mais l'introspection.

| Driver | Lignes | Raison |
| ------ | -----: | ------ |
| Elasticsearch | 212 | délègue tout à `search_compat` |
| TimescaleDB | 480 | délègue à `pg_compat` |
| Neon | 494 | délègue à `pg_compat` |
| SQLite | 2168 | autonome |

En revanche, la surface *hors* driver est large et constante : l'ajout de
PlanetScale (v0.1.38), pourtant un simple dérivé MySQL, a touché **41 fichiers**.
C'est cette surface, pas le fichier `<driver>.rs`, qui dimensionne le lot A.

---

## Principe d'implémentation : pas de dépendance lourde

Direction retenue : pour un moteur sans driver existant, écrire le client à la
main plutôt que tirer un SDK d'éditeur. Ces SDK sont dimensionnés pour des
applications serveur à fort débit — pooling multi-nœuds, routage, politiques de
retry, télémétrie — dont un client desktop interactif n'a aucun usage, et qu'il
paie en temps de compilation, en surface d'audit et en taille de binaire.

L'inventaire des dépendances rend cette direction peu coûteuse : **le lot B
n'exige aucune dépendance tierce nouvelle.**

| Besoin | Déjà présent |
| ------ | ------------ |
| Client HTTP | `reqwest` 0.12 (`rustls-tls`, `json`, `gzip`) dans `qore-drivers` |
| Signature JWT RS256 | `jsonwebtoken` 9 dans `qore-server` — à promouvoir en dépendance de workspace |
| Empreinte de clé publique | `sha2` 0.10 et `base64` 0.22, déjà dans le workspace |
| TLS sur socket brut | `tokio-rustls`, déjà dans `Cargo.lock` |

Seule exception au principe : **la cryptographie ne se réimplémente pas.** La
signature RS256 et la lecture des clés PKCS#8 passent par `jsonwebtoken`, déjà
dans l'arbre. Écrire du RSA à la main serait une faute, pas une économie.

---

## Socle partagé (à faire avant le lot B)

### `warehouse_compat` — module mutualisé Snowflake / BigQuery

Sur le modèle de `search_compat` et `pg_compat`. Les deux entrepôts partagent :

- signature d'un JWT RS256 à partir d'une clé privée PEM (`jsonwebtoken`) ;
- échange du JWT contre un token d'accès à durée de vie ~1 h, et son
  rafraîchissement transparent ;
- client HTTP `reqwest` avec retry, timeout et propagation `EngineError` ;
- soumission asynchrone d'une requête, polling d'un handle de job, annulation ;
- transformation d'un payload JSON (colonnes typées + lignes) en `QueryResult`.

C'est cette mutualisation qui justifie de faire Snowflake **et** BigQuery : le
second coûte environ la moitié du premier. Faire Snowflake seul et BigQuery
plus tard fait perdre le bénéfice.

### Champs de connexion spécifiques

`ConnectionConfig` n'a pas de champ pour une clé privée ni pour un JSON de
service account. Le précédent existe (`search_auth_mode`, `mssql_auth`,
`clickhouse_cluster`) : ajouter des champs optionnels dédiés, et faire transiter
le secret par `password`, déjà chiffré par le vault.

Côté front, `BasicSection.tsx` gère déjà un rendu conditionnel par mode d'auth
pour les moteurs de recherche — c'est le patron à reprendre pour la saisie
d'une clé PEM ou d'un JSON de service account.

### Dialectes

`qore-query/src/dialect.rs` expose cinq variantes, dont aucune ne correspond aux
nouveaux moteurs. Chaque entrepôt reçoit sa variante et son `DialectOps` dès le
départ : emprunter `Dialect::Postgres` ou `Dialect::MySql` « en attendant »
produirait des requêtes méta silencieusement fausses, et la dette se paierait au
moment le plus coûteux — une fois des connexions utilisateurs déjà créées.

- `Dialect::Snowflake` → `SnowflakeOps` dans `compiler/snowflake.rs` : quoting
  `"`, repliement des identifiants non quotés en MAJUSCULES là où PostgreSQL
  replie en minuscules — la divergence est immédiate, pas hypothétique.
  `LIMIT` / `OFFSET`, pas d'`ILIKE`.
- `Dialect::BigQuery` → `BigQueryOps` dans `compiler/bigquery.rs` : quoting
  backtick, noms pleinement qualifiés `projet.dataset.table`,
  `INFORMATION_SCHEMA` régionalisé et préfixé par le dataset, paramètres nommés
  `@param` et non `?` ni `$1`.
- Cassandra : CQL n'est pas du SQL (ni JOIN, ni sous-requête). Pas de variante
  `Dialect` ; `from_driver_id` renvoie `None`, comme MongoDB et Redis.

---

## Lot A — batch wire-compatible

À faire **en premier**, malgré son statut de bonus : il est mécanique, sans
risque technique, et il force à écrire et valider la checklist de bout en bout
avant d'attaquer les lots coûteux. Il est livrable en une release.

Nuance importante par rapport à une première lecture : le protocole gratuit ne
garantit pas l'introspection gratuite.

### A1 — réellement mécaniques (catalogue identique)

| Moteur | Réutilise | Notes |
| ------ | --------- | ----- |
| TiDB | `sqlx-mysql` | `information_schema` conforme |
| YugabyteDB | `pg_compat` | catalogue PostgreSQL complet |
| StarRocks / Doris / SingleStore | `sqlx-mysql` | analytique, `information_schema` conforme |
| KeyDB / Garnet | driver Redis | RESP compatible |
| Azure SQL / Synapse | driver SQL Server | vues `sys.*` identiques, TLS forcé |

Coût attendu par moteur : le fichier driver est un dérivé de ~300–500 lignes,
l'essentiel du travail est la traversée de la checklist front + docs.

État au 30 août 2026 : A1 est implémenté pour les neuf identités. Les variantes
qui partagent exactement le même transport sont portées par un `flavor` dans
`MySqlDriver`, `PostgresDriver`, `RedisDriver` ou `SqlServerDriver`, plutôt que
par neuf wrappers dupliqués. Les capacités sont restreintes lorsque le dialecte
de gestion diverge : DDL visuel désactivé pour StarRocks, Doris et Synapse,
mutations structurées désactivées pour les deux moteurs OLAP et Synapse, et
objets stockés MySQL masqués sur les quatre nouveaux flavors.

### A2 — wire compatible, introspection divergente

| Moteur | Divergence | Surcoût |
| ------ | ---------- | ------- |
| Amazon Redshift | `pg_catalog` partiel, tailles via les vues `svv_*`, pas de `pg_total_relation_size` | requêtes méta dédiées |
| QuestDB | pas d'`information_schema` standard, catalogue via `tables()` / `table_columns()` | introspection à réécrire |
| Quickwit | sous-ensemble de l'API Elasticsearch | capacités à restreindre |

Ces trois-là ne sont pas des dérivés triviaux. Les traiter après A1, ou les
sortir du lot si le temps manque.

---

## Lot B — Snowflake, puis BigQuery

### B1 — Snowflake

Aucun driver natif Rust. Passer par la **SQL API v2** en REST, ce qui réutilise
la stack `reqwest` déjà en place pour les drivers search.

- Endpoint : `POST https://<account>.snowflakecomputing.com/api/v2/statements`.
- Auth : key-pair JWT RS256 en mode principal — claim
  `iss = <ACCOUNT>.<USER>.SHA256:<empreinte>`, `sub = <ACCOUNT>.<USER>`, `exp`
  plafonné à une heure, rotation transparente. Le *programmatic access token*
  vient en second mode : un simple en-tête `Bearer`, donc quasi gratuit une
  fois le premier écrit, sur le patron de `search_auth_mode`.
- Mapping : `host` = identifiant de compte, `username` = utilisateur,
  `password` = clé privée PEM ou token, `database` = base ; `options` porte
  `warehouse`, `role`, `schema`.
- `Namespace` : `database` = DATABASE, `schema` = SCHEMA. Supporté nativement.
- Introspection : `SHOW DATABASES` / `SHOW SCHEMAS` / `SHOW TABLES` et
  `INFORMATION_SCHEMA.COLUMNS`.
- Requêtes longues : réponse `202` + `statementHandle`, puis polling. Annulation
  réelle via `POST /api/v2/statements/<handle>/cancel` → `CancelSupport`
  autre que `None`, ce qui est rare parmi les drivers non-SQLx.
- Non couvert en v1 : transactions (l'API REST est sans session persistante),
  DDL visuel, routines.
- Garde-fou : chaque requête consomme des crédits de warehouse. `preview_table`
  doit poser un `LIMIT` strict, ne jamais déclencher de scan implicite, et
  l'UI doit afficher le warehouse actif.

### B2 — BigQuery

Bâti sur `warehouse_compat` posé en B1. Client REST direct plutôt que
`gcp-bigquery-client`, pour partager la couche d'auth.

- Auth : JSON de service account → JWT RS256 → échange sur
  `https://oauth2.googleapis.com/token`. La signature est le code de B1.
- Mapping : `database` = projet, `schema` = dataset ; `password` porte le JSON
  du service account ; `options` porte la `location` et le projet de
  facturation. `list_namespaces` énumère les projets accessibles au service
  account (`projects.list`) puis leurs datasets : le cross-project est possible
  dans une seule connexion. Le projet de facturation reste un champ distinct —
  c'est celui dont les quotas sont débités, pas nécessairement celui des
  données lues.
- Annulation réelle via `jobs.cancel`.
- Différenciateur : `dryRun: true` renvoie `totalBytesProcessed` **sans**
  exécuter la requête. Afficher l'estimation de volume scanné avant exécution,
  branchée sur l'`InterceptorPipeline` existante. Aucun client desktop léger ne
  le fait correctement aujourd'hui.
- Non couvert en v1 : transactions, DDL visuel.

---

## Lot C — Cassandra + ScyllaDB

Indépendant des lots A et B, parallélisable.

### Client CQL écrit à la main

Application du principe ci-dessus, mais c'est ici que l'arbitrage est réel :
contrairement au lot B, le protocole n'est pas du HTTP mais du binaire natif.

Ce qu'il faut écrire pour un client CQL v4 en lecture plus CRUD simple :

- le cadrage des trames (en-tête de 9 octets : version, flags, stream id,
  opcode, longueur) ;
- la poignée de main `STARTUP` / `READY` / `AUTHENTICATE` / `AUTH_RESPONSE`
  (`PasswordAuthenticator` se réduit à du SASL PLAIN) ;
- les opcodes `QUERY`, `PREPARE`, `EXECUTE`, `RESULT` ;
- le décodage des types CQL — le gros du travail : une vingtaine de types
  scalaires plus les collections (`list`, `map`, `set`, `tuple`, UDT) ;
- le paging state, simple blob opaque à renvoyer tel quel.

Ordre de grandeur : 1500 à 2500 lignes, dominées par le codec de types. Ce
qu'on abandonne en renonçant à la crate `scylla` — routage *token-aware*,
politiques d'équilibrage, exécution spéculative, découverte de topologie — est
précisément ce dont un client interactif mono-utilisateur n'a pas besoin. Le
compromis est donc plus défendable ici que pour un driver applicatif.

Point de vigilance : le codec de types est la seule partie du plan où une erreur
est silencieuse — une valeur mal décodée s'affiche sans lever d'erreur. Elle
doit être couverte par des tests unitaires par type, adossés à une instance
réelle via docker-compose, avant d'être considérée comme finie.

TLS via `tokio-rustls`. Un seul client couvre Cassandra, ScyllaDB et — en dérivé
quasi gratuit ensuite — Amazon Keyspaces (TLS forcé, port 9142).

### Reste du driver

- Auth `PasswordAuthenticator` + TLS optionnel : rentre dans `ConnectionConfig`
  sans champ nouveau. Protocole natif sur 9042, donc `supports_ssh` reste
  `true` — contrairement aux deux entrepôts du lot B.
- `Namespace` : `database` = keyspace, `schema` = `None`.
- Introspection : `system_schema.keyspaces` / `tables` / `columns`. La colonne
  `kind` distingue partition key, clustering key et colonne régulière, ce qui
  donne un `TableSchema` avec clé primaire composite exacte.
- Pagination : le paging state natif est un curseur opaque. C'est le premier
  driver à pouvoir déclarer `PaginationCapability { keyset: true }` sans clé
  unique requise. Vérifier comment ce curseur se branche sur
  `TableQueryOptions`, qui raisonne aujourd'hui en numéros de page.
- Mutations : insert/update/delete exigent la clé primaire complète. Renvoyer
  une erreur explicite plutôt qu'une requête qui échoue côté serveur.
- Sécurité : nouveau `cassandra_safety.rs` sur le modèle de `mongo_safety.rs`
  et `redis_safety.rs` — refuser `ALLOW FILTERING` hors développement, bloquer
  `TRUNCATE` et `DROP KEYSPACE` en production, exiger la partition key sur les
  `SELECT` non bornés.
- Front : nouvelle valeur `'wide-column'` dans le type `DataModel` — une entrée
  dans `MODEL_LABELS`, une clé i18n par langue, et rien d'autre.
  `getQueryDialect` retombe seul sur `'sql'`, donc CQL emprunte le chemin de
  l'éditeur SQL, ce qui est le comportement voulu.
- Hors périmètre : JOIN, transactions (les LWT n'en sont pas), triggers,
  routines, DDL visuel.
- Tests : images `cassandra:5` et `scylladb/scylla` dans `docker-compose.yml` —
  ce lot est le seul des trois testable intégralement en local.

---

## Checklist de bout en bout

Établie en traçant les 41 fichiers touchés par l'ajout de PlanetScale. C'est un
gabarit à reparcourir pour *chaque* lot, pas un état d'avancement : les cases
restent vides même après A1, qui l'a traversée intégralement.

### Backend Rust

- [ ] `qore-drivers/Cargo.toml` — dépendance + feature `driver-<id>`
- [ ] `qore-drivers/src/drivers/<id>.rs` — implémentation `DataEngine` + en-tête `// SPDX-License-Identifier: Apache-2.0`
- [ ] `qore-drivers/src/drivers/mod.rs` — module déclaré et ré-exporté
- [ ] `qore-service/Cargo.toml` — feature relayée + ajout à `all-drivers`
- [ ] `qore-service/src/context.rs` — enregistrement dans le `DriverRegistry`
- [ ] `qore-cli`, `qore-mcp`, `qore-server` — feature ajoutée à `all-drivers`
- [ ] `qore-query/src/dialect.rs` — `Dialect::from_driver_id`
- [ ] `qore-sql/src/generator.rs`, `migration_split.rs`
- [ ] `qore-drivers/src/fulltext_strategy.rs`
- [ ] `src-tauri/src/contracts/sql/dialect.rs` (Premium)
- [ ] `src-tauri/src/commands/backup.rs`, `migrations.rs`, `api/handlers.rs`, `time_travel/rollback.rs`
- [ ] `src-tauri/tests/integration_databases.rs`

### Frontend — garanti par le compilateur

Ces cinq `Record<Driver, …>` sont exhaustifs : ajouter une valeur à l'enum
`Driver` les fait échouer à la compilation. C'est la checklist gratuite.

- [ ] `src/lib/connection/drivers.ts` — enum `Driver` + `DRIVERS`
- [ ] `src/lib/connection/driverCapabilities.ts`
- [ ] `src/lib/ddl/driverCapabilities.ts`
- [ ] `src/lib/ddl/typeDefinitions.ts` — `COLUMN_TYPES`
- [ ] `src/lib/query/sqlFormatter.ts` — `DIALECT_MAP`

### Frontend — sans garde-fou

Aucune erreur de compilation ici : c'est là que les oublis se logent.

- [ ] `src/lib/connection/connectionUrls.ts`, `dsnDetector.ts` (+ `dsnDetector.test.ts`)
- [ ] `src/lib/migrations/drivers.ts`, `externalLinks.ts`, `share/projectTransfer.ts`
- [ ] `src/lib/tauri/backup.ts`, `tauri/maintenance.ts`
- [ ] `src/lib/templates/routineTemplates.ts`, `triggerTemplates.ts`, `ddl/alter/helpers.ts`
- [ ] `src/components/Connection/connection-modal/` — `types.ts`, `mappers.ts`, `useConnectionForm.ts`, et `BasicSection.tsx` si champs d'auth spécifiques
- [ ] `src/components/Editor/SQLEditor.tsx`, `src/components/Query/QueryPanel.tsx`
- [ ] `public/databases/<id>.png`
- [ ] `src/data/changelog.ts`
- [ ] `src/locales/*.json` — toutes les langues

### Documentation

- [ ] `README.md`
- [ ] `doc/FEATURES.csv`
- [ ] `doc/todo/DATABASES.md` — déplacer de « Prévus » vers « Implémentés », mettre à jour la matrice DDL
- [ ] `doc/tests/DRIVER_LIMITATIONS.md`
- [ ] `docker-compose.yml` si le moteur est testable en local

---

## Ordre proposé

1. **Lot A1** — les cinq dérivés mécaniques, en une passe. Valide la checklist,
   livre de la largeur de catalogue immédiatement.
2. **Lot C** — Cassandra + ScyllaDB. Le client CQL est la brique la plus longue
   à écrire, mais la moins risquée : protocole stable et documenté, auth
   classique, et seul lot vérifiable intégralement en local via docker-compose.
   Le faire avant le lot B évite d'enchaîner deux inconnues (protocole écrit à
   la main *et* auth cloud) sur le même lot.
3. **Lot B1** — Snowflake, avec `warehouse_compat` posé au passage.
4. **Lot B2** — BigQuery, qui valide l'abstraction de B1 et apporte le dry-run.
5. **Lot A2** — Redshift, QuestDB, Quickwit, si le temps le permet. Redshift
   devient plus simple une fois l'angle entrepôt travaillé en B.

Amazon Keyspaces se greffe en dérivé après C, pour un coût marginal.

---

## Décisions actées

1. **Auth Snowflake — key-pair JWT RS256, mode complet dès la v1.** C'est ce que
   les équipes ont déjà provisionné pour leur outillage, et c'est le code
   réutilisé tel quel par BigQuery. Le *programmatic access token* est ajouté
   comme second mode pour le coût d'un `match`.

2. **BigQuery — multi-projets.** `Namespace.database` = projet,
   `schema` = dataset, symétrique de Snowflake (DATABASE / SCHEMA).
   `list_namespaces` énumère les projets accessibles au service account puis
   leurs datasets. Le projet de facturation est un champ de configuration
   distinct de celui des données lues.

3. **Cassandra — nouvelle valeur `'wide-column'`.** Tranché sur la mesure du
   coût réel plutôt qu'au principe : `dataModel` n'a que deux points d'usage
   fonctionnels dans tout le front, le filtre de groupes du `DriverPicker` et
   `getQueryDialect`. Comme `'wide-column'` n'est ni `'search'` ni documentaire,
   `getQueryDialect` retombe sur `'sql'` sans une ligne de code à écrire. Le
   coût total se réduit à une valeur de type, une entrée dans `MODEL_LABELS` et
   une clé i18n par langue. Le précédent existe : `'graph'` est déjà déclaré
   sans aucun driver derrière, et le picker masque simplement un groupe vide.

   Déclarer Cassandra `'relational'` aurait au contraire menti dans le
   sélecteur de connexion en le rangeant à côté de PostgreSQL, alors qu'il n'a
   ni JOIN ni `WHERE` arbitraire.

4. **Licence — tout en Core (`Apache-2.0`).** Les nouveaux drivers portent
   l'en-tête `// SPDX-License-Identifier: Apache-2.0`, comme les vingt
   existants. À réexaminer si l'angle entrepôt devient un axe commercial : le
   déplacement Core → Premium se fait alors en un commit, en-tête et
   `LICENSE-BSL` mis à jour ensemble.

5. **Dry-run BigQuery — Core.** Cohérent avec la décision 4, et défendable sur
   le fond : l'estimation du volume scanné est autant un garde-fou de coût
   qu'une fonctionnalité. La placer derrière une licence exposerait les
   utilisateurs Core à des requêtes coûteuses sans avertissement.
