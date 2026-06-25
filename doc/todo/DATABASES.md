# QoreDB — Databases supportées

> Liste des moteurs de bases de données supportés ou prévus.

---

## Implémentés

### SQL Relationnel

- [x] **PostgreSQL** — Driver complet (connexion, requêtes, schémas, SSL, SSH)
- [x] **MySQL** — Driver complet
- [x] **MariaDB** — Via driver MySQL (compatible)
- [x] **SQL Server** — Driver complet (connexion, requêtes, schémas, transactions, SSL, SSH)
- [x] **SQLite** — Base locale, fichier unique
- [x] **CockroachDB** — PostgreSQL-compatible, distribué

### SQL Analytique

- [x] **DuckDB** — Analytics embarqué (OLAP), fichier local
- [x] **ClickHouse** — Analytics OLAP _(v0.1.28)_

### Time-Series

- [x] **TimescaleDB** — Extension PostgreSQL

### Cloud-Native / Serverless

- [x] **Neon** — PostgreSQL serverless
- [x] **Supabase** — PostgreSQL (API REST)

### NoSQL Document

- [x] **MongoDB** — Driver complet (connexion, collections, find, aggregate)

### NoSQL Key-Value

- [x] **Redis** — Cache / store in-memory

### Search

- [x] **Elasticsearch** — Recherche full-text (REST/HTTP, console Dev Tools, Query DSL)
- [x] **OpenSearch** — Fork Elasticsearch (driver mutualisé `search_compat`)

---

## Prévus

### Search / Analytics

- [ ] **Apache Druid** — Real-time analytics

### SQL Relationnel

- [ ] **Oracle Database** — Enterprise

### NoSQL Document

- [ ] **CouchDB** — HTTP/REST API
- [ ] **Amazon DocumentDB** — MongoDB-compatible (AWS)

### NoSQL Key-Value

- [ ] **Valkey** — Fork open-source de Redis (réutilise le driver Redis)
- [ ] **Memcached** — Cache distribué
- [ ] **Amazon DynamoDB** — Key-value AWS

### NoSQL Colonnes

- [ ] **Cassandra** — Wide-column store
- [ ] **ScyllaDB** — Cassandra-compatible, performance
- [ ] **HBase** — Hadoop ecosystem

### NoSQL Graphe

- [ ] **Neo4j** — Graphe natif, Cypher
- [ ] **Amazon Neptune** — Graphe AWS
- [ ] **ArangoDB** — Multi-model (document + graphe)

### Time-Series

- [ ] **InfluxDB** — Time-series natif
- [ ] **QuestDB** — Time-series haute performance
- [ ] **Prometheus** — Métriques (read-only)

### Cloud-Native / Serverless

- [ ] **PlanetScale** — MySQL serverless
- [ ] **Turso** — SQLite edge (libSQL)
- [ ] **Cloudflare D1** — SQLite edge

### Embedded / Local

- [ ] **LevelDB** — Key-value embarqué
- [ ] **RocksDB** — Key-value haute perf

---

## Non prévus (hors scope)

- [ ] **Mainframe (DB2 z/OS, IMS)** — Trop niche
- [ ] **Legacy (Sybase, Informix)** — Marché très réduit
- [ ] **Propriétaires cloud-only sans API standard** — Lock-in

---

## Support DDL Management UI (CREATE / ALTER TABLE)

> Matrice de support de l'éditeur visuel CREATE/ALTER TABLE introduit en v0.1.27.

| Driver         | CREATE TABLE | ALTER TABLE | FK | Indexes | CHECK | Comments | Notes |
| -------------- | :----------: | :---------: | :-: | :-----: | :---: | :------: | ----- |
| PostgreSQL     | ✅           | ✅          | ✅  | ✅      | ✅    | ✅       | Support complet |
| MySQL / MariaDB| ✅           | ✅          | ✅  | ✅      | ✅¹   | ✅       | ¹ CHECK respecté à partir de MySQL 8.0.16 / MariaDB 10.2 |
| SQLite         | ✅           | ⚠️          | ✅  | ✅      | ✅    | ❌       | ALTER limité avant SQLite 3.35 (warning explicite, pas de DROP/ALTER COLUMN auto) |
| DuckDB         | ✅           | ✅          | ⚠️  | ✅      | ✅    | ✅       | FK syntaxiques uniquement (non vérifiées au runtime) |
| SQL Server     | ✅           | ✅          | ✅  | ✅      | ✅    | ⚠️       | Comments via `sp_addextendedproperty` |
| CockroachDB    | ✅           | ✅          | ✅  | ✅      | ✅    | ✅       | Wire-compatible PostgreSQL |
| ClickHouse     | ✅           | ⚠️          | ❌  | ✅      | ✅    | ✅       | MergeTree-family subset. Pas de FK enforcement (laissée syntaxique uniquement). INDEX … TYPE bloom_filter\|minmax\|set. (v0.1.28) |
| MongoDB        | ❌           | ❌          | —  | —       | —     | —        | Pas de schéma rigide. Voir `CreateCollectionModal` (v0.3.x). |
| Redis          | ❌           | ❌          | —  | —       | —     | —        | Pas applicable (KV store). |
| Elasticsearch  | ❌           | ❌          | —  | —       | —     | —        | DDL visuel non applicable. Création d'index via la console (`PUT /index`). |
| OpenSearch     | ❌           | ❌          | —  | —       | —     | —        | Idem Elasticsearch (driver mutualisé). |

Légende : ✅ supporté · ⚠️ partiel ou avec limitations · ❌ non applicable

---

## Architecture Driver

Chaque driver implémente le trait `DataEngine` :

```rust
pub trait DataEngine: Send + Sync {
    fn driver_id(&self) -> &'static str;
    fn driver_name(&self) -> &'static str;
    async fn test_connection(&self, config: &ConnectionConfig) -> EngineResult<()>;
    async fn connect(&self, config: &ConnectionConfig) -> EngineResult<SessionId>;
    async fn disconnect(&self, session: SessionId) -> EngineResult<()>;
    async fn list_namespaces(&self, session: SessionId) -> EngineResult<Vec<Namespace>>;
    async fn list_collections(&self, session: SessionId, namespace: &Namespace) -> EngineResult<Vec<Collection>>;
    async fn execute(&self, session: SessionId, query: &str) -> EngineResult<QueryResult>;
    async fn describe_table(&self, session: SessionId, namespace: &Namespace, table: &str) -> EngineResult<TableSchema>;
    async fn preview_table(&self, session: SessionId, namespace: &Namespace, table: &str, limit: u32) -> EngineResult<QueryResult>;
    async fn cancel(&self, session: SessionId) -> EngineResult<()>;
}
```

---

## Priorités suggérées

| Priorité | Database      | Raison                                         |
| -------- | ------------- | ---------------------------------------------- |
| +        | Valkey        | Fork Redis, réutilise le driver existant       |
| +        | Oracle        | Angle enterprise (QorePlatform)                |
| +        | Neo4j         | Niche mais différenciant (graphe / Cypher)     |
