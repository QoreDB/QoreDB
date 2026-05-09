# Plan v0.1.28 — Data Quality & Integration

> **Axe principal** : faire de QoreDB le tableau de bord de la qualité de tes données et le moyen le plus simple de les exposer ailleurs.
> **Approche** : qualité production sur Windows / macOS / Linux. Pas de quick win. Code irréprochable. Une seule branche, une seule PR.

---

## 📦 Périmètre

Six items : 2 features Pro phares + 3 polish Core + 1 driver. Volume comparable à v0.1.27 (DDL + Bulk Edit + Blob + Tab Groups + drivers cloud).

| # | Feature | Effort | Licence | Source |
| - | ------- | ------ | ------- | ------ |
| 1 | Data Contracts | L | **Pro** (`BUSL-1.1`) | `v3.md` § Killer |
| 2 | Instant Data API | M | **Pro** (`BUSL-1.1`) | `v3.md` § Killer |
| 3 | Customizable Keyboard Shortcuts | M | Core (`Apache-2.0`) | `v3.md` L224 |
| 4 | Backup / Restore Helpers | M | Core (`Apache-2.0`) | `v3.md` § Core Engineering |
| 5 | Audit & Security Hardening | M | Core (`Apache-2.0`) | `SECURITY_AUDIT.md` 2026-04-05 |
| 6 | Driver ClickHouse | M | Core (`Apache-2.0`) | `v3.md` § Drivers |

**Hors périmètre explicite** :
- Plugin System Foundation (chantier dédié, prévu v0.1.29).
- Query Replay Lab (chevauche Time-Travel V2).
- Federation Pro (qualified syntax / pushdown / sources Mongo) — release dédiée.
- AI dans Notebooks + Contextual AI Memory (à pairer ensemble plus tard).
- Elasticsearch (V0.1.29).
- Accessibilité WCAG 2.1 AA (chantier transverse, plusieurs releases).
- Perf Tier 1 résiduel (`react-window` cleanup, defaults pool, JSON export buffer) — pas vendable, à grouper dans une release perf-only.

---

## 🎯 Phase 1 — Data Contracts [Pro]

**Objectif** : assertions exécutables sur la qualité des données, persistées en YAML/JSON dans le workspace, exécutables on-demand ou depuis une cellule notebook, monitorées dans un dashboard de santé.

### Couverture des règles (objectif : 100 %)

12 types de règles, classés par catégorie. Toutes exécutées via SQL généré côté backend (pas de download brut des données).

| Catégorie | Règle | Sortie |
| --- | --- | --- |
| Présence | `not_null_pct` (seuil min %) | Pourcentage NULL |
| Présence | `not_empty` (string non vide) | Count violations |
| Format | `regex_match` (pattern Rust regex) | Count + sample violations |
| Format | `length_range` (min, max) | Count + sample |
| Domaine | `numeric_range` (min, max, inclusive flags) | Count + min/max observés |
| Domaine | `date_range` (min, max, freshness `max_age`) | Count + plage observée |
| Domaine | `allowed_values` (enum) | Count + valeurs hors set |
| Unicité | `unique` (colonne ou tuple de colonnes) | Count duplicatas + sample |
| Unicité | `distinct_count` (min, max) | Count distinct observé |
| Référentielle | `foreign_key_integrity` (table.col → table.col) | Count orphelins |
| Cardinalité | `row_count` (min, max) | Count total |
| Avancé | `custom_sql` (assertion SQL retournant 0 ligne = OK) | Lignes retournées = violations |

**Format YAML canonique** (`.qoredb/contracts/<name>.yml`) :

```yaml
name: orders_quality
version: 1
target:
  connection: prod-pg
  schema: public
  table: orders
rules:
  - id: id_unique
    type: unique
    columns: [id]
  - id: status_enum
    type: allowed_values
    column: status
    values: [pending, paid, shipped, refunded]
  - id: amount_positive
    type: numeric_range
    column: amount_cents
    min: 0
    inclusive_min: true
  - id: customer_fk
    type: foreign_key_integrity
    column: customer_id
    references: { table: customers, column: id }
```

### Découpage fichiers

`src/lib/contracts/` (nouveau, **Pro**, BUSL-1.1)
- `types.ts` (~150 lignes) — `Contract`, `Rule`, `RuleResult`, `ContractRun`.
- `parser.ts` (~200 lignes) — YAML/JSON → `Contract` validé. Erreurs détaillées par ligne.
- `sqlBuilder/` (~200 lignes total, splitté par catégorie : `presence.ts`, `format.ts`, `domain.ts`, `uniqueness.ts`, `referential.ts`, `cardinality.ts`, `customSql.ts`).
- `index.ts` — barrel.

`src-tauri/src/contracts/` (nouveau, **Pro**, BUSL-1.1, `#[cfg(feature = "pro")]`)
- `mod.rs` — types miroir + dispatcher.
- `parser.rs` — relecture serveur (defense-in-depth, le YAML peut venir d'un fichier disque).
- `runner.rs` — orchestration : pour chaque règle → SQL builder → `executeQuery` via session existante → agrégation des `RuleResult`.
- `sql/` — un fichier par catégorie de règle, dialect-aware (PG / MySQL / SQLite / DuckDB / SQL Server / CockroachDB / ClickHouse).
- `events.rs` — émission progress (`contract.run.progress`, `contract.run.completed`).

**⚠️ Aucun fichier > 500 lignes.** `sql/` est splitté pour rester sous le seuil.

`src/components/Contracts/` (nouveau, **Pro**, BUSL-1.1)
- `ContractsPanel.tsx` (~250 lignes) — sidebar entrée, dashboard de santé.
- `ContractEditor.tsx` (~300 lignes) — édition YAML avec validation live (réutilise CodeMirror).
- `ContractRunDialog.tsx` (~150 lignes) — exécution + progress.
- `ContractResultsView.tsx` (~250 lignes) — pass/fail par règle, sample violations expandables.
- `ContractHealthBadge.tsx` (~80 lignes) — pastille verte/orange/rouge dans la sidebar des connexions.

**Cellule notebook** : nouveau type `'contract'` dans `src/components/Notebook/`. Référence un fichier `.yml` ou inline. Affiche le `ContractResultsView`.

**Hook intercepteur** : `src-tauri/src/interceptor/contract_alert.rs` (Pro). Quand une mutation est exécutée sur une table couverte par un contrat actif, post-execute déclenche une re-évaluation asynchrone des règles concernées. Notification UI si nouvelle violation. Best-effort, pas bloquant.

**Backend bindings (nouveaux)** :
- `list_contracts(workspace_id) -> Vec<ContractMeta>`
- `load_contract(path) -> Contract`
- `save_contract(path, content) -> ()`
- `run_contract(connection_id, contract) -> ContractRun` (streaming events)
- `get_contract_history(contract_id, limit) -> Vec<ContractRun>` (persistance JSONL local)

**i18n** : sous-arbre `contracts.*` (~80 clés), 9 locales.

**Events PostHog** : `contract_created`, `contract_run_started`, `contract_run_completed` (props : `rules_count`, `violations_count`, `duration_ms`, `driver`).

---

## 🎯 Phase 2 — Instant Data API [Pro]

**Objectif** : exposer une query sauvegardée comme endpoint REST local. Lancement explicite par l'utilisateur, jamais en arrière-plan automatiquement.

### Décisions sécurité

1. **Bind `127.0.0.1` uniquement** (jamais `0.0.0.0`). Pas configurable en V1.
2. **Bearer token obligatoire**, auto-généré (`api-` + 32 octets base64-url), affiché une seule fois à la création de l'endpoint puis hashé (Argon2) en stockage. Bouton "Regenerate" dans l'UI.
3. **Read-only** : seules les queries classifiées `Read` par `sql_safety.rs` / `mongo_safety.rs` sont éligibles. Toute mutation rejetée à l'enable, pas seulement à l'exécution.
4. **CORS** : par défaut `Access-Control-Allow-Origin` non envoyé (= same-origin only). Allowlist optionnelle dans Settings (regex).
5. **Rate limit** : 10 req/s par endpoint (token bucket en mémoire, configurable). Au-delà → `429`.
6. **Pas de log des paramètres** dans l'audit interceptor : redaction systématique des valeurs query string (réutilise `interceptor/redaction.rs`).
7. **Lifecycle** : le serveur s'arrête si l'app est verrouillée (App Lock), si l'utilisateur se déconnecte du workspace, ou via toggle explicite.

### Surface API exposée

```
GET  /api/<endpoint-name>?<params>     → JSON paginé { data, page, total }
GET  /openapi.json                     → OpenAPI 3.1 auto-généré
GET  /health                           → { status: "ok", uptime_s }
```

Tous les endpoints (sauf `/health`) requièrent `Authorization: Bearer <token>`.

### Découpage fichiers

`src-tauri/src/api/` (nouveau, **Pro**, BUSL-1.1, `#[cfg(feature = "pro")]`)
- `mod.rs` — re-export + commands.
- `server.rs` (~300 lignes) — `axum` server, lifecycle (start/stop), bind `127.0.0.1` strict.
- `auth.rs` (~150 lignes) — middleware Bearer, hash Argon2, regen token.
- `endpoints.rs` (~250 lignes) — registry persisté JSON, CRUD.
- `handlers.rs` (~250 lignes) — exécution query → JSON paginé, gestion erreurs.
- `openapi.rs` (~200 lignes) — génération OpenAPI 3.1 depuis le registry.
- `rate_limit.rs` (~100 lignes) — token bucket par endpoint.

**Choix `axum`** : déjà dans l'écosystème tokio, dépendance unique, pas de TLS local nécessaire (`127.0.0.1` only).

`src/components/InstantAPI/` (nouveau, **Pro**, BUSL-1.1)
- `InstantApiPanel.tsx` (~250 lignes) — liste endpoints, status, port, action start/stop globale.
- `EndpointDialog.tsx` (~250 lignes) — création/édition (query source, nom, params déclarés).
- `EndpointTokenDialog.tsx` (~120 lignes) — affichage token unique post-création + bouton "Copy".
- `OpenApiPreview.tsx` (~100 lignes) — preview du JSON généré.

**Backend bindings** :
- `start_instant_api(port?) -> { port, base_url }`
- `stop_instant_api() -> ()`
- `get_instant_api_status() -> { running, port, endpoints_count }`
- `list_endpoints() -> Vec<EndpointMeta>`
- `create_endpoint(query_id, name, params) -> { endpoint, token }` (token retourné une seule fois)
- `regenerate_endpoint_token(endpoint_id) -> { token }`
- `delete_endpoint(endpoint_id) -> ()`

**i18n** : sous-arbre `instantApi.*` (~50 clés), 9 locales.

**Events PostHog** : `instant_api_started`, `instant_api_endpoint_created`, `instant_api_request` (sampled 1/100, props `driver`, `status_code`, `duration_ms` — **jamais** le nom d'endpoint ni les params).

---

## 🎯 Phase 3 — Customizable Keyboard Shortcuts [Core]

**Objectif** : remplacer les raccourcis hardcodés dans `useKeyboardShortcuts` par un registry configurable, exposé dans Settings, exporté/importable.

### Découpage fichiers

`src/lib/shortcuts/` (nouveau)
- `registry.ts` (~250 lignes) — registry typé (`ShortcutId`, `ShortcutDefinition`, `KeyChord`), parsing et serialisation.
- `defaults.ts` (~150 lignes) — défauts cross-OS (`Cmd` mac vs `Ctrl` win/linux), groupés par catégorie : Editor, Navigation, Notebook, Tabs, DataGrid, Global.
- `conflicts.ts` (~120 lignes) — détection : deux chords identiques sur des contextes overlap (ex. global vs editor focused).
- `storage.ts` (~80 lignes) — persistance dans le data dir Tauri (`shortcuts.json`).

`src/hooks/useKeyboardShortcuts.ts` — refactor pour lire le registry au lieu de constantes en dur. Re-render sur changement (event listener).

`src/components/Settings/ShortcutsSettings.tsx` (~350 lignes) — sous-onglet Settings → "Raccourcis", liste par catégorie avec champs chord-recorder, conflits highlightés en orange, boutons Reset par catégorie + Reset all + Export JSON + Import JSON.

`src/components/KeyboardCheatsheet.tsx` — refactor pour lire le registry.

**Cross-OS** :
- Recorder accepte les chords avec modifiers `Mod` (= `Cmd` mac, `Ctrl` win/linux), `Shift`, `Alt`, `Ctrl` (séparé de `Mod` pour les gens qui veulent forcer).
- Validation : refuse les chords système (ex. `Cmd+Tab`, `Cmd+Q`, `F11`).
- Persistance unifiée : on stocke `Mod+S`, l'OS résout au moment du binding.

**Pas de nouveau binding Tauri** (pure frontend + localStorage / app data dir via `tauri-plugin-fs` déjà en place).

**i18n** : sous-arbre `shortcuts.*` (~40 clés).

**Events PostHog** : `shortcut_customized` (props : `shortcut_id`, `category`), `shortcuts_reset` (props : `scope`).

---

## 🎯 Phase 4 — Backup / Restore Helpers [Core]

**Objectif** : wrappers autour de `pg_dump` / `mysqldump` / `mariadb-dump` / `mongodump` / `sqlite3 .dump` + leurs équivalents `restore`. Pas de réimplémentation : on délègue aux binaires officiels.

### Décisions

1. **Détection automatique** dans `$PATH` à l'ouverture du dialog. Si trouvé → utilisé d'office, chemin affiché.
2. **Picker explicite** (`tauri-plugin-dialog`) si non trouvé OU si l'utilisateur clique "Choisir un autre binaire". Cross-OS : sur Windows, autoriser `.exe` ; sur macOS/Linux, autoriser sans extension.
3. **Persistance des chemins** dans Settings (`backup_tools.pg_dump_path`, etc.), workspace-scoped.
4. **Streaming stdout/stderr** vers l'UI via `tauri::async_runtime` + events `backup.progress`. Pas de buffer infini : ring buffer 1000 lignes.
5. **Fichiers de sortie** : choisis par l'utilisateur via picker, jamais auto-générés dans des chemins implicites.
6. **Confirmation explicite** avant restore : double saisie du nom de la base cible (pattern destructif).
7. **Pas d'exécution dans la sandbox Tauri shell-plugin par défaut** : on whitelist explicitement les noms de binaires (`pg_dump`, `pg_restore`, `mysqldump`, `mariadb-dump`, `mongodump`, `mongorestore`, `sqlite3`) dans `tauri.conf.json` capabilities.

### Découpage fichiers

`src-tauri/src/backup/` (nouveau)
- `mod.rs` — types + commands.
- `tools.rs` (~200 lignes) — détection binaires (which crate), stockage paths.
- `runner.rs` (~300 lignes) — spawn + streaming + cleanup.
- `args.rs` (~250 lignes) — construction des arguments par driver (PG : `--schema-only`, `--data-only`, `--table=...`, MySQL : `--no-data`, `--no-create-info`, etc.).

**Drivers supportés en V1** :

| Driver | Backup | Restore | Format |
| --- | --- | --- | --- |
| PostgreSQL / Supabase / Neon / TimescaleDB / CockroachDB | `pg_dump` | `pg_restore` ou `psql` | SQL plain ou custom |
| MySQL / MariaDB | `mysqldump` / `mariadb-dump` | `mysql` / `mariadb` CLI | SQL plain |
| SQLite | `sqlite3 .dump` | `sqlite3 < file.sql` | SQL plain |
| MongoDB | `mongodump` | `mongorestore` | BSON archive |

DuckDB / SQL Server / Redis / ClickHouse → hors scope V1, message explicite "Backup non disponible pour ce driver".

`src/components/Backup/` (nouveau)
- `BackupDialog.tsx` (~300 lignes) — sélection mode (full / schema / data), picker tables, picker output, options driver-specific.
- `RestoreDialog.tsx` (~280 lignes) — picker fichier, confirmation destructive, preview commande.
- `ToolPathSettings.tsx` (~150 lignes) — sous-section Settings → "Outils externes", paths persistés.
- `BackupProgressView.tsx` (~150 lignes) — progress bar + log scroll.

**Backend bindings** :
- `detect_backup_tools() -> Vec<{ name, path, found }>`
- `set_backup_tool_path(name, path) -> ()`
- `start_backup(connection_id, options) -> JobId`
- `start_restore(connection_id, options) -> JobId`

Réutilise `Background Job Manager` existant pour suivi/cancel.

**i18n** : sous-arbre `backup.*` (~70 clés), 9 locales.

**Events PostHog** : `backup_started`, `backup_completed` (props : `driver`, `mode`, `duration_ms`, `size_bytes_bucket`), `restore_started`, `restore_completed`.

---

## 🎯 Phase 5 — Audit & Security Hardening [Core]

Solde en une PR cinq findings de l'audit `SECURITY_AUDIT.md` (2026-04-05) + ajoute le query fingerprinting tracé en `v3.md` § Sécurité.

### 5.1 Read-only uniformity (HIGH)

**Cible** : tout `commands/*.rs` qui mute → passe par le même garde-fou que `execute_query`.

**Fichiers** :
- `src-tauri/src/commands/connection.rs` — `create_database`, `drop_database` : ajouter `policy::ensure_writes_allowed(env, &session)?` avant exécution.
- `src-tauri/src/commands/maintenance.rs`, `routines.rs`, `triggers.rs`, `sequences.rs`, `mutation.rs` — vérifier la couverture, ajouter si manquant.
- Ajouter test d'intégration matriciel : pour chaque command mutante × env Production → assert `Err(PolicyViolation::ReadOnly)`.

### 5.2 Governance limits étendues (MEDIUM)

**Cible** : timeout, max_rows, concurrent_queries appliqués à `preview_table`, `query_table`, `peek_foreign_key`.

**Fichiers** :
- `src-tauri/src/commands/query.rs` — extraire la logique de governance dans un helper `with_governance(session, op, fut)`. Wrapper les 3 endpoints concernés.
- Tests : assert que `preview_table` sur une table de 10M lignes en env Production tronque au seuil configuré.

### 5.3 Audit log read-from-disk (MEDIUM)

**Cible** : `get_audit_entries()` et `export_audit_log()` lisent les fichiers JSONL rotés sur disque, pas seulement le cache mémoire 1000.

**Fichiers** :
- `src-tauri/src/interceptor/audit.rs` — nouveau `read_audit_files(filter, limit, cursor)` avec stream paginé. Préserve l'ordre chronologique inverse.
- Doc : `doc/internals/UNIVERSAL_QUERY_INTERCEPTOR.md` — section "Audit retention" mise à jour.

### 5.4 Query fingerprinting + export JSONL/CSV (MEDIUM)

**Cible** : regroupement des requêtes par signature normalisée, export multi-format.

**Fichiers** :
- `src-tauri/src/interceptor/fingerprint.rs` (nouveau, ~250 lignes) — normalisation : littéraux → `?`, identifiers préservés, whitespace collapsé. SHA-256 du résultat = fingerprint. Multi-dialecte (réutilise `sqlparser`).
- `src-tauri/src/interceptor/audit.rs` — chaque entrée enrichie d'un champ `fingerprint`. Migration : recompute au démarrage si absent (best-effort).
- `src-tauri/src/interceptor/export.rs` (nouveau, ~150 lignes) — writers JSONL et CSV. Réutilise `csv` crate déjà présente côté export.
- Frontend : `src/components/Interceptor/AuditPanel.tsx` — ajouter colonne "Fingerprint", filtre par fingerprint, dropdown export (JSON / JSONL / CSV).

### 5.5 Share providers HTTPS-only (MEDIUM)

**Fichier** : `src-tauri/src/share/manager.rs` — refuser `http://` (Err `InvalidShareUrl::InsecureScheme`), exception explicite si `host == "127.0.0.1"` ou `host == "localhost"` ET flag `allow_localhost_http: true` dans la config provider.

Migration : flagger les providers existants en `http://` au démarrage, afficher un warning UI, désactiver tant que non corrigé.

### 5.6 FS capability scope tightening (MEDIUM)

**Fichier** : `src-tauri/capabilities/default.json` — limiter `fs:allow-write-text-file` et `fs:allow-write-file` aux scopes :
- `$DOCUMENT/qoredb/*`
- `$DOWNLOAD/*`
- `$APPDATA/qoredb/*`
- `$HOME/.qoredb/*`

Tout autre chemin → médiatisé par le file picker (déjà en place).

**i18n** : nouvelles clés (`audit.fingerprint`, `audit.export.jsonl`, `audit.export.csv`, `share.https_required`, etc.) ~25 clés, 9 locales.

**Events PostHog** : `audit_exported` (props : `format`, `entries_count`), `audit_filtered_by_fingerprint`.

---

## 🎯 Phase 6 — Driver ClickHouse [Core]

**Objectif** : driver natif ClickHouse, intégré dans `DriverRegistry`, classification AST safety, support des concepts MergeTree.

### Décisions

1. **Crate** : `klickhouse` (tokio-native, pas de bridge C). Évalué vs `clickhouse-rs` : `klickhouse` plus actif, async-first, support TLS via Rustls.
2. **Protocol** : binaire natif (port 9000 / 9440 TLS) plutôt que HTTP. Cohérent avec les autres drivers (perf streaming).
3. **Mapping types** : `Int8/16/32/64`, `UInt*`, `Float32/64`, `String`, `FixedString`, `Date`, `DateTime`, `DateTime64`, `UUID`, `Decimal`, `Array(T)`, `Nullable(T)`, `LowCardinality(T)`, `Tuple`, `Map`, `Enum8/16`. Mappage vers `Value` interne avec extension `Value::Decimal` déjà présente.
4. **describeTable** : remonte engine (MergeTree, ReplicatedMergeTree, Distributed, Log, etc.), partition key, sorting key, sample by → tags affichés dans l'UI.
5. **Pagination** : `LIMIT N OFFSET M` natif. Pas de cursor.
6. **Mutations** : `INSERT`, `ALTER TABLE ... UPDATE WHERE`, `ALTER TABLE ... DELETE WHERE`. Pas de UPDATE/DELETE row-level standard.
7. **Safety classification** : `DROP DATABASE`, `TRUNCATE`, `DETACH`, `SYSTEM SHUTDOWN`, `OPTIMIZE FINAL` → dangerous. Inserts sans WHERE n'existent pas (INSERT n'a pas de WHERE).

### Découpage fichiers

`src-tauri/crates/qore-drivers/src/drivers/clickhouse.rs` (~1200 lignes max, sinon split en `clickhouse/` module : `mod.rs`, `types.rs`, `query.rs`, `describe.rs`).

`src-tauri/crates/qore-drivers/src/clickhouse_safety.rs` (~300 lignes) — analogue `mongo_safety.rs`, classification 4 niveaux (Read / Mutation / Dangerous / Unknown).

**Tests** : `docker-compose.yml` — service `clickhouse` (image `clickhouse/clickhouse-server:24-alpine`). Tests d'intégration alignés sur les autres drivers (`tests/clickhouse_*.rs`).

`src/lib/connection/drivers.ts` — entrée `clickhouse` avec icône + couleur dédiée + DSN smart-paste (`clickhouse://`, `tcp://`).

`src/lib/ddl/typeDefinitions.ts` — types ClickHouse pour le DDL editor (subset : MergeTree, Engine, partition by, order by).

**i18n** : entrée driver `clickhouse` dans les 9 locales.

**Events PostHog** : aucun nouveau, le driver hérite des events généraux (`query_executed`, etc.).

---

## 🧱 Décisions techniques transverses

1. **Mono-PR**, mono-branche `feat/v0.1.28`. Phases mergées en interne séquentiellement mais commits scoped par phase pour reviewabilité.
2. **SPDX systématique** : Phase 1 et 2 → `BUSL-1.1` partout (frontend + backend). Phases 3, 4, 5, 6 → `Apache-2.0`.
3. **Aucun fichier > 500 lignes**, splittage prévu d'avance dans chaque phase.
4. **i18n exhaustive sur 9 locales** (en, fr, de, es, pt-BR, ru, ja, ko, zh-CN). Zéro string en dur.
5. **Composants `src/components/ui/`** réutilisés (Dialog, Tabs, Input, Button, Select, Tooltip, Toast).
6. **Tests Vitest** pour la logique pure : `contracts/parser`, `contracts/sqlBuilder`, `shortcuts/conflicts`, `shortcuts/registry`, `interceptor/fingerprint` (côté JS si dupliqué pour preview).
7. **Tests cargo** : `contracts::runner` matriciel par driver, `api::auth`, `api::rate_limit`, `backup::args`, `interceptor::fingerprint`, `clickhouse_safety::classify`, governance + read-only matriciels.
8. **Aucune dépendance Tauri shell-plugin non whitelistée** : Phase 4 whitelistera explicitement les binaires backup.
9. **PostHog events documentés** dans `doc/release/EVENTS.md` au fil de chaque phase.
10. **Cargo features** : Phases 1 et 2 sous `#[cfg(feature = "pro")]`. Build Core sans `pro` doit toujours compiler et passer les tests existants.

---

## 🔗 Dépendances et ordre

```
Phase 5.1 (read-only) ──► Phase 5.2 ──► Phase 5.3 ──► Phase 5.4 ──► Phase 5.5 ──► Phase 5.6
                                                          │
Phase 6 (ClickHouse) ─────────────────────────────────────┤
                                                          │
Phase 1 (Contracts) ◄─── needs ClickHouse pour 100 % couv.┤
                                                          │
Phase 2 (Instant API) ────────────────────────────────────┤
                                                          │
Phase 3 (Shortcuts) ──── indépendante ────────────────────┤
                                                          │
Phase 4 (Backup) ─────── indépendante ────────────────────┘
```

**Ordre recommandé** :
1. **Phase 5** d'abord (audit hardening) → débloque le respect des governance limits par les autres phases.
2. **Phase 6** (ClickHouse) → débloque le SQL builder de Phase 1 sur 7 dialectes au lieu de 6.
3. **Phase 1** (Contracts) → exploite l'intercepteur durci.
4. **Phase 2** (Instant API) → réutilise la classification Read de Phase 5.
5. **Phases 3 et 4** en parallèle, indépendantes.

---

## ✅ Checklist de release v0.1.28

### Code
- [ ] Header SPDX correct sur tous les nouveaux fichiers (Apache-2.0 / BUSL-1.1).
- [ ] Aucun fichier > 500 lignes (split au besoin).
- [ ] `pnpm lint:fix && pnpm format:write` clean.
- [ ] `pnpm test` (Rust) sans régression.
- [ ] `cargo build` (sans flag) compile (Core).
- [ ] `cargo build --features pro` compile (Pro).
- [ ] Tests Vitest verts pour logique pure frontend.
- [ ] i18n FR + EN + 7 autres locales exhaustive (zéro string en dur).
- [ ] Composants `src/components/ui/` réutilisés (audit visuel).
- [ ] Validation cross-OS effective : binaires backup détectés, raccourcis cross-OS, ports `127.0.0.1`, FS capabilities scopes appliquées.

### Documentation
- [ ] `doc/todo/v3.md` — cocher Data Contracts, Instant Data API, Backup/Restore Helpers, raccourcis customisables, ClickHouse, Audit Log Improvements (fingerprinting + export), File System Scope Restriction.
- [ ] `doc/FEATURES.csv` — 6 nouvelles lignes (`data_contracts`, `instant_data_api`, `customizable_shortcuts`, `backup_restore`, `audit_fingerprinting`, `clickhouse_driver`).
- [ ] `doc/rules/FEATURES.md` — section dédiée par feature.
- [ ] `doc/rules/DATABASES.md` — ligne ClickHouse dans la matrice (DDL / Bulk Edit / Backup support).
- [ ] `doc/internals/UNIVERSAL_QUERY_INTERCEPTOR.md` — sections fingerprinting + read-from-disk + contract alert hook.
- [ ] `doc/audits/SECURITY_AUDIT.md` — marquer findings 1, 2 (HIGH+MEDIUM), 3, 4, 5 comme résolus avec date.
- [ ] `doc/release/EVENTS.md` — nouveaux events PostHog.
- [ ] `README.md` — bullets pour les 6 features.
- [ ] `doc/release/RELEASE_NOTES_v0.1.28.md` — release notes finales.

### Release
- [ ] Bump `package.json`, `Cargo.toml` (workspace + crates), `tauri.conf.json` → `0.1.28`.
- [ ] `aur/PKGBUILD` mis à jour automatiquement par le workflow AUR.
- [ ] Release notes : highlights des 6 features + limitations connues (drivers sans backup, ClickHouse sans DDL Alter en V1, etc.).
- [ ] Migration audit log : si fichiers existants sans `fingerprint`, recompute au démarrage best-effort, log des conversions.

---

## 🚧 Limitations connues à documenter

- **Data Contracts** : `custom_sql` ne tourne que sur le driver de la connexion cible (pas de fédération automatique). À élargir en V0.1.29.
- **Instant Data API** : pas de HTTPS local en V1 (justifié par bind `127.0.0.1`). Pas de WebSocket ni d'endpoints en écriture.
- **Backup/Restore** : ne couvre pas DuckDB / SQL Server / Redis / ClickHouse en V1. Message explicite dans l'UI.
- **ClickHouse** : DDL Alter visuel limité (subset MergeTree only) ; pas de support `ON CLUSTER` en V1.
- **Customizable Shortcuts** : les chords système sont protégés mais la liste n'est pas exhaustive sur Linux (varie par WM).
