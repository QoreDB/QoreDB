# Plan d'amélioration — Drivers NoSQL

> **Référence** : `doc/audits/NOSQL_DRIVERS_AUDIT.md` (2026-04-22)
> **Périmètre** : MongoDB + Redis, backend Rust (`qore-drivers`) et frontend React.
> **Usage** : checklist de suivi. Cocher `- [x]` au fur et à mesure. Chaque bloc est conçu pour être livrable indépendamment.

---

## Vue d'ensemble des phases

| Phase | Durée cible | Items | Objectif |
| --- | --- | --- | --- |
| 1 — Quick wins | ~5 sem. | 4 | Corriger les frustrations UX/sécurité les plus visibles |
| 2 — Extension fonctionnelle | ~7 sem. | 4 | Couvrir les opérations NoSQL manquantes |
| 3 — Structurel | ~13 sem. | 3 | Refondre l'abstraction + features différenciantes |
| 4 — Backlog robustesse | en continu | n | Items secondaires + dette technique |

**Définition de "done" par tâche** : code livré + tests unitaires/intégration + doc mise à jour (`doc/rules/DATABASES.md`, `doc/tests/DRIVER_LIMITATIONS.md`) + header SPDX + i18n (EN/FR) si UI.

---

## Phase 1 — Court terme (quick wins)

### 1.1 Redis : UI de mutation (SET/DEL/HSET/LPUSH/ZADD)

_Effort estimé : 1-2 semaines_

> **Décision d'architecture (2026-04-23)** : plutôt que d'ajouter N helpers Rust + N commandes Tauri, on construit les commandes Redis textuelles dans `src/lib/redisCommands.ts` et on les envoie via le `executeQuery` existant. Bénéfices : pipeline safety/audit/redaction déjà câblé (y compris la redaction Redis de 1.3), surface backend inchangée, code plus simple. Les helpers Rust typés restent listés pour une itération future s'ils deviennent nécessaires (ex. exposition depuis une API non-UI).

- [ ] **Backend — helpers de mutation typés** (reporté — non bloquant pour l'UI)
  - [ ] Ajouter `set_string(key, value, ttl)` dans `redis.rs`
  - [ ] Ajouter `delete_keys(keys: Vec<String>)` avec batch
  - [ ] Ajouter `set_hash_field(key, field, value)` et `delete_hash_field(key, field)`
  - [ ] Ajouter `push_list_item(key, value, side: Left|Right)` et `pop_list_item(key, side)`
  - [ ] Ajouter `set_zset_member(key, member, score)` et `remove_zset_member(key, member)`
  - [ ] Ajouter `add_set_member(key, member)` et `remove_set_member(key, member)`
  - [ ] Exposer via commandes Tauri dans `src-tauri/src/commands/`
- [x] **Frontend — command builder**
  - [x] Créer `src/lib/redisCommands.ts` (quote/escape + builders pour SET, DEL, HSET, HDEL, LPUSH/RPUSH, LPOP/RPOP, ZADD, ZREM, SADD, SREM, EXPIRE, PERSIST)
- [ ] **Frontend — RedisEditorModal**
  - [x] Créer `src/components/Editor/RedisEditorModal.tsx`
  - [x] Variants par type Redis (string/hash/list/set/zset)
  - [x] Validation client-side (score numérique pour ZSET, clé/champ/membre requis)
  - [ ] Bouton "Éditer" sur chaque cellule du grid Redis (hash/list/set/zset) — dépend d'un grid Redis dédié, pas encore implémenté
  - [x] Bouton "Supprimer clé" avec confirmation (via `DangerConfirmDialog`)
  - [x] Bouton "Nouvelle clé" dans le `DatabaseBrowser` (header, visible si `driver === Redis && !readOnly`)
- [x] **Safety**
  - [x] Commandes générées (SET/DEL/HSET/HDEL/LPUSH/RPUSH/LPOP/RPOP/ZADD/ZREM/SADD/SREM/EXPIRE/PERSIST) déjà classées `Mutation` dans `redis_safety.rs`
  - [x] Confirmation production via `DangerConfirmDialog` + flag `acknowledgedDangerous`
- [ ] **Tests**
  - [ ] Tests unitaires sur `redisCommands.ts` (quoting / escape / builders) — bloqué : pas d'infra de tests JS dans le projet (pas de Vitest/Jest configuré). À prévoir en amont d'une PR dédiée "infra de tests frontend"
  - [ ] Tests E2E sur modal (Playwright ou équivalent)
- [ ] **Doc**
  - [x] Entrée dans `doc/rules/FEATURES.md` section "Edition des données > NoSQL"
  - [ ] Screenshot dans `doc/rules/FEATURES.md` (à faire manuellement une fois l'UI testée en dev)
- [x] **i18n**
  - [x] Clés dans `src/locales/en.json` et `fr.json` (namespace `redis.*` + `environment.mutationConfirmGeneric`)
  - [x] Propagées vers `de/es/ja/ko/pt-BR/ru/zh-CN` (34 clés `redis.*` + `environment.mutationConfirmGeneric` dans chaque locale)

**Done when** : un utilisateur peut créer, éditer, supprimer des valeurs Redis de tous types sans passer par la command-line.

**État (2026-04-23)** : MVP livré — bouton "Nouvelle clé" dans le `DatabaseBrowser`, modal multi-mode, confirmation production, i18n complète (9 locales). Restent : édition in-cell (dépend d'un grid Redis dédié), tests unitaires (bloqués par l'absence d'infra Vitest/Jest), screenshot, helpers Rust typés si besoin d'une API non-UI.

---

### 1.2 MongoDB : Aggregation pipeline first-class ✅

_Effort estimé : 2-3 semaines_

- [x] **Backend — AST validator**
  - [x] Créer `src-tauri/crates/qore-drivers/src/mongo_pipeline.rs`
  - [x] Parser JSON → `Vec<PipelineStage>` typé (`StageKind` + `PipelineStage` + `ValidatedPipeline`)
  - [x] Valider les stages autorisés : `$match`, `$project`, `$group`, `$sort`, `$limit`, `$skip`, `$unwind`, `$lookup`, `$count`, `$addFields`, `$replaceRoot`, `$set`, `$unset`, `$bucket`, `$facet`, `$graphLookup`, `$sortByCount`, `$sample`, `$redact`, `$geoNear`, `$out`, `$merge`…
  - [x] Rejeter explicitement `$out`/`$merge` non-terminal et les opérateurs dangereux `$function`, `$accumulator`, `$where` (scan récursif jusqu'à `MAX_SCAN_DEPTH = 64`)
  - [x] Valider que chaque stage a exactement un opérateur préfixé `$`, cap à `MAX_PIPELINE_STAGES = 50`
- [x] **Backend — exécution first-class**
  - [x] Branche `"aggregate"` dédiée dans `drivers/mongodb.rs` (n'est plus redirigée vers `find`)
  - [x] Validation via `validate_pipeline` avant dispatch, conversion stage → `bson::Document`, iteration cursor (avec/sans session transactionnelle)
  - [x] Support abort/cancel via le `Abortable` existant qui englobe l'exécution
- [x] **Safety**
  - [x] `mongo_safety::classify_aggregate` délègue à l'AST validator ; échec = `Unknown` (fail-closed)
  - [x] `$out`/`$merge` classés `Mutation` (validé par tests)
- [x] **Frontend**
  - [x] Templates dans `mongo-constants.ts` : `aggregate` (group), `aggregateTopN`, `aggregateLookup` exposés dans le `QueryPanelToolbar`
  - [x] Highlight syntaxique des opérateurs `$*` (livré avec 1.4 via `mongoHighlight.ts`)
- [x] **Tests**
  - [x] 20 tests dans `mongo_pipeline` (cas nominaux, rejets, `$out` en milieu, `$where` imbriqué, profondeur max, etc.)
  - [x] 6 tests d'intégration dans `mongo_safety` (aggregate normal/merge/function/where/out-middle/stage-inconnu)
- [x] **Doc**
  - [x] `doc/tests/DRIVER_LIMITATIONS.md` : section aggregation MongoDB + 3 exemples (group, top N, lookup)

**Done when** : les aggregations complexes (lookup multi-collections, groupBy, unwind) s'exécutent avec validation stricte. **✅ livré (2026-04-23)**

**État (2026-04-23)** : AST validator + branche aggregate + safety + snippets frontend + doc livrés, 32 tests verts côté Rust. Restent optionnels : streaming par batches de 500 documents (Phase 2 si besoin, le cursor actuel reste asynchrone via `try_next`), écriture dédiée `execute_stream()` pour aggregate (aujourd'hui route via `execute_query`).

---

### 1.3 Redaction dans l'audit log ✅

_Effort estimé : 3-5 jours_

- [x] **Backend — hook redact**
  - [x] ~~Ajouter trait `QueryRedactor` dans `src-tauri/src/interceptor/`~~ — remplacé par `redact_query(query, driver_id)` qui dispatche selon le driver (plus simple, même résultat)
  - [x] Implémentation SQL (URI de connexion, `password=/token=/api_key=`, littéraux `'...'`)
  - [x] Implémentation MongoDB (champs `password`, `passwd`, `secret`, `token`, `api_key`, `credentials`, `authorization`, `auth` + URI `mongodb(+srv)://`)
  - [x] Implémentation Redis (args `AUTH`, `CONFIG SET requirepass/masterauth/…`, corps `EVAL`/`EVALSHA`, clauses `ACL SETUSER` `>`/`<`/`#`/`!`)
  - [x] Appel dans `interceptor.post_execute()` avant persistance (via `AuditLogEntry::new` + `ProfilingStore::record_slow_query`)
- [x] **Configuration**
  - [x] Liste de patterns configurables dans settings (`redaction_patterns`)
  - [x] Toggle global (`redact_enabled`)
- [x] **Tests**
  - [x] Tests unitaires par redactor (20 tests dans `redaction.rs`)
  - [x] Test d'intégration : vérifier qu'une query avec password est stockée caviardée (3 tests dans `types.rs` couvrant SQL/Mongo/Redis)
- [x] **Doc**
  - [x] `doc/audits/GDPR_AUDIT.md` : mise à jour section audit log
  - [x] `doc/security/PRODUCTION_SAFETY.md` : ajout du mécanisme

**Done when** : aucun secret en clair dans l'audit log après exécution d'une query sensible. **✅ livré (2026-04-22)**

---

### 1.4 Éditeur MongoDB avec autocomplete fields

_Effort estimé : 1 semaine_

- [x] **Frontend — CodeMirror extensions**
  - [x] Créer `src/components/Editor/extensions/mongoCompletion.ts`
  - [x] Source d'autocomplete depuis le schéma sampling existant (`describeTable` via `useSchemaCache`)
  - [x] Complétion sur : noms de champs, opérateurs (`$gt`, `$in`, `$regex`, `$match`, `$group`…), methods (`find`, `insertOne`, `aggregate`, `updateMany`…), noms de collections après `db.`
- [x] **Validation JSON temps réel**
  - [x] Linter JSON dans `MongoEditor.tsx` via `src/components/Editor/extensions/mongoLint.ts` (+ dépendance `@codemirror/lint`)
  - [x] Tolère le style mongosh (commentaires `//` et `/* */`, strings single-quotes, trailing commas)
  - [x] Highlight erreur avec diagnostic inline + marker gutter avant envoi
- [x] **Highlighting opérateurs**
  - [x] ViewPlugin `mongoOperatorHighlight` (`mongoHighlight.ts`) qui décore les clés `"$xxx":` avec la classe `cm-mongo-operator`
  - [x] Theme CSS dans `MongoEditor.tsx` (couleur distincte dark/light)
- [ ] **Tests**
  - [ ] Tests unitaires extension autocomplete/lint — bloqué : pas d'infra Vitest/Jest dans le projet
- [x] **i18n**
  - [x] Clé `mongo.jsonError` ajoutée dans les 9 locales (en/fr/de/es/ja/ko/pt-BR/ru/zh-CN)

**Done when** : un utilisateur tape `{ "field":` et voit les noms de champs existants dans sa collection.

**État (2026-04-23)** : livré — autocomplete multi-niveau (collections, méthodes, opérateurs `$*`, champs de la collection résolue), linter JSON-ish tolérant, highlight des opérateurs, i18n complète. Tests unitaires restent bloqués par l'absence d'infra JS.

---

## Phase 2 — Moyen terme

### 2.1 MongoDB : Index management UI ✅

_Effort estimé : 2 semaines_

- [x] **Backend**
  - [x] Branche `createIndex` dans `drivers/mongodb.rs` (via `IndexModel` + `IndexOptions`, support txn/non-txn)
  - [x] Branche `dropIndex` dans `drivers/mongodb.rs` (via `db.run_command(doc! { "dropIndexes": ... })`, rejette `_id_` et `*`)
  - [x] `listIndexes` déjà exposé en lecture (classé `Read` dans `mongo_safety.rs`)
  - [x] Options supportées : `name`, `unique`, `sparse`, `expireAfterSeconds` (TTL), `partialFilterExpression`
- [x] **Frontend — dialog création index**
  - [x] `src/components/Schema/IndexDialog.tsx` (~320 lignes)
  - [x] Sélection champs + direction (1 / -1 / `text` / `2dsphere`)
  - [x] Options avancées (unique, sparse, TTL, partialFilterExpression)
  - [x] Preview JSON live de la commande générée
  - [x] Confirmation production via `DangerConfirmDialog` (avec saisie du nom de collection en prod)
- [x] **Frontend — liste des indexes**
  - [x] Section "Indexes" étendue dans `TableBrowser.tsx` (TableInfoPanel) pour MongoDB
  - [x] Bouton "Nouvel index" visible si `driver === Mongodb && !readOnly`
  - [x] Actions drop par ligne (Trash) en excluant `_id_`
  - [x] État vide dédié Mongo + refresh via `schemaCache.invalidateTable` + `getTableSchema`
- [x] **Safety**
  - [x] `createIndex/createIndexes` + `dropIndex/dropIndexes` classés `Mutation` (JSON et shell)
  - [x] `listIndexes/.getIndexes()/.indexes()` classés `Read`
  - [x] 6 nouveaux tests dans `mongo_safety` (18 total verts)
- [ ] **Tests d'intégration**
  - [ ] Tests de création/drop d'index côté driver — bloqué : pas d'infra d'intégration MongoDB dans le projet (runtime tokio + serveur mongo requis), à traiter en amont d'une PR "infra d'intégration NoSQL"
- [x] **Doc**
  - [x] `doc/tests/DRIVER_LIMITATIONS.md` : section "Index management" MongoDB + 2 exemples (createIndex composite unique, dropIndex)
- [x] **i18n**
  - [x] Namespace `mongoIndex` ajouté dans les 9 locales (en/fr/de/es/ja/ko/pt-BR/ru/zh-CN)

**Done when** : création d'un index unique composite depuis l'UI sans toucher à la command-line. **✅ livré (2026-04-24)**

**État (2026-04-24)** : livré — backend `createIndex`/`dropIndex` + safety classifier + IndexDialog frontend + intégration TableBrowser + doc + i18n complète (9 locales, 36 clés chacune). Restent optionnels : tests d'intégration Mongo (bloqués par l'absence d'infra) et un onglet "stats" par index (non demandé dans le plan initial).

---

### 2.2 Redis : Lua script editor ✅

_Effort estimé : 2 semaines_

> **Décision d'architecture (2026-04-24)** : on réutilise la même approche que 1.1 — les commandes Redis (EVAL, EVALSHA, SCRIPT LOAD) sont construites textuellement côté frontend dans `src/lib/redisCommands.ts` et envoyées via `executeQuery` existant. Zéro helper Rust, zéro commande Tauri dédiée, réutilisation de tout le pipeline safety/audit/redaction déjà en place.

- [x] **Frontend — LuaScriptEditor**
  - [x] `src/components/Editor/LuaScriptEditor.tsx` (CodeMirror + `@codemirror/legacy-modes/mode/lua` via `StreamLanguage`)
  - [x] Snippets autocomplete (`redis.call`, `redis.pcall`, `redis.status_reply`, `redis.error_reply`, `redis.sha1hex`, `cjson.encode`/`decode`, `KEYS[1]`, `ARGV[1]`)
  - [x] Mod-Enter pour exécuter, theme dark/light
- [x] **Frontend — LuaScriptModal**
  - [x] `src/components/Editor/LuaScriptModal.tsx` (wrapper modal)
  - [x] Input dédié pour `KEYS` et `ARGV` (textareas une valeur par ligne + compteur live)
  - [x] Affichage du dernier résultat et du SHA chargé
  - [x] Confirmation production via `DangerConfirmDialog` + `acknowledgedDangerous`
- [x] **Frontend — command builders**
  - [x] `buildEvalScript({ script, keys, args })`, `buildEvalSha({ sha, keys, args })`, `buildScriptLoad(script)` dans `redisCommands.ts`
  - [x] Validation : script non vide, SHA hex 40 chars, quoting des KEYS/ARGV
- [x] **Backend** — _hors périmètre par décision d'archi_
  - [x] EVAL/EVALSHA/FCALL déjà classés `Mutation` dans `redis_safety.rs:97` (aucune modif nécessaire)
  - [x] SCRIPT FLUSH/KILL déjà classés `Dangerous` (`redis_safety.rs:47-56`)
- [x] **UI**
  - [x] Bouton "Script Lua" (icône `FileCode`) dans `DatabaseBrowser` header si `driver === Redis && !readOnly`
  - [x] Bouton "Charger (SCRIPT LOAD)" + "Exécuter (EVAL)" + "Exécuter via SHA" (conditionnel)
  - [ ] Bibliothèque de scripts sauvegardés — **non livré V1** (localStorage/Query Library réutilisable mais pas câblé ; report Phase 4 si besoin utilisateur)
- [x] **Safety**
  - [x] EVAL reste `Mutation`, SCRIPT FLUSH/KILL reste `Dangerous`
  - [x] Warning frontend `detectDangerousLuaCalls()` : regex best-effort sur `FLUSHALL`/`FLUSHDB`/`SHUTDOWN`/`CONFIG`/`SCRIPT FLUSH`/`DEBUG SLEEP` à l'intérieur de `redis.call/pcall`
- [x] **i18n**
  - [x] Namespace `redisLua` (20 clés) dans les 9 locales (en/fr/de/es/ja/ko/pt-BR/ru/zh-CN)
- [x] **Doc**
  - [x] `doc/tests/DRIVER_LIMITATIONS.md` : section "Lua scripting" Redis (classification, exemples EVAL/SCRIPT LOAD/EVALSHA, note sur le warning best-effort)

**Done when** : écriture et exécution d'un script Lua avec KEYS/ARGV, résultat affiché dans le viewer. **✅ livré (2026-04-24)**

**État (2026-04-24)** : livré — modal accessible depuis le `DatabaseBrowser` Redis, éditeur CodeMirror Lua avec snippets, builders `EVAL`/`EVALSHA`/`SCRIPT LOAD`, détection regex des commandes dangereuses, confirmation production, i18n complète. Bibliothèque de scripts reportée (non demandée explicitement par un utilisateur et `queryLibrary.ts` est réutilisable tel quel si besoin).

---

### 2.3 MongoDB : `$text` et `$regex` natifs ✅

_Effort estimé : 2 semaines_

> **Décision d'archi (2026-04-24)** : l'utilisateur a exigé "surtout pas de `NotSupported`". Les deux nouveaux opérateurs `Regex` et `Text` sont donc implémentés nativement dans **tous les drivers** (PostgreSQL, CockroachDB, MySQL, MariaDB, SQLite, DuckDB, SQL Server, MongoDB), avec des fallbacks pragmatiques quand le moteur n'a pas de primitive (SQL Server sans CLR, SQLite hors FTS5, DuckDB).

- [x] **Core types (Rust + TS)**
  - [x] `FilterOperator` étendu avec `Regex` et `Text` (reste `Copy`)
  - [x] Nouveau struct `FilterOptions { regex_flags, text_language }` avec `serde(default, skip_serializing_if)` pour garder la compat on-wire
  - [x] `ColumnFilter` gagne `options: FilterOptions`
  - [x] `TableIndex` gagne `index_type: Option<String>` (btree/hash/gin/gist/fulltext/text/2dsphere)
  - [x] Miroir TS dans `src/lib/tauri.ts`
- [x] **Backend — PostgreSQL / CockroachDB** (`pg_compat.rs`)
  - [x] `Regex` → `~` ou `~*` selon `flags.contains('i')`
  - [x] `Text` → `to_tsvector('<lang>', col::text) @@ plainto_tsquery('<lang>', $n)`
  - [x] Requête d'index étendue avec `JOIN pg_am` pour capturer `amname`
- [x] **Backend — MySQL / MariaDB** (`mysql.rs`)
  - [x] `Regex` → `col REGEXP ?` (préfixe `(?i)` si flag `i`)
  - [x] `Text` → `MATCH(col) AGAINST(? IN NATURAL LANGUAGE MODE)`
  - [x] Requête d'index étendue avec `INDEX_TYPE` (BTREE/HASH/FULLTEXT/SPATIAL → lowercase)
- [x] **Backend — SQLite** (`sqlite.rs`)
  - [x] `Regex` → `col REGEXP ?` (nécessite UDF `REGEXP` chargée ; erreur claire sinon)
  - [x] `Text` → fallback `col LIKE '%?%'` (FTS5 n'est pas column-level)
- [x] **Backend — DuckDB** (`duckdb.rs`)
  - [x] `Regex` → `regexp_matches(col::VARCHAR, ?[, flags])`
  - [x] `Text` → fallback `col::VARCHAR ILIKE '%?%'`
- [x] **Backend — SQL Server** (`sqlserver.rs`)
  - [x] `Regex` → `PATINDEX('%?%', CAST(col AS NVARCHAR(MAX))) > 0` (flags ignorés côté serveur)
  - [x] `Text` → `CONTAINS(col, '"?"')` (nécessite full-text catalog)
- [x] **Backend — MongoDB** (`mongodb.rs`)
  - [x] `Regex` → `{ $regex: pattern, $options: flags }` (flags filtrés à `imxs`)
  - [x] `Text` → top-level `{ $text: { $search: query, $language?: lang } }` (nom de colonne ignoré, c'est une limitation de `$text`)
  - [x] `index_type` extrait depuis `IndexModel.keys` via helper `infer_mongo_index_type` (text, 2dsphere, 2d, hashed)
- [x] **Frontend — query builder**
  - [x] `GridColumnFilter` refactoré : dropdown opérateur (like/eq/neq/regex/text) + input + input flags conditionnel pour regex
  - [x] `DataGrid.handleColumnFiltersChange` adapté pour emballer `operator`+`value`+`options` dans `ColumnFilter`
- [ ] **Frontend — warning index text** (déférée V1)
  - [ ] Bannière "pas d'index text/fulltext sur cette colonne" — nécessite lookup via `useSchemaCache` + reducer sur `index_type`, à brancher quand une UX dédiée sera validée
- [ ] **Score `$meta: "textScore"`** (déférée V2)
  - [ ] Propagation nécessite extension de `QueryResult`/`ColumnInfo` pour colonnes virtuelles — trop d'impact pour cette itération, reporté
- [x] **i18n**
  - [x] Clés `grid.filterPlaceholderRegex`, `grid.filterPlaceholderText`, `grid.filterRegexFlagsHint`, `grid.filterOp.{like,eq,neq,regex,text}` dans les 9 locales
- [x] **Doc**
  - [x] `doc/tests/DRIVER_LIMITATIONS.md` : nouvelle section "Filter operators" sous "Common behavior" avec table per-driver

**Done when** : recherche full-text depuis la barre de filtre du DataGrid fonctionne. **✅ livré (2026-04-24)**

**État (2026-04-24)** : livré — ajout des opérateurs `Regex` et `Text` à l'abstraction cross-driver, implémentation native dans les 8 drivers SQL/NoSQL sans aucun `NotSupported`, UI filter bar étendue, i18n complète, doc comparative. Tests d'intégration par moteur : bloqués par l'absence d'infra (pas de serveurs réels dans CI), mais `cargo test` continue à passer sur les 97 tests unitaires + la traduction FilterOperator reste exhaustive (match checked par le compilateur).

---

### 2.4 MongoDB : `bulkWrite` + `findOneAnd*` ✅

_Effort estimé : 1 semaine_

- [x] **Backend**
  - [x] Branche `"bulkwrite"` dans `drivers/mongodb.rs` — parse `operations: [...]` (insertOne/updateOne/updateMany/replaceOne/deleteOne/deleteMany), pin chaque `WriteModel` sur le namespace payload, appel `Client::bulk_write` (3.x)
  - [x] Branche `"findoneandupdate" | "findoneandreplace" | "findoneanddelete"` unifiée — parse `filter`, `update`/`replacement` selon le kind, support `options.returnDocument` ("before"/"after")
  - [x] Support options `returnDocument: Before|After` (before par défaut, cohérent avec MongoDB)
  - [x] Retour bulkWrite : 1 row avec `inserted_count/matched_count/modified_count/deleted_count/upserted_count`
  - [x] Retour findOneAnd* : 0 ou 1 row avec le document (avant/après selon option, toujours le doc supprimé pour delete)
- [x] **Safety**
  - [x] `bulkwrite` déjà classé `Mutation` dans `mongo_safety.rs:110`
  - [x] `findoneandupdate/replace/delete` déjà classés `Mutation` dans `mongo_safety.rs:111-113`
- [x] **Frontend**
  - [x] Snippets `bulkWrite`, `findOneAndUpdate`, `findOneAndReplace`, `findOneAndDelete` ajoutés dans `MONGO_TEMPLATES` (format JSON QoreDB directement exécutable)
  - [x] 4 nouvelles entrées dans le `QueryPanelToolbar` Templates select
- [ ] **Tests** — non livrés
  - [ ] Tests unitaires bulkWrite avec mix insert/update/delete : bloqués par absence d'infra d'intégration MongoDB (même limitation que 2.1)
- [x] **Doc**
  - [x] `doc/tests/DRIVER_LIMITATIONS.md` : nouvelle section "Bulk writes and atomic find-and-modify" avec 2 exemples

**Done when** : un bulkWrite de 1000 opérations atomiques s'exécute et retourne le résumé par type. **✅ livré (2026-04-24)**

**État (2026-04-24)** : livré — `bulkWrite` (6 kinds) + `findOneAndUpdate/Replace/Delete` avec `returnDocument` exposés au niveau du driver, cohérence avec le path de mutation existant (transaction-aware, production confirm), snippets frontend directement exécutables. Tests d'intégration par serveur MongoDB réel bloqués par l'absence d'infra (même cas que 2.1).

---

## Phase 3 — Long terme (structurel)

### 3.1 Refactor `DataEngine` v2 — abstraction moins SQL-centric

_Effort estimé : 6-8 semaines. **Breaking change — à planifier pour release majeure**_

- [ ] **Design**
  - [x] Rédiger RFC dans `doc/internals/RFC_DATAENGINE_V2.md` _(draft livré 2026-04-24 — basé sur les pain points concrets relevés en Phase 1+2 : FilterOperator SQL-centric, dispatch MongoDB par string, TableSchema mal adapté au NoSQL, pas de stream d'events, Value lossy pour BSON/UUID, capabilities non-extensibles)_
  - [ ] Review par les mainteneurs des drivers SQL + NoSQL
  - [ ] Plan de migration versionné (v1 + v2 coexistent pendant une release) — proposé dans la RFC § 5
  - [ ] Prototype SQLite pour valider le design avant migration des autres drivers
- [ ] **Core — nouveaux traits**
  - [ ] `QueryOptions` générique (remplace `FilterOperator` hardcodé)
  - [ ] `DriverCapabilities` versionnées : `cap.aggregation_pipeline`, `cap.pub_sub`, `cap.change_streams`, `cap.consumer_groups`
  - [ ] `describe_table()` optionnel (retour `Option<TableSchema>`)
  - [ ] Builders d'opérations par driver (au lieu de parsing générique)
- [ ] **Migration drivers SQL**
  - [ ] PostgreSQL, MySQL, SQLite, DuckDB, SQL Server, CockroachDB, MariaDB, pg_compat
  - [ ] Tests de non-régression pour chaque
- [ ] **Migration drivers NoSQL**
  - [ ] MongoDB : aggregation comme first-class citizen
  - [ ] Redis : commandes natives sans passer par pseudo-SQL
- [ ] **Frontend**
  - [ ] Adapter `src/lib/tauri.ts` (types capabilities)
  - [ ] UI conditionnelle selon capabilities (ex: onglet "Change Streams" si `cap.change_streams`)
- [ ] **Doc**
  - [ ] Réécriture `doc/internals/` sur l'abstraction
  - [ ] Guide de migration pour les drivers tiers (si plugin system)

**Done when** : les drivers NoSQL implémentent leurs primitives natives sans mapping SQL forcé, et les drivers SQL fonctionnent toujours.

---

### 3.2 Redis : Consumer Groups & Pub/Sub

_Effort estimé : 3-4 semaines. **Dépend de 3.1 pour l'élégance**_

- [ ] **Backend — Consumer Groups**
  - [ ] Commandes XGROUP CREATE/DESTROY/SETID
  - [ ] XREADGROUP avec auto-ack optionnel
  - [ ] XACK, XCLAIM, XPENDING
  - [ ] XINFO GROUPS / CONSUMERS
- [ ] **Backend — Pub/Sub**
  - [ ] PUBLISH (mutation)
  - [ ] SUBSCRIBE / PSUBSCRIBE via Tauri events (push vers frontend)
  - [ ] Gestion du lifecycle des subscribers (cleanup sur disconnect)
- [ ] **Frontend — Consumer Groups UI**
  - [ ] Onglet "Streams" dans le panneau Redis
  - [ ] Liste des groupes d'un stream
  - [ ] Création de groupe + dialog
  - [ ] Consommation en direct avec affichage progressif
- [ ] **Frontend — Pub/Sub UI**
  - [ ] Onglet "Pub/Sub" dans le panneau Redis
  - [ ] Input pour publier, liste des messages reçus
  - [ ] Subscribe avec pattern matching
- [ ] **Safety**
  - [ ] XGROUP DESTROY classé `Mutation`
  - [ ] PUBLISH classé `Mutation`
- [ ] **Tests**
  - [ ] Tests E2E avec docker redis 7+

**Done when** : Redis peut être utilisé comme message queue complet (publish + consumer groups) depuis QoreDB.

---

### 3.3 MongoDB : Change Streams

_Effort estimé : 4 semaines. **Dépend de 3.1**_

- [ ] **Backend**
  - [ ] Helper `watch_collection(db, col, pipeline, options)` retournant un stream
  - [ ] Helper `watch_database(db, pipeline, options)`
  - [ ] Bridge Tauri events pour push vers frontend
  - [ ] Gestion resumeToken pour reprise après reconnexion
- [ ] **Frontend**
  - [ ] Créer `src/components/ChangeStreams/ChangeStreamPanel.tsx`
  - [ ] Affichage temps réel des events (insert/update/delete/replace)
  - [ ] Filtre par type d'operation
  - [ ] Export des events vers fichier
- [ ] **Safety**
  - [ ] Classé `Read` (pas de mutation)
  - [ ] Timeout/limite de durée configurable
- [ ] **Performance**
  - [ ] Backpressure : drop events si frontend en retard
- [ ] **Doc**
  - [ ] Prérequis : replica set ou sharded cluster

**Done when** : un utilisateur peut "watcher" une collection et voir les mutations en temps réel, avec resume après déconnexion.

---

## Phase 4 — Backlog robustesse (en continu)

### 4.1 Sécurité — détection d'injection et validation avancée

- [ ] **MongoDB — injection detection**
  - [ ] Détecter les champs utilisateur dans les filtres (audit trail)
  - [ ] Warning sur `$where` (code JS arbitraire)
  - [ ] Rejeter `$expr` avec functions dangereuses
- [ ] **MongoDB — validation aggregation complexe**
  - [ ] Limiter profondeur de `$lookup` récursifs
  - [ ] Limiter nombre de stages (ex: 50 max)
- [ ] **Redis — validation Lua**
  - [ ] Regex detecting FLUSHALL/SHUTDOWN/CONFIG dans EVAL
  - [ ] Warning user avant exécution
- [ ] **Redis — classification étendue**
  - [ ] Classer BITFIELD, BITCOUNT, BITOP
  - [ ] Classer Slowlog commands (protéger contre abuse)
  - [ ] Classer CLIENT commands (TRACKING, KILL)

### 4.2 MongoDB — features restantes

- [ ] **Options `createCollection`** : capped, size, max, validator JSON schema, collation
- [ ] **Geo queries** : `$near`, `$nearSphere`, `$geoWithin`, `$geoIntersects` + index `2dsphere`
- [ ] **Transactions UI** : onglet dédié pour démarrer/commit/rollback (pas seulement auto-detect)
- [ ] **Explain plans** : intégration dans le Query Panel (déjà listé dans v3.md)

### 4.3 Redis — features restantes

- [ ] **GEO commands** : GEOADD, GEOSEARCH, GEODIST, GEOHASH avec carte affichée
- [ ] **HyperLogLog** : PFADD, PFCOUNT, PFMERGE
- [ ] **BITFIELD / BITOP / BITCOUNT** : éditeur dédié pour manipulation bit-level
- [ ] **TTL UI** : bouton "Définir expiration" sur chaque clé
- [ ] **MEMORY commands** : MEMORY USAGE, MEMORY STATS, MEMORY DOCTOR avec dashboard
- [ ] **Persistance** : BGSAVE, LASTSAVE, DEBUG avec confirmation prod

### 4.4 Frontend — UX NoSQL

- [ ] **Templates / snippets MongoDB** : bibliothèque (find+sort, aggregate group, lookup, upsert)
- [ ] **Templates / snippets Redis** : bibliothèque par type de structure
- [ ] **Édition in-place Redis** : saisie directe dans le grid sans passer par modal (optionnel)
- [ ] **Affichage Stream Redis** : formatage spécifique des IDs (`1234567890-0` → date lisible)
- [ ] **Affichage ObjectId MongoDB** : tooltip avec timestamp décodé

### 4.5 Documentation

- [ ] Mettre à jour `CLAUDE.md` : chemin réel des drivers (`src-tauri/crates/qore-drivers/src/drivers/`)
- [ ] Compléter `doc/tests/DRIVER_LIMITATIONS.md` avec les items "absent" du rapport
- [ ] Ajouter une section NoSQL dans `doc/rules/DATABASES.md`
- [ ] Diagramme d'architecture dans `doc/internals/` montrant le flux query NoSQL

---

## Suivi d'avancement

### Dashboard rapide

| Phase | Items done | Items total | % |
| --- | --- | --- | --- |
| Phase 1 | 3.75 | 4 | 94% |
| Phase 2 | 4 | 4 | 100% |
| Phase 3 | 0.15 | 3 | 5% |
| Phase 4 | 0 | n/a | — |

_Mettre à jour au fur et à mesure._

### Ordre de démarrage recommandé

1. **1.3 Redaction audit log** — petit effort, gros gain conformité, aucune dépendance.
2. **1.1 UI mutations Redis** — plus gros gain UX visible.
3. **1.4 Autocomplete MongoDB** — améliore l'éditeur avant d'ajouter 1.2.
4. **1.2 Aggregation pipeline** — base pour les features MongoDB de Phase 2.
5. **Phase 2** en parallèle selon disponibilité.
6. **RFC pour 3.1** à démarrer en parallèle de Phase 2 pour valider le design avant dev.

### Dépendances inter-phases

- `3.2 Redis Consumer Groups & Pub/Sub` → dépend de **3.1 refactor** pour rester propre.
- `3.3 MongoDB Change Streams` → dépend de **3.1 refactor** (capabilities).
- `2.3 $text / $regex` → peut réutiliser l'AST de **1.2** si celui-ci supporte les filtres.
- `4.1 validation Lua` → bénéficie de **2.2 Lua editor** (UI pour afficher warnings).
