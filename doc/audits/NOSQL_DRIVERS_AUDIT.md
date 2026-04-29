# Audit technique — Drivers NoSQL QoreDB

> **Date** : 2026-04-22
> **Périmètre** : drivers MongoDB et Redis (backend Rust + frontend React)
> **Objectif** : cartographier l'état actuel, identifier les frictions, proposer des axes d'amélioration priorisés.

---

## 1. Inventaire

**2 drivers NoSQL** dans `src-tauri/crates/qore-drivers/src/drivers/` :

| Driver | Fichier | Taille |
| --- | --- | --- |
| MongoDB | `mongodb.rs` | ~1900 lignes |
| Redis | `redis.rs` | ~1900 lignes |

Les 10 autres drivers (`postgresql.rs`, `mysql.rs`, `sqlite.rs`, `duckdb.rs`, `sqlserver.rs`, `cockroachdb.rs`, `mariadb.rs`, `pg_compat.rs`, `postgres_utils.rs`) sont SQL.

**Safety classifiers dédiés** :

- `src-tauri/crates/qore-drivers/src/mongo_safety.rs` — classification Read / Mutation / Unknown
- `src-tauri/crates/qore-drivers/src/redis_safety.rs` — classification Read / Mutation / Dangerous / Unknown

---

## 2. Architecture — friction SQL/NoSQL

Le trait `DataEngine` (défini dans `qore-core`) est **pensé SQL** et force MongoDB/Redis à un mapping artificiel :

| Trait SQL | MongoDB | Redis |
| --- | --- | --- |
| `list_namespaces` | databases réels | `db0..dbN` virtuels |
| `list_collections` | collections | clés typées (string/hash/list/set/zset/stream) |
| `describe_table` | sampling 100 docs | types prédéfinis par type Redis |
| `query_table` (FilterOperator SQL) | mappé vers BSON | mappé vers commandes Redis |

Les opérateurs universels (`EQ`, `NE`, `GT`, `GTE`, `LT`, `LTE`, `LIKE`, `IS NULL`) sont adaptés tant bien que mal, mais les primitives natives (aggregation pipeline MongoDB, pub/sub et consumer groups Redis) n'ont aucun angle d'attaque dans le trait.

**Verdict abstraction : 4/10 pour NoSQL.**

---

## 3. MongoDB — état fonctionnel

### 3.1 Ce qui marche bien

- **Connexion & sessions** : auto-détection replica set / mongos pour activer les transactions, timeouts 10s (connection + server selection), credentials percent-encodés.
- **Dual parsing** : JSON structuré (recommandé) ou shell syntax en fallback.

  Format JSON :

  ```json
  {
    "database": "db",
    "collection": "col",
    "operation": "find|insert|update|delete|createCollection|drop",
    "query": {...},
    "document": {...},
    "filter": {...},
    "update": {...}
  }
  ```

  Shell syntax :

  ```javascript
  db.collection.find({})
  db.collection.insertOne({})
  ```

- **Streaming** : batches de 500 documents via curseur avec support transactions et abort.
- **Pagination `query_table`** : skip/limit/tri multi-colonne/filtres universels + regex insensible sur champs string.
- **Inférence schéma** : sampling 100 documents, tous types BSON reconnus (null, boolean, int32/64, double, string, ObjectId, datetime, array, document, binary, mixed), `_id` identifié comme PK, `list_indexes()` exposé.
- **Conversion BSON ↔ Value** : exacte dans les deux sens, ObjectId auto-détecté.

### 3.2 Ce qui manque / est limité

| Feature | État |
| --- | --- |
| Aggregation pipeline complet | ⚠️ Parsing basique, pas de validation AST |
| `$out` / `$merge` | ✅ Bloqués (classés mutation) |
| `$text` / recherche full-text | ❌ Absent |
| `$regex` avec flags | ⚠️ Partiel (page-level only) |
| `bulkWrite` | ❌ Absent |
| `findOneAndUpdate` / `Replace` / `Delete` | ❌ Absent |
| Create/drop index UI | ❌ Lecture seule |
| Change Streams (watch) | ❌ Absent |
| Geo queries (`$near`, `$geoWithin`) | ❌ Absent |
| Options `createCollection` (capped, validator) | ❌ Absent |

---

## 4. Redis — état fonctionnel

### 4.1 Ce qui marche bien

- **Connexion** : `MultiplexedConnection` + `AtomicU16 current_db` pour tracker SELECT, URL `redis://` ou `rediss://` (TLS), credentials percent-encodés, timeout PING 10s.
- **Parsing commandes** : split whitespace-aware avec support quotes simples/doubles et escape sequences.
- **Rendus type-spécifiques** : chaque type Redis a sa méthode dédiée avec pagination native et colonnes adaptées.

  | Type Redis | Méthode | Pagination | Colonnes |
  | --- | --- | --- | --- |
  | String | `read_string` | — | `value` |
  | Hash | `read_hash_page` | HSCAN | `field`, `value` |
  | List | `read_list` | LRANGE offset/limit | `index`, `value` |
  | Set | `read_set_page` | SSCAN | `member` |
  | Sorted Set | `read_zset` | ZRANGE WITHSCORES | `member`, `score` |
  | Stream | `read_stream` | XRANGE en-mémoire | `id`, `data` (JSON) |

- **Browsing clés** : SCAN avec binary heap (top-K, O(1) mémoire) + safety limit 100K keys.
- **Listing DBs intelligent** : only non-empty databases listées via `CONFIG GET databases`, DB0 toujours affiché.
- **Conversion types** : Nil → Null, Integer/Double/Boolean directs, BulkString avec tentative JSON parse (fallback Text ou Bytes), Array récursif, Map → JSON Object.

### 4.2 Ce qui manque / est limité

| Feature | État |
| --- | --- |
| **Mutations via UI** | ❌ **Bloquant** — command-line uniquement |
| Lua script editor | ❌ EVAL fonctionne mais pas d'IDE |
| Pub/Sub (PUBLISH/SUBSCRIBE) | ❌ Absent |
| Consumer Groups (XGROUP, XREAD GROUP) | ❌ Absent |
| CLUSTER commands | ✅ Bloqués (dangerous) |
| ACL / MODULE | ✅ Bloqués (dangerous) |
| GEO commands | ❌ Absent |
| HyperLogLog, BITFIELD | ❌ Absent |
| UI TTL / EXPIRE | ❌ Command-line only |
| MEMORY DOCTOR/STATS | ❌ Absent |

---

## 5. Frontend React — dégradation UX

**Fichiers clés** :

- `src/components/Query/QueryPanel.tsx` — détection `isDocumentDatabase(dialect)`, streaming désactivé pour documents.
- `src/components/Editor/MongoEditor.tsx` — CodeMirror JSON basique, pas d'autocomplete spécifique MongoDB.
- `src/components/Editor/SQLEditor.tsx` — highlighting SQL + autocomplete table/column (SQL only).
- `src/components/DocumentEditorModal.tsx` — modal édition JSON (MongoDB), mode Code/Form.

| Aspect | SQL | MongoDB | Redis |
| --- | --- | --- | --- |
| Éditeur requête | CodeMirror SQL + autocomplete dialect-aware | CodeMirror JSON | Command-line shell |
| Templates / snippets | Riches (SELECT/INSERT…) | Basiques (find, insert) | Aucun |
| Édition in-place | Grille native | Modal Document | ❌ |
| Validation pré-envoi | Syntax SQL | ❌ Pas de validation JSON Mongo | ❌ |
| Autocomplete fields | oui | ❌ | ❌ |

**Verdict UX** : SQL 9/10, MongoDB 5/10, Redis 3/10. L'expérience NoSQL est clairement de seconde classe.

---

## 6. Sécurité

**Flux d'exécution** (`src-tauri/src/commands/query.rs`) :

1. Classification via `mongo_safety::classify()` ou `redis_safety::classify()`.
2. Interceptor pre-execute (safety rules built-in + custom).
3. Exécution.
4. Interceptor post-execute (audit + profiling).
5. Production enforcement : mutations en prod demandent confirmation, `FLUSHALL` / `CONFIG SET` toujours bloqués.

### 6.1 MongoDB safety

Classification en 3 catégories :

- **Read** : `find`, `findOne`, `aggregate` (sans `$out`/`$merge`), `count`, `distinct`, `listCollections`
- **Mutation** : `insert*`, `update*`, `delete*`, `createCollection`, `dropDatabase`
- **Unknown** : opérations non reconnues

Détection : parse JSON si format structuré, sinon pattern matching regex sur shell syntax, cas spécial `aggregate` + `$out`/`$merge` = mutation.

**Limites** :

- Ne détecte pas l'injection de champs dans les filtres.
- Ne valide pas les pipelines aggregation complexes.
- Pas de détection de `$where` ou expressions dangereuses.
- Parsing shell basique, bypassable.

### 6.2 Redis safety

Classification en 4 catégories :

- **Read** : GET, HGETALL, SCAN, SELECT, AUTH, XRANGE, etc.
- **Mutation** : SET, DEL, HSET, LPUSH, ZADD, XADD, MULTI/EXEC
- **Dangerous** : FLUSHALL, FLUSHDB, SHUTDOWN, CONFIG SET, SCRIPT FLUSH, ACL changes, CLUSTER operations
- **Unknown** : modules custom

Détection context-aware : split command + subcommand (e.g., `CONFIG SET`), match contre listes known, puis policy.

**Limites** :

- Pas de validation du contenu Lua (EVAL / EVALSHA).
- BITFIELD et opérations rares non classés.
- Pas de protection contre Slowlog / Client tracking abuse.

### 6.3 Audit log

- Requêtes MongoDB persistées brutes, **pas de redaction**.
- Commandes Redis persistées brutes, risque de fuite (mots de passe dans EVAL, credentials dans connection strings).

---

## 7. Axes d'amélioration priorisés

### 7.1 Court terme — impact fort, effort modéré

**1. Redis : UI de mutation (SET/DEL/HSET/LPUSH/ZADD)** — _Effort : 1-2 sem._

- Actuellement Redis est un **viewer** : toute mutation passe par la command-line. Plus gros manque UX identifié.
- Créer un `RedisEditorModal.tsx` analogue à `DocumentEditorModal`, avec édition key-value typée (validation selon type Redis).
- La classification safety est déjà en place, il ne manque que l'UI.
- **Impact** : +50% usability, Redis passe de viewer à éditeur.

**2. MongoDB : Aggregation pipeline first-class** — _Effort : 2-3 sem._

- L'aggregation représente ~80% des usages MongoDB en prod. Le parsing actuel est basique.
- Remplacer par un AST validator supportant `$match`, `$project`, `$group`, `$sort`, `$limit`, `$skip`, `$unwind`, `$lookup` (bloquer `$out`/`$merge` comme aujourd'hui).
- Streaming cursor sur résultats de pipeline.
- **Impact** : +40% couverture fonctionnelle MongoDB.

**3. Redaction dans l'audit log** — _Effort : 3-5 jours._

- Ajouter un hook `redact()` dans `interceptor.post_execute()` pour filtrer patterns sensibles (passwords dans EVAL, tokens dans connection strings, champs `password`/`token` dans documents MongoDB).
- **Impact** : conformité GDPR + réduction risque fuites audit.

**4. Éditeur MongoDB avec autocomplete fields** — _Effort : 1 sem._

- CodeMirror extensions : complétion sur noms de champs basée sur le schéma sampling existant.
- Validation JSON temps réel + highlight des opérateurs MongoDB (`$match`, `$gt`, etc.).
- **Impact** : +30% vélocité éditeur MongoDB.

### 7.2 Moyen terme

**5. MongoDB : Index management UI** — _Effort : 2 sem._
Création / drop d'index (unique, sparse, TTL) depuis l'UI. Critique pour les perfs.

**6. Redis : Lua script editor** — _Effort : 2 sem._
Langage Lua dans CodeMirror, command builder EVAL / EVALSHA, docs `redis.call` intégrées.

**7. MongoDB : `$text` et `$regex` natifs** — _Effort : 2 sem._
Débloque la recherche full-text, indispensable pour des cas d'usage content.

**8. MongoDB : `bulkWrite` + `findOneAnd*`** — _Effort : 1 sem._
Opérations atomiques manquantes.

### 7.3 Long terme — structurel

**9. Refactor `DataEngine` v2 — abstraction moins SQL-centric** — _Effort : 6-8 sem., breaking change._

- Nouveau modèle : `QueryOptions` trait générique au lieu de `FilterOperator` hardcodé SQL.
- `describe_table()` optionnel (tous les stores n'ont pas de schéma).
- Capabilities versionnées par driver (`cap.aggregation_pipeline`, `cap.pub_sub`, etc.).
- Builders d'opérations spécifiques par driver, au lieu de parsing JSON/shell générique.
- **Impact** : débloque les primitives natives NoSQL comme citoyens de première classe.
- **Risque** : élevé (touche tous les drivers), à planifier pour une release majeure.

**10. Redis : Consumer Groups & Pub/Sub** — _Effort : 3-4 sem._
Permet l'usage de Redis comme message queue / event bus, actuellement impossible.

**11. MongoDB : Change Streams** — _Effort : 4 sem._
Monitoring temps réel, push vers frontend via Tauri event emitter. Gros différenciateur vs DBeaver.

---

## 8. Synthèse

| Dimension | Score NoSQL | Note |
| --- | --- | --- |
| Qualité abstraction | 4/10 | Trait `DataEngine` force les NoSQL dans un moule SQL |
| Couverture MongoDB | 5/10 | CRUD OK, mais aggregation / indexes / change streams absents |
| Couverture Redis | 6/10 | Browsing riche, mais mutations UI absentes |
| UX MongoDB | 5/10 | Éditeur JSON basique, peu d'assistance |
| UX Redis | 3/10 | Command-line only pour toute mutation |
| Sécurité validation | 7/10 | Classification OK, injection non détectée |
| Sécurité dangerous ops | 8/10 | Bien couvert |
| Audit | 5/10 | Complet mais sans redaction |

**Les 3 quick wins à prioriser** :

1. UI mutations Redis
2. Aggregation pipeline MongoDB
3. Redaction audit log

Avec ~5 semaines d'effort cumulé, ils corrigent les frustrations les plus visibles.

**Le chantier de fond** : refactor `DataEngine` pour sortir du moule SQL. Sans ça, les features avancées (change streams, pub/sub, consumer groups) resteront toujours des hacks greffés à côté du trait principal.

---

## 9. Fichiers clés référencés

| Chemin | Rôle |
| --- | --- |
| `src-tauri/crates/qore-drivers/src/drivers/mongodb.rs` | Implémentation MongoDB complète |
| `src-tauri/crates/qore-drivers/src/drivers/redis.rs` | Implémentation Redis complète |
| `src-tauri/crates/qore-drivers/src/mongo_safety.rs` | Classification query MongoDB |
| `src-tauri/crates/qore-drivers/src/redis_safety.rs` | Classification query Redis |
| `src-tauri/src/commands/query.rs` | Flux exécution + safety checks |
| `src/components/Query/QueryPanel.tsx` | Frontend query execution |
| `src/components/Editor/MongoEditor.tsx` | MongoDB editor (JSON basique) |
| `src/components/DocumentEditorModal.tsx` | Édition document MongoDB |
| `doc/internals/UNIVERSAL_QUERY_INTERCEPTOR.md` | Architecture intercepteur |
| `doc/tests/DRIVER_LIMITATIONS.md` | Limitations documentées |
