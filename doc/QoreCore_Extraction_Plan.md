# QoreCore — Plan d'extraction

> Extraire le moteur universel de QoreDB en crates Rust autonomes pour fonder QoreORM

**Version 1.0 — Mars 2026**
**Auteur :** Raphaël Plassart
**Projet :** QoreDB → QoreORM

---

## Table des matières

1. [Contexte et vision](#1-contexte-et-vision)
2. [Analyse du code existant](#2-analyse-du-code-existant)
3. [Architecture cible du workspace](#3-architecture-cible-du-workspace)
4. [Détail des 3 crates](#4-détail-des-3-crates)
5. [Plan d'extraction étape par étape](#5-plan-dextraction-étape-par-étape)
6. [Gestion des dépendances](#6-gestion-des-dépendances)
7. [Points d'attention et pièges](#7-points-dattention-et-pièges)
8. [Tests et validation](#8-tests-et-validation)
9. [Roadmap vers QoreORM](#9-roadmap-vers-qoreorm)
10. [Annexes](#10-annexes)

---

## 1. Contexte et vision

### 1.1 Pourquoi extraire qore-core ?

QoreDB contient un moteur de bases de données universel extrêmement riche : 9 drivers (PostgreSQL, MySQL, SQLite, MongoDB, Redis, DuckDB, SQL Server, CockroachDB, MariaDB), un système de types unifié, un trait d'abstraction complet, et des outils SQL réutilisables. Tout ce code est actuellement couplé dans un seul crate Tauri.

L'extraction en crates indépendants apporte trois bénéfices majeurs :

- **Pour QoreDB :** architecture plus propre, compilation incrémentale plus rapide, meilleure testabilité unitaire du moteur sans dépendre de Tauri.
- **Pour QoreORM :** fondation immédiate avec 9 drivers prêts à l'emploi, un type system universel, et un outillage SQL éprouvé en production.
- **Pour l'écosystème :** des crates publiables sur crates.io que d'autres projets Rust pourront utiliser.

### 1.2 Stratégie : Monorepo d'abord

L'extraction se fait en **Cargo workspace dans le repo QoreDB existant**. Pas de repo séparé tant que l'API n'est pas stabilisée.

> **💡 Pourquoi le monorepo ?**
> Refactoring progressif et sécurisé (chaque étape compile), un seul CI, pas de problèmes de versioning croisé. C'est l'approche utilisée par Prisma (prisma-engines), Diesel, et SQLx avant séparation.

### 1.3 Vision QoreORM

L'objectif à terme est de créer un ORM Rust + TypeScript/JavaScript compétitif avec Prisma, TypeORM et Diesel. Le différenciateur principal : **un seul ORM supportant 9+ bases de données, SQL et NoSQL, avec un cœur Rust performant**. Personne ne fait ça aujourd'hui.

---

## 2. Analyse du code existant

### 2.1 Inventaire du module engine/

Le dossier `src-tauri/src/engine/` contient 28 fichiers Rust totalisant environ 15 000 lignes de code. Voici l'inventaire complet :

| Fichier | Lignes | Rôle | Crate cible |
|---|---|---|---|
| `traits.rs` | ~590 | Trait DataEngine (50+ méthodes) | qore-core |
| `types.rs` | ~800 | Types universels (Value, Row, Schema…) | qore-core |
| `error.rs` | ~135 | EngineError unifié (15 variantes) | qore-core |
| `registry.rs` | ~80 | DriverRegistry (découverte drivers) | qore-core |
| `query_manager.rs` | ~155 | Tracking des requêtes actives | qore-drivers |
| `session_manager.rs` | ~1200 | Gestion sessions + health + SSH | qore-drivers |
| `ssh_tunnel.rs` | ~200 | Tunnel SSH (backend OpenSSH) | qore-drivers |
| `sql_safety.rs` | ~295 | Analyse sécurité SQL (mutations) | qore-sql |
| `sql_generator.rs` | ~570 | Génération SQL multi-dialecte | qore-sql |
| `connection_url.rs` | ~1115 | Parsing d'URLs de connexion | qore-sql |
| `postgres.rs` | ~700 | Driver PostgreSQL | qore-drivers |
| `pg_compat.rs` | ~1830 | Code partagé PG/CockroachDB | qore-drivers |
| `mysql.rs` | ~2450 | Driver MySQL | qore-drivers |
| `mongodb.rs` | ~1970 | Driver MongoDB | qore-drivers |
| `redis.rs` | ~2110 | Driver Redis | qore-drivers |
| `sqlite.rs` | ~1700 | Driver SQLite | qore-drivers |
| `duckdb.rs` | ~1700 | Driver DuckDB | qore-drivers |
| `sqlserver.rs` | ~2060 | Driver SQL Server | qore-drivers |
| `cockroachdb.rs` | ~620 | Driver CockroachDB | qore-drivers |
| `mariadb.rs` | ~460 | Driver MariaDB | qore-drivers |
| `fulltext_strategy.rs` | — | Stratégie full-text search | qore-drivers |
| `mongo_safety.rs` | — | Validation requêtes MongoDB | qore-drivers |
| `redis_safety.rs` | — | Validation requêtes Redis | qore-drivers |
| `schema_export.rs` | — | Export de schéma | qore-drivers |
| `collection_list.rs` | — | Helpers listing collections | qore-drivers |

### 2.2 Trait DataEngine — L'abstraction clé

Le trait `DataEngine` est au cœur de toute l'architecture. Il définit l'interface universelle que chaque driver implémente. Voici ses principales catégories de méthodes :

| Catégorie | Méthodes | Default impl. |
|---|---|---|
| Connexion | `test_connection`, `connect`, `disconnect`, `ping` | Non |
| Navigation | `list_namespaces`, `list_collections`, `describe_table` | Non |
| Exécution | `execute`, `execute_in_namespace`, `execute_stream` | Partiel |
| CRUD | `insert_row`, `update_row`, `delete_row` | Oui (NotSupported) |
| Transactions | `begin_transaction`, `commit`, `rollback` | Oui (NotSupported) |
| Routines | `list_routines`, `get/drop_routine_definition` | Oui (vide/NotSupported) |
| Triggers | `list_triggers`, `get/drop/toggle_trigger` | Oui (vide/NotSupported) |
| Events | `list_events`, `get/drop_event_definition` | Oui (vide/NotSupported) |
| Maintenance | `list_maintenance_ops`, `run_maintenance` | Oui (vide/NotSupported) |
| Méta | `capabilities`, `driver_id`, `driver_name`, `cancel_support` | Partiel |

Le design est élégant : toutes les méthodes optionnelles ont une implémentation par défaut qui retourne `NotSupported` ou une liste vide. Un driver minimal ne doit implémenter que les méthodes de connexion, navigation et exécution.

### 2.3 Système de types universel

Le type `Value` est l'enum algébrique central qui représente toute valeur de base de données :

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Bytes(Vec<u8>),         // sérialisé en base64
    Json(serde_json::Value),
    Array(Vec<Value>),
}
```

Les types d'infrastructure associés forment un modèle de données complet :

- `SessionId(Uuid)` / `QueryId(Uuid)` — identifiants type-safe (newtype pattern)
- `ConnectionConfig` — configuration de connexion complète (host, port, SSL, pool, SSH tunnel)
- `SshTunnelConfig` — auth par password ou clé, host key policy, jump host, keepalive
- `Namespace` — database + schema optionnel (PostgreSQL: db+schema, MySQL: db seul, MongoDB: db seul)
- `Collection` — table/vue/collection avec namespace et type (Table, View, MaterializedView, Collection)
- `TableSchema` — colonnes, clés primaires, clés étrangères, index, estimation row count
- `TableColumn` — nom, type, nullabilité, default, is_primary_key
- `ForeignKey` — colonne source, table/colonne référencée, schéma, contrainte, flag virtuel
- `TableIndex` — nom, colonnes, unicité, flag primary
- `QueryResult` — colonnes + lignes + affected_rows + temps d'exécution
- `PaginatedQueryResult` — QueryResult + total_rows + page + total_pages
- `RowData` — HashMap<String, Value> pour les mutations (insert/update)
- `DriverCapabilities` — transactions, mutations, cancel, SSH, streaming, explain, maintenance
- `TableQueryOptions` — pagination, tri, filtres, recherche full-text
- `FilterOperator` — Eq, Neq, Gt, Gte, Lt, Lte, Like, IsNull, IsNotNull
- Types maintenance — `MaintenanceOperationType` (Vacuum, Analyze, Reindex, Optimize, Repair…)
- Types routines — `RoutineType` (Function, Procedure), `RoutineDefinition`
- Types triggers — `TriggerTiming` (Before, After, InsteadOf), `TriggerEvent` (Insert, Update, Delete, Truncate)
- Types events — `EventStatus` (Enabled, Disabled, SlavesideDisabled)
- Types création — `CreationOptions`, `CharsetInfo`, `CollationInfo`

### 2.4 Couplage avec Tauri — État des lieux

Bonne nouvelle : le module `engine/` est remarquablement bien isolé. **Aucun fichier dans engine/ n'importe de crate Tauri.** Les seuls couplages sont :

- **SessionManager** utilise `SshTunnel` qui dépend de `tokio::process::Command` (pas de Tauri)
- **StreamSender** est un `tokio::sync::mpsc::Sender<StreamEvent>` — pur tokio, pas de Tauri
- **Le seul point de contact** est `lib.rs` qui enregistre les drivers et crée l'AppState pour Tauri

> **✅ Verdict :** L'extraction est chirurgicale : aucune réécriture nécessaire, seulement des déplacements de fichiers et des changements d'imports (`crate::engine::*` → `qore_core::*`).

---

## 3. Architecture cible du workspace

### 3.1 Structure des répertoires

```
QoreDB/
├── src/                          # Frontend React (inchangé)
└── src-tauri/
    ├── Cargo.toml                # WORKSPACE root
    ├── crates/
    │   ├── qore-core/            # Types, traits, erreurs
    │   │   ├── Cargo.toml
    │   │   └── src/
    │   │       ├── lib.rs
    │   │       ├── types.rs      # Value, Row, SessionId, ConnectionConfig…
    │   │       ├── traits.rs     # DataEngine trait + StreamEvent
    │   │       ├── error.rs      # EngineError (15 variantes)
    │   │       └── registry.rs   # DriverRegistry
    │   │
    │   ├── qore-sql/             # Outillage SQL réutilisable
    │   │   ├── Cargo.toml
    │   │   └── src/
    │   │       ├── lib.rs
    │   │       ├── safety.rs     # Analyse sécurité SQL
    │   │       ├── generator.rs  # Génération SQL multi-dialecte
    │   │       └── connection_url.rs  # Parsing URLs de connexion
    │   │
    │   └── qore-drivers/         # Toutes les implémentations de drivers
    │       ├── Cargo.toml
    │       └── src/
    │           ├── lib.rs
    │           ├── session_manager.rs
    │           ├── query_manager.rs
    │           ├── ssh_tunnel.rs
    │           ├── fulltext_strategy.rs
    │           ├── mongo_safety.rs
    │           ├── redis_safety.rs
    │           ├── schema_export.rs
    │           └── drivers/
    │               ├── mod.rs
    │               ├── postgres.rs
    │               ├── pg_compat.rs
    │               ├── mysql.rs
    │               ├── sqlite.rs
    │               ├── mongodb.rs
    │               ├── redis.rs
    │               ├── duckdb.rs
    │               ├── sqlserver.rs
    │               ├── cockroachdb.rs
    │               ├── mariadb.rs
    │               └── collection_list.rs
    │
    └── app/                      # App Tauri (shell mince)
        ├── Cargo.toml            # Dépend de qore-core, qore-sql, qore-drivers + Tauri
        ├── build.rs
        └── src/
            ├── main.rs
            ├── lib.rs
            ├── commands/         # Handlers Tauri (inchangés)
            ├── vault/
            ├── interceptor/
            ├── export/
            ├── ai/
            ├── license/
            └── ...
```

### 3.2 Pourquoi 3 crates et pas 1 ?

La séparation en 3 crates **isole les dépendances lourdes**. C'est critique pour QoreORM et pour tout consommateur externe :

| Crate | Dépendances lourdes | Poids estimé | Qui en dépend |
|---|---|---|---|
| **qore-core** | serde, uuid, tokio, async-trait, chrono, thiserror | Léger (~2 Mo) | Tout le monde |
| **qore-sql** | qore-core + sqlparser, url | Léger (~3 Mo) | ORM, outils SQL |
| **qore-drivers** | qore-core + sqlx, mongodb, tiberius, duckdb, redis | Lourd (~50+ Mo) | Optionnel par feature flag |

Un utilisateur QoreORM qui ne veut que PostgreSQL ne tirera pas les 50 Mo de DuckDB. C'est rendu possible par les **feature flags** sur qore-drivers.

### 3.3 Graphe de dépendances

```
                    ┌──────────────┐
                    │  qore-core   │  ← Zéro dép. lourde
                    └──────┬───────┘
                           │
                ┌──────────┼──────────┐
                │                     │
       ┌────────┴──────┐    ┌────────┴────────┐
       │   qore-sql    │    │  qore-drivers   │
       └───────────────┘    └────────┬────────┘
                │                     │
                └──────────┬──────────┘
                           │
                  ┌────────┴────────┐
                  │   app (Tauri)   │
                  └─────────────────┘
```

---

## 4. Détail des 3 crates

### 4.1 qore-core

Le noyau pur. **Zéro dépendance lourde.** Contient uniquement les abstractions et types que tout consommateur doit connaître.

#### Fichiers inclus

| Fichier source | Fichier cible | Modifications |
|---|---|---|
| `engine/types.rs` | `qore-core/src/types.rs` | Aucune (déjà indépendant) |
| `engine/types/` (dossier) | `qore-core/src/types/` | Aucune |
| `engine/traits.rs` | `qore-core/src/traits.rs` | Retirer `crate::engine::*` → `crate::*` |
| `engine/error.rs` | `qore-core/src/error.rs` | Aucune |
| `engine/registry.rs` | `qore-core/src/registry.rs` | Retirer `crate::engine::*` → `crate::*` |

#### Cargo.toml

```toml
[package]
name = "qore-core"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
description = "Universal database engine abstraction — types, traits, and error handling"

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
tokio = { workspace = true }
async-trait = { workspace = true }
thiserror = { workspace = true }
uuid = { workspace = true }
chrono = { workspace = true }
base64 = { workspace = true }
```

#### Exports (lib.rs)

```rust
// SPDX-License-Identifier: Apache-2.0

pub mod error;
pub mod registry;
pub mod traits;
pub mod types;

// Re-exports for convenience
pub use error::{EngineError, EngineResult};
pub use registry::DriverRegistry;
pub use traits::{DataEngine, StreamEvent, StreamSender};
pub use types::*;
```

> **💡 Point clé : StreamEvent**
> Le type `StreamEvent` et `StreamSender` (tokio mpsc) restent dans qore-core car ils font partie de l'interface du trait DataEngine (méthode `execute_stream`). Ils n'ont aucune dépendance Tauri.

---

### 4.2 qore-sql

Outillage SQL réutilisable. Dépend de `qore-core` + `sqlparser`. Utile pour l'ORM même sans drivers concrets.

#### Fichiers inclus

| Fichier source | Fichier cible | Modifications |
|---|---|---|
| `engine/sql_safety.rs` | `qore-sql/src/safety.rs` | Imports : `crate::engine::*` → `qore_core::*` |
| `engine/sql_generator.rs` | `qore-sql/src/generator.rs` | Idem |
| `engine/connection_url.rs` | `qore-sql/src/connection_url.rs` | Indépendant (n'utilise pas `engine::*`) |

#### Cargo.toml

```toml
[package]
name = "qore-sql"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
description = "SQL safety analysis, generation, and connection URL parsing"

[dependencies]
qore-core = { path = "../qore-core" }
sqlparser = { workspace = true }
url = { workspace = true }
percent-encoding = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
```

#### Capacités fournies

**SQL Safety Analysis** — Détecte mutations (INSERT/UPDATE/DELETE) et opérations dangereuses (DDL) dans du SQL multi-dialecte (PostgreSQL, MySQL, DuckDB, SQL Server, générique). Fonctions exposées :

- `analyze_sql(driver_id, sql)` → `SqlSafetyAnalysis { is_mutation, is_dangerous }`
- `returns_rows(driver_id, sql)` → `bool`
- `split_sql_statements(driver_id, sql)` → `Vec<String>`
- `is_select_prefix(sql)` → `bool`

**SQL Generator** — Génère des INSERT/UPDATE/DELETE avec quoting d'identifiants correct par dialecte. Supporte 4 dialectes SQL + MongoDB shell :

| Dialecte | Quoting identifiants | Quoting strings | Bytes |
|---|---|---|---|
| PostgreSQL | `"ident"` | `E'str\n'` | `'\x...'` |
| MySQL | `` `ident` `` | `'str'` | `X'...'` |
| SQLite | `"ident"` | `'str'` | `X'...'` |
| SQL Server | `[ident]` | `N'str'` | `0x...` |

Fonctions exposées : `generate_insert()`, `generate_update()`, `generate_delete()`, `generate_migration_script()`, `generate_mongo_operation()`.

**Connection URL Parser** — Parse des URLs de connexion pour 6 drivers via une architecture trait-based extensible (`ConnectionUrlParser`). Schémas supportés : `postgres://`, `postgresql://`, `mysql://`, `mongodb://`, `mongodb+srv://`, `redis://`, `rediss://`, `mssql://`, `sqlserver://`, `cockroachdb://`, `cockroach://`. Gère SSL inference, percent-decoding, SRV records MongoDB, query parameters.

---

### 4.3 qore-drivers

Toutes les implémentations de drivers + infrastructure de sessions. C'est le crate le plus volumineux (~15 000 lignes) et celui avec les dépendances les plus lourdes.

#### Feature flags (critique)

Chaque driver est derrière un feature flag pour que les consommateurs ne tirent que ce dont ils ont besoin :

```toml
[features]
default = ["postgres", "mysql", "sqlite"]
postgres = ["dep:sqlx"]
mysql = ["dep:sqlx"]
sqlite = ["dep:sqlx"]
mongodb = ["dep:mongodb"]
redis = ["dep:redis"]
duckdb = ["dep:duckdb"]
sqlserver = ["dep:tiberius", "dep:bb8", "dep:bb8-tiberius"]
cockroachdb = ["postgres"]        # réutilise le driver PG
mariadb = ["mysql"]               # réutilise le driver MySQL
all = ["postgres", "mysql", "sqlite", "mongodb",
       "redis", "duckdb", "sqlserver"]
```

#### Fichiers inclus

| Fichier source | Fichier cible |
|---|---|
| `engine/session_manager.rs` | `qore-drivers/src/session_manager.rs` |
| `engine/query_manager.rs` | `qore-drivers/src/query_manager.rs` |
| `engine/ssh_tunnel.rs` | `qore-drivers/src/ssh_tunnel.rs` |
| `engine/fulltext_strategy.rs` | `qore-drivers/src/fulltext_strategy.rs` |
| `engine/mongo_safety.rs` | `qore-drivers/src/mongo_safety.rs` |
| `engine/redis_safety.rs` | `qore-drivers/src/redis_safety.rs` |
| `engine/schema_export.rs` | `qore-drivers/src/schema_export.rs` |
| `engine/drivers/postgres.rs` | `qore-drivers/src/drivers/postgres.rs` |
| `engine/drivers/pg_compat.rs` | `qore-drivers/src/drivers/pg_compat.rs` |
| `engine/drivers/postgres_utils.rs` | `qore-drivers/src/drivers/postgres_utils.rs` |
| `engine/drivers/mysql.rs` | `qore-drivers/src/drivers/mysql.rs` |
| `engine/drivers/sqlite.rs` | `qore-drivers/src/drivers/sqlite.rs` |
| `engine/drivers/mongodb.rs` | `qore-drivers/src/drivers/mongodb.rs` |
| `engine/drivers/redis.rs` | `qore-drivers/src/drivers/redis.rs` |
| `engine/drivers/duckdb.rs` | `qore-drivers/src/drivers/duckdb.rs` |
| `engine/drivers/sqlserver.rs` | `qore-drivers/src/drivers/sqlserver.rs` |
| `engine/drivers/cockroachdb.rs` | `qore-drivers/src/drivers/cockroachdb.rs` |
| `engine/drivers/mariadb.rs` | `qore-drivers/src/drivers/mariadb.rs` |
| `engine/drivers/collection_list.rs` | `qore-drivers/src/drivers/collection_list.rs` |

#### Drivers : capacités par moteur

| Driver | Transactions | CRUD | Streaming | Cancel | SSH |
|---|---|---|---|---|---|
| PostgreSQL | ✓ | ✓ | ✓ | Driver (`pg_terminate`) | ✓ |
| MySQL | ✓ | ✓ | ✓ | Driver (`KILL QUERY`) | ✓ |
| SQLite | ✓ | ✓ | ✓ | BestEffort | ✓ |
| MongoDB | ✓ * | ✓ | ✗ | BestEffort (abort) | ✓ |
| Redis | ✗ | ✓ | ✗ | ✗ | ✓ |
| DuckDB | ✗ | ✓ | ✗ | BestEffort | ✗ |
| SQL Server | ✓ | ✓ | ✓ | ✗ | ✓ |
| CockroachDB | ✓ | ✓ | ✓ | Driver (`pg_terminate`) | ✓ |
| MariaDB | ✓ | ✓ | ✓ | Driver (`KILL QUERY`) | ✓ |

_* MongoDB : transactions supportées uniquement si la topologie le permet (replica set)._

#### Patterns d'implémentation des drivers

Chaque driver suit le même pattern architectural :

```rust
pub struct MyDriver {
    sessions: Arc<RwLock<HashMap<SessionId, Arc<MySession>>>>,
}
```

**Connection pooling :**
- SQL (SQLx) : `Pool<DB>` avec `PoolOptions` (max 5, min 0, timeout 30s)
- MongoDB : `Client` direct + optional `ClientSession` pour les transactions
- Redis : `MultiplexedConnection` async
- DuckDB : `std::sync::Mutex<Connection>` + `spawn_blocking` (API synchrone)
- SQL Server : `bb8::Pool<ConnectionManager>` (Tiberius)

**Transactions :**
- SQL (SQLx) : connection dédiée extraite du pool via `PoolConnection<DB>`
- MongoDB : `ClientSession` avec transactions au niveau session
- DuckDB : flag `AtomicBool` sérialisé via Mutex

---

## 5. Plan d'extraction étape par étape

Le plan est conçu pour que **chaque étape laisse le projet dans un état compilable et testable**. Temps total estimé : 1 à 2 jours de travail concentré.

### Étape 1 — Créer le Cargo workspace (30 min)

Transformer le répertoire `src-tauri/` en workspace Cargo. Le code existant est déplacé dans `app/` et devient un membre du workspace.

**Actions :**

1. Créer le dossier `src-tauri/crates/`
2. Déplacer `src-tauri/src/` vers `src-tauri/app/src/`
3. Déplacer `src-tauri/build.rs` vers `src-tauri/app/build.rs`
4. Transformer `src-tauri/Cargo.toml` en workspace root (voir ci-dessous)
5. Créer `src-tauri/app/Cargo.toml` avec le contenu de l'ancien Cargo.toml (ajuster les chemins)
6. Mettre à jour `tauri.conf.json` si nécessaire (chemins vers le binaire)
7. Vérifier : `cargo build` dans le workspace

**Workspace root `src-tauri/Cargo.toml` :**

```toml
[workspace]
members = ["app", "crates/*"]
resolver = "2"

[workspace.dependencies]
# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = { version = "1", features = ["raw_value"] }

# Async runtime
tokio = { version = "1", features = ["full"] }
tokio-util = { version = "0.7", features = ["compat"] }
async-trait = "0.1"
futures = "0.3"

# Error handling & IDs
thiserror = "2"
uuid = { version = "1", features = ["v4", "serde"] }

# Date/time & encoding
chrono = { version = "0.4", features = ["serde"] }
base64 = "0.22"

# SQL tools
sqlparser = "0.60"
url = "2"
percent-encoding = "2"

# Logging
tracing = "0.1"

# Numeric precision
rust_decimal = { version = "1", features = ["serde"] }
bigdecimal = "0.4"

# Database drivers (optionnels, utilisés par qore-drivers)
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "mysql", "sqlite", "chrono", "rust_decimal", "bigdecimal", "uuid"] }
mongodb = "3"
redis = { version = "0.27", features = ["tokio-comp", "aio", "connection-manager"] }
tiberius = { version = "0.12", features = ["tds73", "chrono", "rust_decimal", "bigdecimal"] }
bb8 = "0.9"
bb8-tiberius = "0.16"
duckdb = { version = "1.4", features = ["bundled"] }
```

> **⚠️ Vérification :** Après cette étape, `cargo build` et `pnpm tauri dev` doivent fonctionner exactement comme avant. Si ce n'est pas le cas, ne pas passer à l'étape suivante.

---

### Étape 2 — Extraire qore-core (2-3h)

C'est l'étape la plus importante. On extrait les types fondamentaux et le trait DataEngine.

**Actions :**

1. Créer `src-tauri/crates/qore-core/` avec `Cargo.toml` et `src/lib.rs`
2. Copier `engine/types.rs` → `qore-core/src/types.rs`
3. Copier `engine/types/` (dossier) → `qore-core/src/types/` (si existant)
4. Copier `engine/error.rs` → `qore-core/src/error.rs`
5. Copier `engine/traits.rs` → `qore-core/src/traits.rs`
6. Copier `engine/registry.rs` → `qore-core/src/registry.rs`
7. **Dans qore-core** — remplacer les imports internes :
   - `crate::engine::types::*` → `crate::types::*`
   - `crate::engine::error::*` → `crate::error::*`
   - `crate::engine::traits::*` → `crate::traits::*`
8. **Dans app/** — ajouter `qore-core = { path = "../crates/qore-core" }` au Cargo.toml et remplacer tous les `crate::engine::{types,error,traits,registry}` par `qore_core`
9. Supprimer les fichiers originaux dans `engine/` (après vérification)
10. Vérifier : `cargo test -p qore-core` + `cargo build`

> **💡 Astuce find & replace :** La majorité du travail est du remplacement d'imports. Utilise `cargo check` fréquemment pour détecter les erreurs d'import restantes. Le compilateur Rust te guidera précisément.

---

### Étape 3 — Extraire qore-sql (1-2h)

**Actions :**

1. Créer `src-tauri/crates/qore-sql/` avec `Cargo.toml`
2. Copier `engine/sql_safety.rs` → `qore-sql/src/safety.rs`
3. Copier `engine/sql_generator.rs` → `qore-sql/src/generator.rs`
4. Copier `engine/connection_url.rs` → `qore-sql/src/connection_url.rs`
5. Mettre à jour les imports (`crate::engine::types::*` → `qore_core::*`)
6. **Dans app/** — ajouter `qore-sql` en dépendance et rediriger les imports
7. Vérifier : `cargo test -p qore-sql` (doit passer les 37+ tests existants)

---

### Étape 4 — Extraire qore-drivers (3-4h)

Le plus volumineux mais le plus mécanique. Les fichiers bougent, les imports changent, la logique reste identique.

**Actions :**

1. Créer `src-tauri/crates/qore-drivers/` avec `Cargo.toml` (feature flags)
2. Déplacer les 9 fichiers de drivers + `pg_compat.rs` + `postgres_utils.rs` + `collection_list.rs`
3. Déplacer `session_manager.rs`, `query_manager.rs`, `ssh_tunnel.rs`
4. Déplacer `fulltext_strategy.rs`, `mongo_safety.rs`, `redis_safety.rs`, `schema_export.rs`
5. Mettre à jour tous les imports (le plus gros du travail) :
   - `crate::engine::types::*` → `qore_core::*`
   - `crate::engine::error::*` → `qore_core::{EngineError, EngineResult}`
   - `crate::engine::traits::*` → `qore_core::{DataEngine, StreamEvent, StreamSender}`
   - `crate::engine::sql_generator::*` → `qore_sql::generator::*` (si utilisé)
6. **Dans app/** — ajouter `qore-drivers = { path = "../crates/qore-drivers", features = ["all"] }`
7. Supprimer le dossier `engine/` de `app/` (ne devrait plus rien contenir)
8. Vérifier : `cargo test --workspace`

---

### Étape 5 — Validation finale (1h)

1. `cargo test --workspace` — tous les tests passent
2. `cargo clippy --workspace -- -D warnings` — zéro warning
3. `pnpm tauri dev` — l'app démarre et fonctionne
4. Tester manuellement : connexion PostgreSQL + MySQL + SQLite au minimum
5. Vérifier les **headers SPDX** sur tous les fichiers déplacés (`Apache-2.0`)
6. Commit unique avec message descriptif

---

## 6. Gestion des dépendances

### 6.1 Workspace dependencies

Toutes les dépendances partagées sont centralisées dans le workspace root. Chaque crate référence la version workspace :

```toml
# Dans qore-core/Cargo.toml :
serde = { workspace = true }

# Dans le workspace root Cargo.toml :
[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
```

Cela garantit qu'il n'y a jamais de conflit de versions entre crates.

### 6.2 Répartition des dépendances par crate

| Dépendance | Version | qore-core | qore-sql | qore-drivers |
|---|---|---|---|---|
| serde | 1 | ✓ | ✓ | ✓ |
| serde_json | 1 | ✓ | ✓ | ✓ |
| tokio | 1 | ✓ | ✗ | ✓ |
| async-trait | 0.1 | ✓ | ✗ | ✓ |
| thiserror | 2 | ✓ | ✗ | ✗ |
| uuid | 1 | ✓ | ✗ | ✓ |
| chrono | 0.4 | ✓ | ✗ | ✓ |
| base64 | 0.22 | ✓ | ✗ | ✓ |
| sqlparser | 0.60 | ✗ | ✓ | ✗ |
| url | 2 | ✗ | ✓ | ✗ |
| percent-encoding | 2 | ✗ | ✓ | ✗ |
| sqlx | 0.8 | ✗ | ✗ | ✓ (feat.) |
| mongodb | 3 | ✗ | ✗ | ✓ (feat.) |
| redis | 0.27 | ✗ | ✗ | ✓ (feat.) |
| tiberius | 0.12 | ✗ | ✗ | ✓ (feat.) |
| bb8 + bb8-tiberius | 0.9/0.16 | ✗ | ✗ | ✓ (feat.) |
| duckdb | 1.4 | ✗ | ✗ | ✓ (feat.) |
| tracing | 0.1 | ✗ | ✗ | ✓ |
| rust_decimal | 1 | ✗ | ✗ | ✓ |
| bigdecimal | 0.4 | ✗ | ✗ | ✓ |
| regex | 1 | ✗ | ✗ | ✓ |

### 6.3 Dépendances qui restent dans app/ uniquement

Ces dépendances ne sont pas extraites car elles sont spécifiques à Tauri :

- `tauri`, `tauri-plugin-opener`, `tauri-plugin-dialog`, `tauri-plugin-fs`, `tauri-plugin-updater`
- `keyring`, `argon2`, `rand` (vault/sécurité)
- `ed25519-dalek` (licence)
- `rust_xlsxwriter`, `arrow`, `parquet` (export pro)
- `reqwest` (AI BYOK pro)
- `tracing-appender`, `tracing-subscriber` (observabilité app)
- `csv`, `dirs` (utilitaires app)

---

## 7. Points d'attention et pièges

### 7.1 Pièges techniques

#### Tauri conf path

Le fichier `tauri.conf.json` référence le binaire Rust via un chemin relatif. Si tu déplaces le code dans `app/`, il faudra potentiellement mettre à jour `beforeBuildCommand` et `beforeDevCommand` dans la configuration Tauri, ainsi que le champ `build.beforeBuildCommand` qui peut pointer vers `cargo build`.

#### Re-exports et façade

Certains modules de QoreDB importent `crate::engine::*` via le re-export dans `engine/mod.rs`. Après extraction, il faudra créer un **module façade** dans `app/` qui re-exporte depuis les 3 crates pour minimiser le diff dans les fichiers `commands/*` :

```rust
// app/src/engine.rs (façade)
pub use qore_core::*;
pub use qore_sql::*;
pub use qore_drivers::*;
```

Cela permet de faire `use crate::engine::*` dans les commands sans tout réécrire d'un coup. Tu pourras nettoyer la façade progressivement.

#### SessionManager et SshTunnel

Le `SessionManager` utilise `SshTunnel` qui lance un processus `ssh -L`. Ce couplage est interne à qore-drivers, pas un problème. Mais si tu veux rendre SshTunnel optionnel plus tard (feature flag), il faudra le découpler du SessionManager avec un trait.

#### DuckDB et bundled

Le driver DuckDB utilise `features = ["bundled"]` qui compile DuckDB from source. C'est lent (~2 min) et lourd. Assure-toi que le feature flag `duckdb` est bien **opt-in** dans qore-drivers pour ne pas ralentir la compilation par défaut.

#### Conditional compilation dans les drivers

Les fichiers `drivers/mod.rs` et `lib.rs` de qore-drivers devront utiliser `#[cfg(feature = "...")]` pour conditionner chaque driver :

```rust
// qore-drivers/src/drivers/mod.rs
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "postgres")]
pub mod pg_compat;

#[cfg(feature = "mysql")]
pub mod mysql;

#[cfg(feature = "mongodb")]
pub mod mongodb;

// ... etc
```

#### SQLx et les features partagées

Les drivers PostgreSQL, MySQL et SQLite utilisent tous SQLx mais avec des features différentes. Dans le `Cargo.toml` de qore-drivers, SQLx doit être déclaré comme dépendance optionnelle avec toutes les features activées quand au moins un driver SQL est activé :

```toml
[dependencies]
sqlx = { workspace = true, optional = true }
```

### 7.2 Licences SPDX

Tous les fichiers de `engine/` sont marqués `Apache-2.0`. L'extraction ne change pas la licence. Vérifie que chaque fichier déplacé conserve son header SPDX intact.

> **✅ Rappel licensing :** Les crates qore-core, qore-sql et qore-drivers sont intégralement Apache-2.0. Le code Premium (BUSL-1.1) reste dans app/ (Diff, ERDiagram, profiling). Pas de mélange.

### 7.3 Compilation incrémentale

Le passage en workspace **améliore** la compilation incrémentale : modifier un fichier dans qore-core ne recompile pas les drivers si l'API publique n'a pas changé, et vice-versa. C'est un gain significatif pour le développement quotidien de QoreDB.

### 7.4 Ordre des re-exports dans lib.rs

Attention à l'ordre des `pub use` dans les `lib.rs` de chaque crate. Si deux crates exportent un type avec le même nom, le consommateur aura un conflit. Vérifie qu'il n'y a pas de collision de noms entre qore-core et qore-sql (normalement il n'y en a pas, les noms sont distincts).

---

## 8. Tests et validation

### 8.1 Tests unitaires existants

Plusieurs modules ont déjà des tests intégrés qui seront automatiquement transférés :

| Module | Tests | Description | Crate cible |
|---|---|---|---|
| `types.rs` | 1 test | SSH auth deserialization | qore-core |
| `sql_safety.rs` | 8 tests | Mutations, dangerous, split, read-only | qore-sql |
| `sql_generator.rs` | 4 tests | Quoting, insert, update, delete | qore-sql |
| `connection_url.rs` | 25+ tests | Tous les drivers, edge cases, erreurs | qore-sql |
| `query_manager.rs` | 3 tests | Register, finish, duplicate rejection | qore-drivers |

### 8.2 Stratégie de validation

1. **Tests unitaires :** `cargo test --workspace` — tous doivent passer
2. **Clippy :** `cargo clippy --workspace -- -D warnings` — zéro warning
3. **Tests d'intégration :** Lancer QoreDB avec `pnpm tauri dev` et tester une connexion réelle
4. **Tests manuels minimum :** Connexion PostgreSQL + MySQL + SQLite, exécution d'un SELECT, navigation dans le schéma, CRUD basique
5. **CI :** Vérifier que le pipeline GitHub Actions passe (si configuré)

### 8.3 Tests à ajouter (recommandé)

Profiter de l'extraction pour ajouter des tests manquants :

- **qore-core :** Tests de sérialisation/désérialisation de `Value`, `ConnectionConfig`, `Namespace`
- **qore-core :** Tests du `DriverRegistry` (register, get, list)
- **qore-drivers :** Tests unitaires du `SessionManager` (mock driver)
- **qore-drivers :** Tests du `SshTunnel` (au moins les cas d'erreur)

---

## 9. Roadmap vers QoreORM

L'extraction de qore-core est la **Phase 1** d'une roadmap en 4 phases vers QoreORM :

| Phase | Livrable | Délai estimé | Pré-requis |
|---|---|---|---|
| **Phase 1** | qore-core / qore-sql / qore-drivers extraits | 1-2 jours | Aucun |
| **Phase 2** | Query Builder Rust type-safe multi-dialecte | 3-6 mois | Phase 1 |
| **Phase 3** | ORM layer (macros derive, relations, hydratation) | 6-12 mois | Phase 2 |
| **Phase 4** | Bindings TypeScript via engine binaire (modèle Prisma) | 12-18 mois | Phase 3 |

### 9.1 Phase 2 — Query Builder (aperçu)

Un query builder type-safe qui compile vers du SQL multi-dialecte. Exemple d'API cible :

```rust
use qore_orm::prelude::*;

let users = Query::table("users")
    .select(["id", "name", "email"])
    .filter(col("age").gt(18))
    .order_by("name", Asc)
    .limit(10)
    .build(Dialect::Postgres);

// Génère :
// SELECT "id", "name", "email"
// FROM "users"
// WHERE "age" > 18
// ORDER BY "name" ASC
// LIMIT 10
```

Le `sql_generator.rs` déjà extrait dans qore-sql fournit les briques de base (quoting, formatage de valeurs) sur lesquelles le query builder s'appuiera.

### 9.2 Phase 3 — ORM layer (aperçu)

```rust
use qore_orm::prelude::*;

#[derive(Model)]
#[qore(table = "users")]
struct User {
    #[qore(primary_key, auto_increment)]
    id: i64,
    name: String,
    email: String,
    #[qore(nullable)]
    age: Option<i32>,
    #[qore(relation = "has_many")]
    posts: Vec<Post>,
}

// Usage
let users = User::find()
    .filter(User::age().gt(18))
    .with(User::posts())  // eager loading
    .limit(10)
    .exec(&db)
    .await?;
```

### 9.3 Avantage compétitif

Grâce aux crates extraits, QoreORM démarrerait avec un avantage que personne d'autre n'a :

- **9 drivers** prêts à l'emploi (Prisma en a 5 après 5 ans)
- **SQL + NoSQL** dans le même ORM (MongoDB + Redis natifs)
- **SSH tunneling** intégré (aucun ORM ne fait ça)
- **SQL safety analysis** intégrée (validation avant exécution)
- **Connection URL parsing** pour 6 schémas d'URL
- **Streaming** natif pour les gros résultats (tokio mpsc)

---

## 10. Annexes

### A. Checklist d'extraction

```
ÉTAPE 1 — Créer le Cargo workspace
  [ ] Créer src-tauri/crates/
  [ ] Déplacer src/ vers app/src/
  [ ] Déplacer build.rs vers app/build.rs
  [ ] Transformer Cargo.toml en workspace root
  [ ] Créer app/Cargo.toml
  [ ] cargo build OK
  [ ] pnpm tauri dev OK

ÉTAPE 2 — Extraire qore-core
  [ ] Créer crates/qore-core/
  [ ] Déplacer types.rs, error.rs, traits.rs, registry.rs
  [ ] Mettre à jour les imports dans qore-core
  [ ] Mettre à jour les imports dans app/
  [ ] Créer la façade engine.rs dans app/
  [ ] cargo test -p qore-core OK

ÉTAPE 3 — Extraire qore-sql
  [ ] Créer crates/qore-sql/
  [ ] Déplacer sql_safety.rs, sql_generator.rs, connection_url.rs
  [ ] Mettre à jour les imports
  [ ] cargo test -p qore-sql OK

ÉTAPE 4 — Extraire qore-drivers
  [ ] Créer crates/qore-drivers/
  [ ] Déplacer les 9 drivers + infrastructure
  [ ] Configurer les feature flags + #[cfg(feature)]
  [ ] Mettre à jour les imports
  [ ] cargo test --workspace OK

ÉTAPE 5 — Validation finale
  [ ] cargo clippy --workspace -- -D warnings OK
  [ ] pnpm tauri dev OK
  [ ] Tests manuels (PG + MySQL + SQLite) OK
  [ ] Vérifier les headers SPDX
  [ ] Commit final
```

### B. EngineError — Référence complète

Les 16 variantes d'erreur et leur usage :

| Variante | Champs | Usage |
|---|---|---|
| `ConnectionFailed` | `message: String` | Impossible de se connecter à la BDD |
| `AuthenticationFailed` | `message: String` | Credentials invalides |
| `SyntaxError` | `message: String` | Erreur de syntaxe SQL |
| `ExecutionError` | `message: String` | Erreur d'exécution de requête |
| `Timeout` | `timeout_ms: u64` | Dépassement de délai |
| `DriverNotFound` | `driver_id: String` | Driver inconnu |
| `SessionNotFound` | `session_id: String` | Session expirée ou invalide |
| `Cancelled` | _(aucun)_ | Requête annulée par l'utilisateur |
| `SslError` | `message: String` | Erreur SSL/TLS |
| `SshError` | `message: String` | Erreur tunnel SSH |
| `Internal` | `message: String` | Erreur interne inattendue |
| `NotSupported` | `message: String` | Feature non supportée par le driver |
| `TransactionError` | `message: String` | Erreur de transaction |
| `ValidationError` | `message: String` | Données invalides |
| `TooManyConcurrentQueries` | `current, limit: u32` | Limite de requêtes atteinte |
| `ResultTooLarge` | `rows, limit: u64` | Résultat trop volumineux |

Chaque variante a un constructeur helper (ex: `EngineError::connection_failed("msg")`) pour l'ergonomie.

### C. Commandes utiles pendant l'extraction

```bash
# Vérifier que tout compile sans lancer les tests
cargo check --workspace

# Lancer les tests d'un seul crate
cargo test -p qore-core
cargo test -p qore-sql
cargo test -p qore-drivers

# Tous les tests
cargo test --workspace

# Clippy strict
cargo clippy --workspace -- -D warnings

# Voir l'arbre de dépendances
cargo tree -p qore-core
cargo tree -p qore-drivers

# Taille des crates compilés
cargo build --workspace --release 2>&1 | tail -5
du -sh target/release/libqore_core.rlib
du -sh target/release/libqore_drivers.rlib

# Lancer QoreDB
pnpm tauri dev
```

### D. Imports — Mapping complet

Référence rapide pour le remplacement d'imports dans `app/` :

| Ancien import | Nouveau import |
|---|---|
| `crate::engine::types::*` | `qore_core::*` |
| `crate::engine::error::EngineError` | `qore_core::EngineError` |
| `crate::engine::error::EngineResult` | `qore_core::EngineResult` |
| `crate::engine::traits::DataEngine` | `qore_core::DataEngine` |
| `crate::engine::traits::StreamEvent` | `qore_core::StreamEvent` |
| `crate::engine::traits::StreamSender` | `qore_core::StreamSender` |
| `crate::engine::DriverRegistry` | `qore_core::DriverRegistry` |
| `crate::engine::SessionManager` | `qore_drivers::SessionManager` |
| `crate::engine::QueryManager` | `qore_drivers::QueryManager` |
| `crate::engine::sql_safety::*` | `qore_sql::safety::*` |
| `crate::engine::sql_generator::*` | `qore_sql::generator::*` |
| `crate::engine::connection_url::*` | `qore_sql::connection_url::*` |
| `crate::engine::ssh_tunnel::*` | `qore_drivers::ssh_tunnel::*` |
| `crate::engine::drivers::postgres::*` | `qore_drivers::drivers::postgres::*` |

Ou, avec la façade :

```rust
// app/src/engine.rs
pub use qore_core::*;
pub use qore_sql::{safety, generator, connection_url};
pub use qore_drivers::{SessionManager, QueryManager, ssh_tunnel, drivers};
```

→ Permet de garder `use crate::engine::*` dans les commands.
