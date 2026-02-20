# QoreDB — Databases supportées

> Liste des moteurs de bases de données supportés ou prévus.

---

## ✅ Implémentés (POC)

### SQL Relationnel

- [x] **PostgreSQL** — Driver complet (connexion, requêtes, schémas, SSL, SSH)
- [x] **MySQL** — Driver complet
- [x] **MariaDB** — Via driver MySQL (compatible)
- [x] **SQL Server** — Driver complet (connexion, requêtes, schémas, transactions, SSL, SSH)

### SQL Analytique

- [x] **DuckDB** — Analytics embarqué (OLAP), fichier local

### NoSQL Document

- [x] **MongoDB** — Driver complet (connexion, collections, find, aggregate)

---

## 🔜 Prévus (V1 / V2)

### SQL Relationnel

- [x] **SQLite** — Base locale, fichier unique
- [ ] **Oracle Database** — Enterprise
- [ ] **CockroachDB** — PostgreSQL-compatible, distribué

### NoSQL Document

- [ ] **CouchDB** — HTTP/REST API
- [ ] **Amazon DocumentDB** — MongoDB-compatible (AWS)

### NoSQL Key-Value

- [x] **Redis** — Cache / store in-memory
- [ ] **Valkey** — Fork open-source de Redis
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
- [ ] **TimescaleDB** — Extension PostgreSQL
- [ ] **QuestDB** — Time-series haute performance
- [ ] **Prometheus** — Métriques (read-only)

### Search / Analytics

- [ ] **Elasticsearch** — Recherche full-text
- [ ] **OpenSearch** — Fork Elasticsearch
- [ ] **ClickHouse** — Analytics OLAP
- [ ] **Apache Druid** — Real-time analytics

### Cloud-Native / Serverless

- [ ] **PlanetScale** — MySQL serverless
- [ ] **Neon** — PostgreSQL serverless
- [ ] **Supabase** — PostgreSQL (API REST)
- [ ] **Turso** — SQLite edge (libSQL)
- [ ] **Cloudflare D1** — SQLite edge

### Embedded / Local

- [ ] **LevelDB** — Key-value embarqué
- [ ] **RocksDB** — Key-value haute perf

---

## 🚫 Non prévus (hors scope)

- [ ] **Mainframe (DB2 z/OS, IMS)** — Trop niche
- [ ] **Legacy (Sybase, Informix)** — Marché très réduit
- [ ] **Propriétaires cloud-only sans API standard** — Lock-in

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

| Priorité | Database      | Raison                    |
| -------- | ------------- | ------------------------- |
| V1       | SQLite        | Local-first, dev workflow |
| V1       | Redis         | Très populaire, simple    |
| V2       | ClickHouse    | Analytics use case        |
| V2       | Elasticsearch | Search use case           |
| V3       | Neo4j         | Niche mais différenciant  |
