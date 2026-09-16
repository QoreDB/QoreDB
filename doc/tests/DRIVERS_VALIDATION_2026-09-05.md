# Validation des drivers — 5 septembre 2026

## Résultats

| Suite | Résultat |
| --- | --- |
| TypeScript (`pnpm test:ts`) | 174 tests réussis, 25 fichiers |
| Types frontend (`pnpm exec tsc --noEmit`) | Réussi |
| Biome sur les fichiers modifiés | Aucune erreur ; deux avertissements de dépendances de hooks préexistants dans `AppLayout.tsx` |
| Drivers Rust, toutes les features par défaut | 327 tests réussis |
| Compilateur `qore-query` | 107 tests réussis ; 3 doctests réussis, 2 exemples ignorés |
| Génération et sécurité `qore-sql` | 137 tests réussis |
| Sous-ensemble Snowflake, BigQuery, Cassandra/ScyllaDB | 160 tests réussis, inclus dans la suite drivers |
| Intégration `integration_databases` | 27 tests exécutés avec succès ; Snowflake et BigQuery sautés faute de comptes |

La sortie Cargo affiche « 29 passed » pour l’intégration : les deux tests cloud
retournent après un message `skipped`. Ils ne constituent pas une validation
sur les services Snowflake et BigQuery réels.

## Moteurs réellement interrogés

- PostgreSQL 16 : CRUD, streaming, pagination, précision numérique ; identité
  YugabyteDB testée sur le protocole PostgreSQL.
- MySQL 8 : CRUD, streaming, pagination ; identités TiDB, StarRocks, Doris,
  SingleStore et PlanetScale testées sur le protocole MySQL.
- MongoDB 7 : CRUD et streaming ; variante TLS utilisée pour DocumentDB,
  avec vérification de la CA et refus d’un certificat non vérifiable.
- Redis 7 et Dragonfly : commandes ; identités KeyDB/Garnet testées sur Redis.
- SQL Server 2022 : connexion et requête des identités Azure SQL/Synapse.
- ClickHouse 24 et Elasticsearch 8.13.4 : tests d’intégration existants.
- SQLite et DuckDB : pagination locale.
- Cassandra 5 et ScyllaDB 6.2 : quatre tests, dont allers-retours des types
  scalaires et collections. Cassandra sans authentification, ScyllaDB avec
  `PasswordAuthenticator`.

Les tests des identités compatibles ne remplacent pas des validations sur les
produits eux-mêmes : notamment aucun cluster TiDB, YugabyteDB, StarRocks,
Doris, SingleStore, KeyDB, Garnet ou endpoint Azure n’a été provisionné.
OpenSearch n’a pas été interrogé en réel lors de cette passe.

## Corrections et compléments

- BigQuery : test HTTP simulé du dry run à travers le driver, avec namespace
  de données distinct du projet facturé, contrôle de `dryRun: true` et absence
  de polling. `EXPLAIN` accepte aussi une tabulation ou un saut de ligne.
- Frontend BigQuery : tests de demande d’estimation, des volumes indisponibles
  et des erreurs. La requête originale n’est pas soumise par cette demande.
  La confirmation visuelle n’a pas été testée dans une application Tauri en
  fonctionnement avec un compte cloud.
- Test Azure : le certificat autosigné local nécessite un mode TLS `require`
  explicite. Le mode `verify-full` par défaut du driver reste inchangé.
- Test BigQuery cloud : le projet de création du dataset est désormais celui
  utilisé pour les requêtes ; un projet sans dataset préexistant est accepté.
  Cette correction compile mais reste à exercer avec un compte réel.

## Reproduction

Depuis la racine du dépôt :

```bash
pnpm test:ts
pnpm exec tsc --noEmit
cd src-tauri
cargo test -p qore-drivers -p qore-query -p qore-sql
cargo test -p qore-drivers --no-default-features \
  --features driver-snowflake,driver-bigquery,driver-cassandra,driver-scylladb --lib
```

Les conteneurs proviennent de `docker-compose.yml`. Des ports dédiés ont été
utilisés pour éviter les services d’autres projets déjà présents :

| Service | Port hôte utilisé | Variable de test |
| --- | ---: | --- |
| PostgreSQL | 15432 | `QOREDB_TEST_PG_PORT` |
| MySQL | 13306 | `QOREDB_TEST_MYSQL_PORT` |
| MongoDB | 17017 | `QOREDB_TEST_MONGO_PORT` |
| MongoDB TLS | 17018 | `QOREDB_TEST_DOCUMENTDB_PORT` |
| Redis | 16379 | `QOREDB_TEST_REDIS_PORT` |
| Dragonfly | 16380 | `QOREDB_TEST_DRAGONFLY_PORT` |
| SQL Server | 11433 | `QOREDB_TEST_SQLSERVER_PORT` |
| ClickHouse | 18123 | `QOREDB_TEST_CLICKHOUSE_PORT` |
| Elasticsearch | 19200 | `QOREDB_TEST_ES_PORT` |
| Cassandra | 9042 | `QOREDB_TEST_CASSANDRA_PORT` |
| ScyllaDB | 9043 | `QOREDB_TEST_SCYLLADB_PORT` |

Adapter les ports publiés de Compose aux valeurs du tableau, ou conserver les
ports par défaut s’ils sont libres. Attendre que les services soient prêts.
Lancer ensuite depuis `src-tauri` avec les ports correspondants :

```bash
env \
  QOREDB_TEST_PG_PORT=15432 QOREDB_TEST_POSTGRES_REQUIRED=1 \
  QOREDB_TEST_MYSQL_PORT=13306 QOREDB_TEST_MYSQL_REQUIRED=1 \
  QOREDB_TEST_MONGO_PORT=17017 QOREDB_TEST_MONGO_REQUIRED=1 \
  QOREDB_TEST_DOCUMENTDB_PORT=17018 QOREDB_TEST_DOCUMENTDB_REQUIRED=1 \
  QOREDB_TEST_REDIS_PORT=16379 QOREDB_TEST_REDIS_REQUIRED=1 \
  QOREDB_TEST_DRAGONFLY_PORT=16380 QOREDB_TEST_DRAGONFLY_REQUIRED=1 \
  QOREDB_TEST_SQLSERVER_PORT=11433 QOREDB_TEST_SQLSERVER_REQUIRED=1 \
  QOREDB_TEST_CLICKHOUSE_PORT=18123 QOREDB_TEST_CLICKHOUSE_REQUIRED=1 \
  QOREDB_TEST_ES_PORT=19200 QOREDB_TEST_SEARCH_REQUIRED=1 \
  QOREDB_TEST_CASSANDRA_REQUIRED=1 QOREDB_TEST_SCYLLADB_REQUIRED=1 \
  cargo test --test integration_databases -- --nocapture
```

Le disque hôte étant occupé à plus de 90 %, Elasticsearch refusait d’allouer
les shards. Pour cette passe locale, ses seuils transitoires ont été fixés à
5 Go / 3 Go / 1 Go d’espace libre, puis réinitialisés après les tests.
Aucun seuil de production ni fichier Compose du dépôt n’a été modifié.
Les conteneurs démarrés pour les tests ont été arrêtés après validation ;
les volumes ont été conservés.

Les tests cloud s’activent via `QOREDB_TEST_SNOWFLAKE_ACCOUNT`, `_USER`,
`_DATABASE`, `_PRIVATE_KEY_PATH` et éventuellement `_WAREHOUSE`, ou via
`QOREDB_TEST_BIGQUERY_SERVICE_ACCOUNT_PATH`, avec `_PROJECT` et `_LOCATION`
en options. Positionner aussi `QOREDB_TEST_SNOWFLAKE_REQUIRED=1` ou
`QOREDB_TEST_BIGQUERY_REQUIRED=1` pour interdire un succès par saut du test.
Ces tests créent des objets temporaires et exécutent des requêtes : prévoir
un compte et un projet de test.
