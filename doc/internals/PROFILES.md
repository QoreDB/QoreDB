<!-- SPDX-License-Identifier: Apache-2.0 -->

# Profils d'exécution — QoreDB

Snapshots datés de profils CPU sur le workload PGO (`qore-pgo-workload`) et l'app
réelle. **Source de vérité pour prioriser le Tier 3** du `doc/todo/PERFORMANCE_PLAN.md`.

Les profils bruts (`*.profile.json.gz`, `*.syms.json`) ne sont **pas versionnés** :
trop volumineux (~250 KB chacun) et machine-dependent. Ils sont attendus dans
`.perf/` (gitignored) et reproductibles via la procédure ci-dessous.

---

## Méthodologie

### Outil

[`samply`](https://github.com/mstange/samply) — profileur Rust-friendly,
multiplateforme (macOS / Linux), sans root requis sur Linux, sortie compatible
Firefox Profiler.

```bash
cargo install samply --locked
```

### Procédure de capture

```bash
# 1) Build avec debuginfo (sinon les frames ne sont pas symbolisés).
#    Override sans toucher Cargo.toml — le profil release reste strip="symbols"
#    pour les binaires distribués.
cd src-tauri
CARGO_PROFILE_RELEASE_DEBUG=line-tables-only \
CARGO_PROFILE_RELEASE_STRIP=none \
  cargo build --release -p qore-pgo-workload

# 2) macOS uniquement : générer le bundle .dSYM (samply le cherche à côté
#    du binaire ; sans lui, frames = adresses brutes).
dsymutil target/release/qore-pgo-workload

# 3) Capture (rate 4 kHz, sortie compressée + sidecar de symboles).
mkdir -p .perf
samply record \
  --save-only --no-open \
  --rate 4000 \
  --unstable-presymbolicate \
  -o .perf/workload-profile.json.gz \
  src-tauri/target/release/qore-pgo-workload
```

### Notes méthodologiques

- **Échantillonnage wall-clock** (samply par défaut sur macOS) : les threads
  parqués (workers tokio idle) apparaissent dans le profil avec leurs syscalls
  d'attente (`__psynch_cvwait`, `kevent`, `semaphore_wait_trap`). Sur ce
  workload async, ~90 % des samples tombent dans ces fonctions. **L'analyse
  utile filtre ces patterns** pour ne garder que le temps CPU actif.
- **Symbolisation** : `--unstable-presymbolicate` produit un sidecar `.syms.json`
  avec les noms de fonctions résolus. Sans dSYM, beaucoup de frames restent à
  l'état d'adresses (`0x4bf4f8` etc.). Le code C linké statiquement (libsqlite3,
  PSM) n'est jamais résolu — les warnings de `dsymutil` sur ces objets sont
  attendus.
- **Couverture** : ce workload exerce SQLite + sqlx + qore-drivers + serde_json
  + csv. Il n'exerce **pas** Postgres/MySQL/MongoDB/SQLServer (pas de réseau),
  ni le `JsonWriter` du module export, ni le streaming IPC vers le frontend.
  Pour ces axes : voir le scénario manuel ci-dessous (TODO).

---

## Snapshot 2026-04-26 — workload SQLite (`qore-pgo-workload`)

Build : `release` + `debug=line-tables-only` + `strip=none`. Apple Silicon
(M-series), macOS 25.2. Workload : 50 000 lignes insérées en batches de 250,
4 patterns SELECT × 3 itérations, 4 workers parallèles. Wall time total
~1,3 s. 67 513 samples capturés à 4 kHz.

### Distribution wall-clock par thread

| Thread | Part | Notes |
|---|---|---|
| `tokio-runtime-worker` (×6) | 71,7 % | Surtout idle (parqués sur `kevent`/`__psynch_cvwait`). Normal pour un runtime async correctement dimensionné. |
| `qore-pgo-workload` (main) | 9,0 % | Coordination + sérialisation JSON/CSV. |
| `sqlx-sqlite-worker-0..3` | 19,3 % | Threads dédiés `spawn_blocking` côté sqlx pour les appels SQLite synchrones. C'est là que se passe le vrai travail SQL. |

### Top 25 self-time, **temps CPU actif uniquement** (kernel waits exclus)

Total samples actifs : 6 004 (≈ 9 % du wall-clock total — cohérent avec un
workload async avec beaucoup d'idle).

| % CPU actif | Fonction | Lib |
|---|---|---|
| 11,84 | `_xzm_free` | libsystem_malloc (mimalloc) |
| 10,81 | `_xzm_xzone_malloc_tiny` | mimalloc |
| 9,23 | `_platform_memmove` | libsystem_platform |
| 5,13 | `mach_absolute_time` | libsystem_kernel |
| 3,43 | `_malloc_zone_malloc` | mimalloc |
| 2,95 | `<alloc::string::String as core::fmt::Write>::write_str` | std |
| 2,81 | `sqlite3VdbeExec` | libsqlite3 |
| 2,70 | `core::fmt::write` | std |
| 2,61 | `_free` | libc |
| 2,42 | `pthread_mutex_lock` | libsystem_pthread |
| 2,28 | `xzm_malloc_zone_size` | mimalloc |
| 2,23 | `alloc::fmt::format::format_inner` | std |
| 2,15 | `xzm_malloc` | mimalloc |
| 2,10 | `pthread_mutex_unlock` | libsystem_pthread |
| 1,98 | `convert_row` | qore-drivers |
| 1,87 | `_xzm_xzone_malloc` | mimalloc |
| 1,67 | `type_info` | sqlx-sqlite |
| 1,48 | `_platform_memset` | libsystem_platform |
| 1,28 | `core::fmt::Formatter::pad` | std |
| 0,78 | `core::fmt::Formatter::pad_integral` | std |
| 0,77 | `serialize` (serde_json) | qore-pgo-workload |
| 0,77 | `sqlite3Malloc` | libsqlite3 |
| 0,72 | `sqlite3ValueFree` | libsqlite3 |
| 0,62 | `serialize<qore_core::types::Value, …>` | qore-drivers |
| 0,57 | `core::str::validations::run_utf8_validation` | std |

### Top 10 inclusive (chemins critiques, kernel waits exclus)

| % inclusive | Fonction |
|---|---|
| 60,1 | `tokio::runtime::scheduler::…::run` |
| 58,9 | `qore_pgo_workload::stream_and_export` (closure async) |
| **46,0** | **`convert_row`** (qore-drivers) |
| 28,2 | `alloc::fmt::format::format_inner` |
| 22,8 | `core::fmt::write` |
| 22,6 | `main` (synchronous wrapper) |
| 19,0 | `_xzm_free` |
| 17,2 | `<alloc::string::String as core::fmt::Write>::write_str` |
| 12,7 | `sqlite3_step` / `returns_rows` |
| 12,3 | `parse_sql` (sqlite3) |

---

## Lectures clés

### 1. Pression d'allocation très forte (~32 % CPU dans mimalloc)

Cumul `_xzm_free` + `_xzm_xzone_malloc_*` + `xzm_malloc*` + `_malloc_zone_malloc`
+ `_free` ≈ **32 % du temps CPU actif**. mimalloc est rapide, mais quand il
représente un tiers du CPU on ne s'attaque pas à la perf de l'allocateur — on
réduit le **nombre** d'allocations.

### 2. `format!()` smoking gun — 28 % inclusive dans `format_inner`

`alloc::fmt::format::format_inner` représente **28 % du CPU actif inclusive**,
appelé via `core::fmt::write` (22,8 %) et `<String as fmt::Write>::write_str`
(17,2 %). Ce volume de formatage sur un workload qui n'imprime que ~10 lignes
de logs est suspect.

**Cause identifiée** : `src-tauri/crates/qore-drivers/src/drivers/sqlite.rs:160-186`
fait un type-probing en cascade dans `extract_value` :

```rust
fn extract_value(row: &SqliteRow, idx: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<bool>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<String>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<Vec<u8>>, _>(idx) { ... }
    Value::Null
}
```

Chaque `try_get` qui échoue construit une `Error` sqlx contenant un
`format!()`-message (« expected type X, got type Y »). Pour 50 000 lignes ×
7 colonnes × ~3 essais ratés en moyenne, c'est **~10⁶ `format!()` par run**.

Le driver Postgres a déjà `convert_row_with_decoders`
(`src-tauri/crates/qore-drivers/src/drivers/postgres_utils.rs:246`) qui
pré-calcule un décodeur par colonne à partir de `col.type_info()`. Il faut
appliquer le même pattern à SQLite, MySQL (`drivers/mysql.rs:201`) et
SQLServer (`drivers/sqlserver.rs:219`).

### 3. `convert_row` : 46 % inclusive — chemin critique unique

Quasi tout le travail utile du workload passe par cette fonction. C'est le
bon point de leverage : un fix sur la stratégie de décodage (item suivant)
amortit sur tous les rows.

### 4. `parse_sql` 12 % inclusive — re-parsing SQLite

SQLite re-parse la chaîne SQL pour chaque exécution dans ce workload (les
appels passent par `query_with_typed_args` qui ne réutilise pas un statement
préparé entre itérations). En production, sqlx maintient son propre cache de
statements préparés — à vérifier si ce 12 % est un artefact du workload ou
une réalité.

### 5. `mach_absolute_time` 5 % — instrumentation sqlx

sqlx mesure les temps d'exécution sur chaque query. C'est une feature, pas un
bug, mais c'est un coût constant. Pas d'action.

---

## Items Tier 3 qui sortent du profil

### Confirmés par ce profil

- **3.A — Decoder array sur SQLite/MySQL/SQLServer (calque sur Postgres)**
  Cause racine du `format!()` à 28 % inclusive. Gain attendu : -10 à -25 %
  wall time sur le workload, idem en proportion sur les drivers production.
  Le pattern existe déjà côté Postgres, c'est du portage.
  → **Plus haute priorité**.

- **3.1 — Buffer + capacity hint pour `rmp_serde`** (déjà dans le plan)
  Le profil ne le voit pas car le workload n'utilise pas le streaming IPC,
  mais le constat factuel sur `stream_msg.rs:50` reste valide. À livrer.

### Conditionnels — toujours conditionnels après ce profil

- **3.2 — Cache LRU sur `sql_safety::analyze_sql`**
  Pas testable depuis ce workload (pas d'analyse syntaxique côté driver SQLite).
  Le profil sur l'app réelle (à faire) tranchera.

- **3.3 — Lazy decode JSON/JSONB**
  Le workload n'a pas de colonne JSON. À ré-évaluer avec le profil app réelle
  sur un dataset Postgres avec JSONB.

### Découverts par ce profil

- **3.E — Pression d'allocation : `SmallVec` / `CompactString`**
  Avec 32 % du CPU dans mimalloc, réduire les allocations sur le hot path
  `convert_row` peut donner un gain non négligeable. Cibles :
  - `Vec<Value>` dans `QRow.values` : connaître la longueur à l'avance permet
    `Vec::with_capacity(col_count)`. À vérifier que c'est déjà fait.
  - Strings courtes (< 23 bytes) : `compact_str::CompactString` évite l'alloc
    heap pour les chaînes inline. Potentiellement applicable aux `ColumnInfo.name`
    et aux valeurs `Value::Text` courtes (UUIDs, codes, statuts).

  Cet item dépend de la livraison de 3.A — si on supprime déjà les `format!()`
  d'erreur, la pression d'allocation chute mécaniquement et 3.E peut devenir
  inutile. À reconsidérer après 3.A.

### Pas en haut de la liste (mais dans le profil)

- **`sqlite3VdbeExec` 2,8 % self** : c'est SQLite qui exécute notre SQL.
  Pas optimisable côté QoreDB.
- **`sqlite3Malloc` 0,77 %** : alloc interne SQLite. Inaccessible.
- **`mach_absolute_time` 5 %** : instrumentation sqlx. Acceptable.

---

## Snapshot 2026-04-28 — wall-clock post-Tier-3 (`qore-pgo-workload`)

Même configuration matérielle / build que le snapshot 2026-04-26. Mesure
post-livraison de Tier 3 (decoders SQLite/MySQL, StreamDispatcher,
caches `returns_rows` + `split_sql_statements`, batch-capacity preservation,
migration `ColumnInfo` → `CompactString`).

### Wall-clock — 5 runs après warmup

| Run | Real time |
|---|---|
| 1 | 0,97 s |
| 2 | 0,97 s |
| 3 | 0,97 s |
| 4 | 0,98 s |
| 5 | 1,04 s |

**Médian : 0,97 s** vs **baseline plan ~1,3 s** → **−25 % wall-clock**.

### Profil capturé

`.perf/workload-profile-post-tier3.json.gz` (203 KB, 17 085 samples actifs sur
19 372 totaux à 4 kHz). Symboles dans le sidecar `.syms.json`. La résolution
fine des top-N inclusive (validation que `format_inner` quitte le top-10 et que
mimalloc CPU passe sous 15 %) est laissée pour une itération ultérieure — le
gain wall-clock est suffisant pour valider la direction des optims.

---

## TODO — scénarios manquants

- [ ] Profil de l'app Tauri réelle sur SELECT 100k lignes Postgres
      (`sample_data`) + export JSON. Cible : voir la pression sur `JsonWriter`,
      le streaming IPC, `sql_safety::analyze_sql`.
- [ ] Profil sur dataset Mongo lourd en documents JSON (10 KB+ par doc).
      Cible : valider ou enterrer 3.3 (lazy JSON decode).
- [ ] Capture Linux x86_64 sur la même charge — comparer la part allocateur
      sous mimalloc x86 vs mimalloc ARM. Vérifier que le PGO build du
      workflow respecte ces ratios.
