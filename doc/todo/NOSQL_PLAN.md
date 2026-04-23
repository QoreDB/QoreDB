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

### 1.2 MongoDB : Aggregation pipeline first-class

_Effort estimé : 2-3 semaines_

- [ ] **Backend — AST validator**
  - [ ] Créer `src-tauri/crates/qore-drivers/src/mongo_pipeline.rs`
  - [ ] Parser JSON → `Vec<PipelineStage>` typé
  - [ ] Valider les stages autorisés : `$match`, `$project`, `$group`, `$sort`, `$limit`, `$skip`, `$unwind`, `$lookup`, `$count`, `$addFields`, `$replaceRoot`
  - [ ] Rejeter explicitement `$out`, `$merge`, `$function`, `$accumulator` (dangerous)
  - [ ] Valider opérateurs imbriqués (`$gt`, `$in`, `$regex`, `$expr`, etc.)
- [ ] **Backend — exécution streaming**
  - [ ] Adapter `execute_stream()` pour supporter les pipelines
  - [ ] Curseur avec batches de 500 documents (comme `find`)
  - [ ] Support abort/cancel
- [ ] **Safety**
  - [ ] Étendre `mongo_safety.rs` pour valider AST pipeline au lieu de pattern matching
  - [ ] Classer `$out`/`$merge` comme `Mutation` (déjà fait, vérifier)
- [ ] **Frontend**
  - [ ] Templates pipeline dans `MongoEditor.tsx` (snippets : count by group, top N, lookup)
  - [ ] Highlight syntaxique des opérateurs `$*`
- [ ] **Tests**
  - [ ] Couverture AST validator (cas nominaux + rejets)
  - [ ] Test fixtures dans `src-tauri/tests/`
- [ ] **Doc**
  - [ ] `doc/rules/DATABASES.md` : section aggregation MongoDB
  - [ ] Exemples dans `doc/tests/DRIVER_LIMITATIONS.md`

**Done when** : les aggregations complexes (lookup multi-collections, groupBy, unwind) s'exécutent en streaming avec validation stricte.

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

### 2.1 MongoDB : Index management UI

_Effort estimé : 2 semaines_

- [ ] **Backend**
  - [ ] Ajouter `create_index(collection, keys, options)` dans `mongodb.rs`
  - [ ] Ajouter `drop_index(collection, name)`
  - [ ] Exposer `list_indexes` en commande Tauri si pas déjà fait
  - [ ] Support options : `unique`, `sparse`, `expireAfterSeconds` (TTL), `partialFilterExpression`
- [ ] **Frontend — dialog création index**
  - [ ] Créer `src/components/Schema/IndexDialog.tsx`
  - [ ] Sélection champs + direction (1 / -1 / `text` / `2dsphere`)
  - [ ] Options avancées (unique, sparse, TTL)
  - [ ] Preview de la commande générée
- [ ] **Frontend — liste des indexes**
  - [ ] Onglet "Indexes" dans le panneau de table MongoDB
  - [ ] Actions : créer, dropper, voir stats
- [ ] **Tests**
  - [ ] Tests de création/drop d'index
- [ ] **Doc**
  - [ ] `doc/rules/DATABASES.md` section MongoDB indexes

**Done when** : création d'un index unique composite depuis l'UI sans toucher à la command-line.

---

### 2.2 Redis : Lua script editor

_Effort estimé : 2 semaines_

- [ ] **Frontend — LuaScriptEditor**
  - [ ] Créer `src/components/Editor/LuaScriptEditor.tsx`
  - [ ] CodeMirror avec `@codemirror/legacy-modes/mode/lua`
  - [ ] Snippets pour patterns courants (`redis.call`, `redis.pcall`, KEYS/ARGV)
  - [ ] Input dédié pour `KEYS` et `ARGV`
- [ ] **Backend**
  - [ ] Helper `eval_script(script, keys, args)` wrappant EVAL
  - [ ] Helper `eval_sha(sha, keys, args)` pour scripts pré-chargés
  - [ ] Helper `script_load(script)` retournant le SHA
- [ ] **UI**
  - [ ] Nouveau type de tab Redis "Lua"
  - [ ] Bouton "Charger (SCRIPT LOAD)" + "Exécuter (EVAL)"
  - [ ] Bibliothèque de scripts sauvegardés (localStorage + Query Library)
- [ ] **Safety**
  - [ ] Garder EVAL classé `Mutation` (peut écrire)
  - [ ] Warning si script contient `redis.call('FLUSHALL')` ou similaire (regex best-effort)
- [ ] **Doc**
  - [ ] `doc/rules/DATABASES.md` section Redis Lua

**Done when** : écriture et exécution d'un script Lua avec KEYS/ARGV, résultat affiché dans le viewer.

---

### 2.3 MongoDB : `$text` et `$regex` natifs

_Effort estimé : 2 semaines_

- [ ] **Backend**
  - [ ] Support `$text` dans filtres `query_table` (nécessite index text)
  - [ ] Support `$regex` avec flags (`i`, `m`, `x`, `s`) complet
  - [ ] Propagation des scores `$meta: "textScore"` dans les résultats
- [ ] **Frontend — query builder**
  - [ ] Option "Recherche full-text" dans le DataGrid filter bar
  - [ ] Option "Regex" avec input regex + flags
  - [ ] Warning si pas d'index text (proposer création)
- [ ] **Tests**
  - [ ] Tests sur collections avec index text
- [ ] **Doc**
  - [ ] Mise à jour section MongoDB filters

**Done when** : recherche full-text depuis la barre de filtre du DataGrid fonctionne.

---

### 2.4 MongoDB : `bulkWrite` + `findOneAnd*`

_Effort estimé : 1 semaine_

- [ ] **Backend**
  - [ ] Ajouter operation `bulkWrite` parsant un array d'operations
  - [ ] Ajouter `findOneAndUpdate`, `findOneAndReplace`, `findOneAndDelete`
  - [ ] Support options `returnDocument: Before|After`
- [ ] **Safety**
  - [ ] `bulkWrite` classé `Mutation`
  - [ ] `findOneAnd*` classé `Mutation`
- [ ] **Frontend**
  - [ ] Snippets bulkWrite dans `MongoEditor`
- [ ] **Tests**
  - [ ] Tests unitaires bulkWrite avec mix insert/update/delete

**Done when** : un bulkWrite de 1000 opérations atomiques s'exécute et retourne le résumé par type.

---

## Phase 3 — Long terme (structurel)

### 3.1 Refactor `DataEngine` v2 — abstraction moins SQL-centric

_Effort estimé : 6-8 semaines. **Breaking change — à planifier pour release majeure**_

- [ ] **Design**
  - [ ] Rédiger RFC dans `doc/internals/RFC_DATAENGINE_V2.md`
  - [ ] Review par les mainteneurs des drivers SQL + NoSQL
  - [ ] Plan de migration versionné (v1 + v2 coexistent pendant une release)
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
| Phase 1 | 1.75 | 4 | 44% |
| Phase 2 | 0 | 4 | 0% |
| Phase 3 | 0 | 3 | 0% |
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
