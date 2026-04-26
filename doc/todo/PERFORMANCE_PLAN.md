# Performance Plan — QoreDB

**Statut** : Plan actif — 2026-04-24
**Source** : audit externe revu + vérification factuelle du code (voir la section « Ce qui a été écarté »)

## Principes directeurs

1. **Long terme > quickfix**. On privilégie les gains structurels (allocateur global, PGO, flags CPU baseline) aux micro-optimisations ponctuelles.
2. **Ne jamais compromettre sécurité ni stabilité**. Toute perf dont le gain < 5 % mais qui introduit du risque (cache global custom, unsafe indirects, dépendances avec historique d'UB non ciblé) est rejetée.
3. **Mesurer avant/après**. Chaque item a un protocole de bench minimal. Les items sans baseline mesurable sont bloqués.
4. **Portabilité des binaires distribués**. Les builds release doivent tourner sur la cible annoncée : on n'utilise **pas** `target-cpu=native` pour les artefacts distribués.

---

## Tier 1 — Gains structurels à coût faible

### 1.1 Allocateur global `mimalloc`

**Constat** : `src-tauri/Cargo.toml` ne configure aucun allocateur custom ; aucun `#[global_allocator]` dans `main.rs` / `lib.rs`. Un client BDD passe son temps à allouer des `String`, `Vec<Row>`, noeuds `serde_json` — le system allocator est le pire cas.

**Action**
- Ajouter `mimalloc = { version = "0.1", default-features = false }` dans `src-tauri/Cargo.toml`.
- Déclarer dans `src-tauri/src/lib.rs` (le vrai point d'entrée Tauri, pas `main.rs`) :
  ```rust
  #[global_allocator]
  static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;
  ```
- Vérifier la compilation sur les trois OS cibles (macOS ARM/Intel, Windows, Linux).

**Alternative évaluée** : `tikv-jemallocator`. Plus performant sous très forte concurrence, mais binaire plus gros (~300 ko) et pas de support Windows natif propre. Inadéquat pour un client desktop cross-platform → mimalloc choisi.

**Bench**
- Temps de retour d'un `SELECT *` sur 100k lignes (Postgres `sample_data`) mesuré côté frontend, via `stream_msg` timestamps.
- Export JSON de 100k lignes vers disque : temps + pic RSS.
- Attendu : -10 à -25 % latence de retour, -5 à -15 % sur pic RSS.

**Risque** : très faible. `mimalloc` est utilisé par Microsoft/Redis/etc. en prod.

---

### 1.2 Baseline CPU dans `.cargo/config.toml`

**Constat** : `src-tauri/.cargo/config.toml:12-16` a tout commenté. AVX2/BMI2 sont inactifs en release pour tous les binaires distribués.

**Action (profil release uniquement, pas dev)**
- Dans `src-tauri/.cargo/config.toml`, ajouter des cibles explicites pour ne **pas** casser la portabilité :

  ```toml
  [target.x86_64-unknown-linux-gnu]
  rustflags = ["-C", "target-cpu=x86-64-v3"]

  [target.x86_64-pc-windows-msvc]
  rustflags = ["-C", "target-cpu=x86-64-v3"]

  [target.aarch64-apple-darwin]
  rustflags = ["-C", "target-cpu=apple-m1"]

  # x86_64-apple-darwin : rester sur le baseline macOS x86_64 pour compat Intel Mac anciens
  ```

**Pourquoi `x86-64-v3` et pas `native`** : `native` produit un binaire qui ne tournera pas sur les CPUs plus anciens que la machine de build. `x86-64-v3` = Haswell (2013+), couvre ~98 % du parc utilisateur et active AVX2. Pour les utilisateurs CPU < v3, on garde les artefacts `x86-64-v2` en option (voir item 3.2).

**Bench** : `cargo build --release` + bench SQL parsing intensif (exports Parquet, conversions row→MessagePack) sur les trois cibles.

**Risque** : faible. À mesurer : taille du binaire, temps de boot cold.

---

### 1.3 Pool de connexions — defaults plus généreux

**Constat** : defaults à `max=5`, `min=0` dans `pg_compat.rs:179-181` et `drivers/mysql.rs:529-531`. Un utilisateur qui ouvre 3 onglets + 1 export parallèle sature.

**Action**
- Nouveaux defaults : `max=10`, `min=2`, `acquire_timeout=15s` (au lieu de 30).
- Garder l'ensemble configurable via `ConnectionConfig` (déjà en place).
- Vérifier que l'UI de création de connexion expose ces paramètres en mode avancé.
- Ajouter un test d'intégration : ouverture de 5 sessions concurrentes sur la même connexion avec 2 queries chacune.

**Bench** : temps total pour 5 requêtes lancées quasi simultanément dans 5 onglets. Attendu : suppression des micro-pauses d'acquisition de connexion.

**Risque** : faible, mais attention aux DBAs dont le serveur est configuré avec un `max_connections` serré. Documenter dans `doc/rules/DATABASES.md`.

---

### 1.4 Export JSON — supprimer les allocations inutiles par ligne

**Constat confirmé avec nuance** : `src-tauri/src/export/writers/json.rs:77-83` fait bien `serde_json::to_string(&json)` par ligne, ce qui alloue puis droppe une `String`. Le BufWriter est déjà en place, donc le surcoût est en allocs, pas en syscalls. **Complexité non signalée par l'audit** : le `BufWriter` est `tokio::io::BufWriter` (async), donc `serde_json::to_writer` (qui attend un `std::io::Write` synchrone) ne s'applique pas directement.

**Action**
- Maintenir un `Vec<u8>` réutilisable dans `JsonWriter` (réinitialisé par `.clear()` après chaque ligne).
- Utiliser `serde_json::to_writer(&mut self.scratch, &json)` puis `self.write_bytes(&self.scratch)` async.
- Pré-allouer la capacité sur la première ligne (ex. `scratch.reserve(512)`).

**Bench** : export 1M lignes vers `/tmp/*.json` ; comparer wall time + allocations (via `heaptrack` ou `instruments` sur macOS).

**Gain attendu** : 10–30 % wall time, allocations divisées par ~N.

**Risque** : nul, pure mécanique.

---

### 1.5 Nettoyer `react-window` (dépendance morte)

**Constat** : `package.json:69` déclare `react-window@^2.2.7` — **aucun import dans `src/`** (la virtualisation est faite par `@tanstack/react-virtual` dans `src/components/Grid/DataGrid.tsx:18`).

**Action** : `pnpm remove react-window` + `pnpm remove @types/react-window` s'il est présent.

**Gain** : quelques ko de node_modules, zéro risque. Hygiène > perf.

---

## Tier 2 — Gains structurels à coût modéré

### 2.1 Profile-Guided Optimization (PGO) en CI release — **Livré (itération 1, Linux x86_64)**

**Constat** : aucun des workflows `.github/workflows/*.yml` n'utilise PGO. Le build release est déjà agressif (`opt-level=3`, `lto="thin"`, `codegen-units=1`) mais on laisse 5–15 % de perf sur la table.

**Mise en œuvre actuelle**
- Workload headless : `src-tauri/crates/qore-pgo-workload/` — binaire opt-in qui pilote `SqliteDriver` via le trait `DataEngine`, insère 50 k lignes, lance 4 patterns de SELECT (full-scan, WHERE, GROUP BY, JOIN) sur 3 itérations, plus 4 workers parallèles. Sérialise chaque ligne en JSON et CSV pour exercer les hot paths d'export. Tourne en ~1,5 s en release. Aucune dépendance Tauri/WebKit (la feature `tauri` de `qore-drivers` n'est pas activée).
- Workflow : `.github/workflows/pgo-release.yml` — déclenchable en `workflow_dispatch` ou via un tag `v*-pgo` (opt-in, pas branché sur le `release.yml` standard pour ne pas allonger les releases tant que le gain n'est pas validé). Pipeline : install `llvm-tools-preview` → build instrumenté du workload → run → `llvm-profdata merge` → `cargo clean` + build final `qoredb` avec `-Cprofile-use=…` + `-Cllvm-args=-pgo-warn-missing-function` → upload artefact + profil mergé.
- Ciblé Linux x86_64 (`ubuntu-22.04`) pour cette première itération. macOS aarch64 + Windows seront ajoutés une fois le gain mesuré sur Linux.

**Limites assumées de cette itération**
- Coverage du workload : SQLite + sqlx + `qore-drivers::sqlite` + serde_json/csv. Les chemins Postgres/MySQL/Mongo ne sont pas profilés (les warnings `pgo-warn-missing-function` du build final le confirmeront — c'est volontaire, pas une régression). Étendre le workload à Postgres/MySQL via Docker est un follow-up évident si le gain mesuré sur SQLite est convaincant.
- Le scénario reste minimaliste (50 k lignes, in-memory) : le PGO capture les hot paths *fréquentés*, pas un benchmark exhaustif. Acceptable — PGO veut de la couverture, pas du volume.

**Bench attendu** : même scénario SQL que pour mimalloc, comparer avant/après PGO seul, puis avec mimalloc+PGO. À effectuer sur les artefacts du workflow opt-in.

**Risque** : faible côté runtime (c'est du même Rust, juste mieux réordonné). Coût CI : +10–20 min par run. Acceptable puisque c'est opt-in.

**Suite à instrumenter (ordre)** :
1. Mesurer le gain sur le binaire produit par le workflow (latence streaming + export 100 k lignes JSON/CSV).
2. Étendre le workload à Postgres/MySQL via Docker pour couvrir les drivers réseau.
3. Ajouter macOS aarch64 + Windows à la matrice du workflow si le gain Linux est confirmé.

---

### 2.2 `simd-json` ciblé sur JSONB Postgres + documents Mongo — **Reporté (scope trop limité)**

**Constat initial** : l'audit proposait de remplacer serde_json partout — mauvaise idée (le frontend et de nombreux call-sites ne sont pas drop-in). On a donc cadré l'évaluation sur les seuls hot paths de parsing JSON entrant : valeurs `JSONB` Postgres (`postgres_utils.rs:657, 735`, via `sqlx::types::Json` → `serde_json::Value`) et documents/filtres Mongo (`drivers/mongodb.rs:159, 517, 679`, via `serde_json::from_str`). Pour Mongo, la conversion BSON→JSON principale (`bson::to_value` à `mongodb.rs:138`) n'est **pas** un parsing JSON et n'est pas concernée.

**Bench réalisé** : `src-tauri/crates/qore-pgo-workload/benches/json_parse.rs` (criterion, 30 samples, payload JSONB-shape avec objets imbriqués + arrays). Mesures Apple M-series :

| Taille | `serde_json::from_slice` | `simd_json::serde::from_slice` (drop-in) | Gain | `simd_json::to_owned_value` (non drop-in) |
|---|---|---|---|---|
| 10 kB | 64,91 µs | 51,89 µs | **+20,1 %** | 32,66 µs |
| 100 kB | 669,92 µs | 487,38 µs | **+27,2 %** | 338,53 µs |
| 1 MB | 7 002,83 µs | 5 355,04 µs | **+23,5 %** | 3 267,11 µs |

**Décision : reporté** (pas un refus technique). L'optimisation est réelle (+20 à +27 % sur l'API drop-in) mais son **rayon d'effet est trop étroit** : elle n'accélère que le décodage de payloads JSON entrants (colonnes `JSONB` Postgres, documents/filtres Mongo). Les 95 % des requêtes — `SELECT` sur colonnes scalaires, joins, exports CSV — n'en bénéficient pas du tout. Côté coût : `simd-json` ajoute une surface `unsafe` non négligeable (~50 k LOC, historique d'UB résolus type RUSTSEC-2020-0064), n'est pas drop-in dans l'API actuelle de `sqlx::types::Json` (il faut bypasser pour passer par `Vec<u8>` + skip du byte de version JSONB `0x01`), et l'API qui rendrait le gain maximal (`to_owned_value`, +50 %) impose de refactoriser `qore_core::Value::Json` pour porter un type opaque côté backend.

**À ré-évaluer si** :
- Le profilage runtime montre que le parsing JSONB / Mongo sature un cœur sur un workload réel (aujourd'hui aucun signal en ce sens).
- On gagne un public BI / logs / payloads JSON volumineux (use case dominant JSON), où l'opt-in via feature flag deviendrait justifié.
- `simd-json` publie une optim qui dépasse +40 % sur l'API serde drop-in, ou on refactorise `qore_core::Value::Json` (alors `to_owned_value` devient drop-in et son +50 % redevient pertinent sans coût caller).

**Statut des artefacts** : le bench reste en place (`[[bench]] json_parse` dans `qore-pgo-workload`) pour pouvoir re-mesurer rapidement.

---

### 2.3 `sccache` en CI + local — **Livré (CI uniquement)**

**Constat** : `src-tauri/.cargo/config.toml:8-13` suggérait `sccache` en commentaire. Pas un gain runtime, mais divise les builds CI par 2–3 sur les runs récurrents.

**Mise en œuvre**
- Activé sur `.github/workflows/ci.yml`, `build-core.yml`, `build-pro.yml` via `mozilla-actions/sccache-action@v0.0.6`. Backend = GitHub Actions Cache (zéro infra à provisionner).
- Env vars au niveau job : `SCCACHE_GHA_ENABLED=true`, `RUSTC_WRAPPER=sccache`, `CARGO_INCREMENTAL=0` (sccache et l'incremental cargo sont incompatibles — incremental est de toute façon désactivé sur les builds release par défaut).
- Local : opt-in via `cargo install sccache --locked` + `export RUSTC_WRAPPER=sccache CARGO_INCREMENTAL=0`. Volontairement non activé dans `.cargo/config.toml` pour ne pas casser le build des contributeurs sans sccache. Voir le commentaire en tête du fichier.

**Non instrumenté volontairement**
- `pgo-release.yml` : les `RUSTFLAGS=-Cprofile-generate=…` puis `-Cprofile-use=…` invalideraient le cache à chaque run et peuvent interférer avec l'instrumentation `.profraw`.
- `release.yml` : trop sensible (codesigning, notarization, MSIX) pour cette première itération. À évaluer après quelques runs CI où on aura validé l'effet de sccache.

**Décision** : hors scope perf runtime mais accélère les itérations perf elles-mêmes (re-builds après `cargo clean` lors d'un changement de RUSTFLAGS, par exemple). Effet réel à mesurer sur les 2-3 prochains runs CI.

---

## Tier 3 — Optimisations dirigées par profil

**Préambule** : Tier 1 et 2.1 ont été livrés sur des intuitions documentées (allocateur, baseline CPU, scratch buffer JSON, PGO). Avant d'attaquer une nouvelle vague de micro-optimisations, on a besoin d'un profil réel pour cibler — sinon on optimise à l'aveugle. C'est le rôle de 3.0, qui est **bloquant** pour 3.2-3.4. Seul 3.1 est suffisamment cadré (constat factuel sur `stream_msg.rs:50`) pour être livrable sans profil préalable.

### 3.0 Profil de référence (samply / flamegraph) — **Livré (workload SQLite, 2026-04-26)**

**Constat initial** : zéro profil capturé sur le workload PGO ou sur l'app réelle. Toutes les hypothèses Tier 3+ étaient non vérifiées.

**Mise en œuvre**
- Outil : `samply 0.13.1` (cargo install). Sortie compatible Firefox Profiler, pas de root.
- Build : override env (`CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, `CARGO_PROFILE_RELEASE_STRIP=none`) — pas de modification du Cargo.toml release distribué. macOS : `dsymutil` à côté du binaire pour la symbolisation.
- Capture sur le workload `qore-pgo-workload` (1,3 s, 50 k rows, 4 SELECT × 3 itérations, 4 workers parallèles). 67 513 samples à 4 kHz.
- Documenté dans `doc/internals/PROFILES.md` (snapshot daté, méthodologie reproductible, top fonctions self-time + inclusive, lectures clés et items dérivés).
- `.perf/` ajouté au gitignore (profils bruts pas versionnés).

**Findings clés** (détails dans PROFILES.md)
1. **90 % wall-clock en attente kernel** (workers tokio idle) — normal pour un workload async, l'analyse utile filtre ces patterns.
2. **32 % CPU actif dans mimalloc** — pression d'allocation très forte. Ne pas changer d'allocateur, **réduire le nombre d'allocs**.
3. **28 % CPU inclusive dans `format_inner`** — smoking gun. Cause racine : `sqlite.rs:160-186 extract_value` fait un type-probing en cascade. Chaque `try_get` qui échoue construit une `Error` sqlx avec `format!()`. ~10⁶ format!() par run.
4. **`convert_row` 46 % inclusive** — chemin critique unique, point de leverage idéal.
5. **`parse_sql` 12 % inclusive** — sqlite re-parse à chaque exécution dans ce workload.

**Couverture / limites**
- SQLite uniquement. Postgres/MySQL/MongoDB/SQLServer non profilés (workload sans réseau).
- `JsonWriter` du module export non exercé (le workload utilise `serde_json::to_writer` direct, pas le writer optimisé en 1.4).
- Streaming IPC frontend non exercé (workload headless).
- TODO listés dans PROFILES.md : profil app Tauri réelle sur Postgres + Mongo, comparaison Linux x86_64.

**Bench** : pas de bench (c'est la fondation pour bencher la suite).

**Risque** : nul. Local uniquement, pas en CI.

---

### 3.A Decoder array sur SQLite / MySQL / SQLServer — **Sorti du profil 3.0, plus haute priorité**

**Constat (vérifié sur le profil)** : `src-tauri/crates/qore-drivers/src/drivers/sqlite.rs:160-186` (et symétriquement `mysql.rs:201`, `sqlserver.rs:219`) fait un type-probing en cascade :

```rust
fn extract_value(row: &SqliteRow, idx: usize) -> Value {
    if let Ok(v) = row.try_get::<Option<i64>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<i32>, _>(idx) { ... }
    if let Ok(v) = row.try_get::<Option<f64>, _>(idx) { ... }
    // … 6 essais au total
}
```

Chaque `try_get` qui échoue retourne une `sqlx::Error` qui contient un `format!()`-message (« mismatched type », etc.). Sur 50 k rows × 7 colonnes × ~3 essais ratés en moyenne = **~10⁶ `format!()` par run**, ce qui explique les 28 % de CPU actif inclusive dans `format_inner` mesurés au snapshot 2026-04-26.

**Pattern déjà en place côté Postgres** : `src-tauri/crates/qore-drivers/src/drivers/postgres_utils.rs:246` — `convert_row_with_decoders` reçoit un `&[PostgresDecoder]` pré-calculé une fois à partir de `col.type_info()` et appelle directement le bon `try_get` par colonne. Aucun probing.

**Action**
1. Définir `enum SqliteDecoder { Int, Float, Bool, Text, Bytes, Json, Null }`.
2. Construire le tableau `Vec<SqliteDecoder>` une fois à la réception des `Columns` (à partir de `SqliteColumn::type_info().name()` : `"INTEGER"`, `"REAL"`, `"TEXT"`, `"BLOB"`, `"NULL"` + reconnaissance des types affines documentés par SQLite, et fallback sur `i64` pour les types inconnus).
3. Remplacer `convert_row` par `convert_row_with_decoders(&SqliteRow, &[SqliteDecoder]) -> QRow`.
4. Wirer dans `execute_stream` côté SQLite — au moment où on émet `StreamEvent::Columns`, on calcule aussi les décodeurs et on les passe à la boucle de batch.
5. Idem pour MySQL (`mysql.rs:201` → `convert_row_with_decoders` déjà esquissé ligne 208, mais à vérifier qu'il est utilisé partout) et SQLServer.

**Bench** : wall time du workload `qore-pgo-workload` avant/après. Le profil prédit -10 à -25 % wall time si on supprime le `format!()` du chemin chaud. À mesurer aussi : part `format_inner` dans le profil après fix (objectif : sortir du top 10 inclusive).

**Risque** : faible. Logique pure, pas de changement de wire format ni d'API publique. Tests existants côté Postgres servent de modèle pour valider l'équivalence.

---

### 3.1 Buffer réutilisable + capacity hint pour MessagePack streaming

**Constat** : `src-tauri/src/commands/stream_msg.rs:50` appelle `rmp_serde::to_vec_named(&msg)` pour chaque `RowBatch`. Sous le capot, ça fait `Vec::new() → encode::write_named(...)` avec une croissance par doublement (5-7 reallocs pour un batch typique). Pattern identique au fix 1.4 sur `JsonWriter`. Tous les drivers émettent en `RowBatch` (vérifié dans `qore-drivers/src/drivers/{sqlite,pg_compat,mysql,mongodb,duckdb,sqlserver}.rs`), donc la sérialisation est sur le hot path principal du streaming.

**Action**
- Wrapper `MsgpackBuffer` (côté `dispatch_stream_event`) qui maintient un `usize last_size_hint` mis à jour par batch.
- Remplacer `rmp_serde::to_vec_named(&msg)` par :
  ```rust
  let mut buf = Vec::with_capacity(self.last_size_hint.max(512));
  rmp_serde::encode::write_named(&mut buf, &msg)?;
  self.last_size_hint = buf.len();
  ch.send(InvokeResponseBody::Raw(buf));
  ```
- Le `Channel<InvokeResponseBody>` consomme le `Vec<u8>` ; on ne peut pas littéralement « réutiliser » la mémoire allouée (l'IPC en prend la propriété), mais le hint évite la cascade de reallocs internes à `rmp_serde`.

**Bench** : wall time du `query.rs` streaming SELECT 100k lignes. Attendu : -3 à -10 % sur le wall time côté backend (la sérialisation n'est pas le bottleneck principal, mais le gain est gratuit).

**Risque** : nul.

---

### 3.2 Cache LRU sur `sql_safety::analyze_sql` — **Déjà en place ; cibler `returns_rows` et `split_sql_statements` à la place**

**Constat (vérification post-3.0)** : `src-tauri/crates/qore-sql/src/safety.rs:22-57` implémente **déjà** un cache LRU borné à 256 entrées sur `analyze_sql`, keyé sur `(driver_id, trimmed_sql)`. Le commentaire en place confirme l'intuition : « sqlparser is the dominant cost in `analyze_sql` (several ms for large queries) and identical queries are re-run constantly during a session ».

L'item d'origine est donc **obsolète**. En revanche, deux fonctions voisines du même module **ne sont pas cachées** et reparsent à chaque appel :

- `qore-sql/src/safety.rs:80` — `returns_rows(driver_id, sql)` → `Parser::parse_sql(...)`.
- `qore-sql/src/safety.rs:93` — `split_sql_statements(driver_id, sql)` → `Parser::parse_sql(...)`.

`returns_rows` est consultée plusieurs fois par requête sur le chemin streaming (preview vs run, gating sur stream); `split_sql_statements` est appelée à `commands/query.rs:394` quand l'éditeur soumet plusieurs statements collés (paste d'un script). Le coût est borné mais réel sur des requêtes longues.

**Action (cadrée et low-effort)**

- Étendre le cache LRU existant pour mémoiser le `Vec<Statement>` parsé, ou plus simplement : ajouter un cache parallèle de `bool returns_rows` et `Vec<String> split_results` sur les mêmes clés `(driver_id, sql)`.
- Risque de surconsommation : nul, on est sur une instance qui s'étend déjà à `analyze_sql`.

**Bench** : pas prioritaire — le gain sera microscopique sur le workload typique (la même requête déjà traversée par `analyze_sql` met le cache au chaud). À cumuler à 3.A dans la même PR si on touche `qore-sql`.

**Risque** : nul (cache pur, immutable).

---

### 3.3 Lazy decode des colonnes JSON / JSONB — **Reporté faute de signal mesuré**

**Constat (post-3.0)** : le profil SQLite ne montre pas de pression sur le décodage JSON (le workload n'a pas de colonne JSON). Côté code, `postgres_utils.rs:657, 735` décode `serde_json::Value` à la lecture via le pattern `Decoder::Json` (déjà optimisé : pas de probing, un seul `try_get`). Le coût n'est donc **pas un problème de chemin**, c'est un problème de **volume de payload**.

D'après le bench 2.2, parser un JSONB 100 KB coûte ~670 µs. À ce niveau, l'optim « lazy decode » ne compte que sur des workloads :

1. Avec colonnes JSON volumineuses (≥ 10 KB/cellule) ;
2. Où l'utilisateur **ne lit pas la majorité** des cellules JSON (sinon le coût est juste reporté à l'affichage, pas évité).

Sans capturer un profil sur un dataset BI / logs réel avec ce profil d'usage, on ne peut pas chiffrer le gain pour QoreDB. Le coût d'implémentation (changer la wire format `qore_core::Value::Json`, toucher tous les drivers + tous les exporters CSV/Parquet/Excel pour forcer le parse à l'export) est non trivial.

**Décision : reporté.** À ré-évaluer si :

- Un utilisateur signale une lenteur précise sur un schéma JSON-heavy.
- Un profil app-réelle sur dataset BI/logs montre `serde_json::de::*` dans le top-10 inclusive.
- On gagne un produit Pro orienté observabilité où le pattern « SELECT * sur une table de logs » est dominant.

---

### 3.E Réduction de la pression d'allocation sur `convert_row` — **Conditionnel à la livraison de 3.A**

**Constat (du profil 3.0)** : 32 % du CPU actif passe dans mimalloc (`_xzm_free`, `_xzm_xzone_malloc_*`, `_malloc_zone_malloc`, `_free`). Sur le hot path `convert_row`, chaque ligne alloue typiquement : un `Vec<Value>` (capacité col_count), N `String`/`Vec<u8>` pour les colonnes textuelles, plus les allocs internes des `Error` sqlx pendant le probing (à supprimer en 3.A).

**Dépendance à 3.A** : la suppression du probing en cascade fera chuter mécaniquement la pression d'allocation (les `Error::Decode` allouent leurs messages d'erreur). Re-profiler après 3.A pour voir si 3.E est encore nécessaire ou si on a déjà bouché le trou.

**Cibles (si toujours pertinent post-3.A)**

- `Vec<Value>::with_capacity(col_count)` dans `convert_row` — vérifier que c'est déjà fait (probablement, mais à confirmer).
- `compact_str::CompactString` à la place de `String` pour les `ColumnInfo.name` et `Value::Text` courtes (< 23 bytes inline). Profil typique : codes pays, statuts, UUIDs (36 chars → toujours heap), noms de colonnes (souvent < 20 chars → inline). Crate `compact_str = "0.8"`, ABI stable.
- Étudier l'usage de `bumpalo` pour le scope d'un batch (allocations courtes de durée de vie identique, libérées en bloc à la fin du batch). Plus invasif, à reconsidérer plus tard.

**Bench** : workload `qore-pgo-workload` post-3.A. Cible : ratio `mimalloc CPU / total CPU actif` sous 15 % (vs 32 % actuel).

**Risque** : faible avec `CompactString` (drop-in pour la plupart des usages), moyen avec bumpalo (lifetimes).

---

### 3.4 Frontend — code-splitting des routes lourdes — **Indépendant du profil**

**Constat** : pas vérifié finement, mais le bundle Vite charge probablement Schema/ERDiagram, Diff (`src/components/Diff/*`, `src/components/Schema/ERDiagram.tsx`), et les features Pro même si l'utilisateur ne les ouvre jamais. Cold start = bundle parse + hydratation.

**Action**

- `React.lazy()` + `Suspense` sur les routes/composants pesants : ER diagram (D3 / GoJS), Diff (CodeMirror diff), et le bundle Pro entier (gated derrière un check de licence).
- Mesurer avant/après : taille du chunk principal + Time to Interactive (Lighthouse).

**Bench** : bundle size (`pnpm build` puis `du -sh dist/assets/*.js`) + TTI sur cold launch.

**Risque** : nul techniquement, attention UX — un Suspense fallback mal placé peut causer un flash visible.

---

## Ce qui a été écarté (audit externe, points refutés ou déjà en place)

Ces items sont listés pour que les futurs audits ne reviennent pas avec les mêmes suggestions sans contexte.

| # | Suggestion audit | Réalité vérifiée | Décision |
| --- | ------------------ | ------------------ | ---------- |
| Parquet streaming | « Accumule tout en RAM » | `export/writers/parquet_writer.rs:17` — `ROW_GROUP_SIZE=10000` avec flush périodique | **Déjà en place** |
| IPC binaire MessagePack | « Tauri sérialise tout en JSON, gros bottleneck » | `commands/stream_msg.rs:50` utilise `rmp_serde::to_vec_named` + `Channel<InvokeResponseBody::Raw>` avec fallback JSON | **Déjà en place**, excellente architecture |
| `bytes` crate | « À introduire pour buffers IPC » | `Cargo.toml:119` — déjà déclaré, utilisé via Tokio/Tauri | **Déjà en place** |
| AHashMap/FxHashMap généralisé | « 3–5× plus rapide » | Les HashMaps sont derrière `RwLock`/`Mutex` ; le hash n'est pas le goulot | **Refusé**, gain invisible sous contention de lock |
| `React.memo` sur toutes les cellules | « Évite re-renders » | `@tanstack/react-virtual` recycle déjà les nœuds | **Refusé**, complexité > gain |
| Tokio `worker_threads=4` | « Réduit context switching » | Tauri 2 configure déjà un multi-thread runtime équilibré | **Refusé** sans profil montrant un overhead réel |
| Rayon `par_iter` sur conversion colonnes | « 2–4× sur desktop moderne » | Le hot path actuel est le streaming IPC, pas la conversion row→MessagePack. Parallélisme = complexité sans gain avéré | **Bloqué**, à reconsidérer si un profiling montre que la conversion sature un cœur |
| `rkyv` pour cache zero-copy | — | Pas de cache persistant aujourd'hui | **Refusé tant qu'il n'y a pas de cache** |
| `zstd` local | — | Pas de cache persistant à compresser | **Refusé tant qu'il n'y a pas de cache** |
| Prepared statement cache global | — | SQLx cache déjà par connexion. Cache global = lifecycle complexe (invalidation après `ALTER TABLE`, etc.) | **Refusé**, risque > gain |
| `io_uring` via `tokio-uring`/`glommio` | « 2–3× I/O disque sur Linux » | Refactor async non trivial, Linux-only, pas de signal terrain que I/O disque est le goulot | **Refusé** par principe de portabilité |
| BOLT post-link optimizer | — | PGO d'abord ; BOLT = tier 4 éventuel | **Reporté** |

---

## Ordre d'exécution recommandé

1. **1.5** nettoyer `react-window` (5 min, zéro risque).
2. **1.3** defaults de pool (30 min + test d'intégration).
3. **1.4** fix JSON export avec buffer réutilisable (1–2 h + bench).
4. **1.1** `mimalloc` (1 h + bench sur les 3 OS).
5. **1.2** baseline `x86-64-v3` / `apple-m1` (30 min + validation artefacts release sur CI).
6. **Pause bench** : mesurer gain cumulé items 1.1 + 1.2 + 1.4, poster les chiffres dans la PR.
7. **2.3** ~~`sccache`~~ ✅ Activé sur ci.yml + build-core.yml + build-pro.yml via `mozilla-actions/sccache-action` + GHA Cache. Hors scope runtime.
8. **2.1** ~~PGO en CI release~~ ✅ Workflow opt-in livré pour Linux x86_64. Mesurer gain sur les artefacts ; étendre à Postgres/MySQL et macOS/Windows ensuite.
9. **2.2** ~~`simd-json` ciblé~~ ⏸ Reporté : gain mesuré (+20 à +27 % sur l'API drop-in) mais rayon d'effet limité aux seules colonnes JSON / documents Mongo, ne touche pas la majorité des requêtes. Bench conservé pour ré-évaluation si le profil utilisateur évolue (BI / logs JSON-heavy).
10. **Bench cumulé Tier 1 + 2.1** (étape 6 répétée à mi-parcours, à ce stade c'est le bon moment).
11. **3.0** ~~Capture d'un profil samply / flamegraph~~ ✅ Workload SQLite profilé, snapshot 2026-04-26 dans `doc/internals/PROFILES.md`.
12. **3.A** Decoder array sur SQLite / MySQL / SQLServer — **plus haute priorité** (cause racine des 28 % CPU dans `format!()` identifiée par 3.0).
13. **3.1** Buffer + capacity hint pour le streaming MessagePack (livrable indépendamment, gain gratuit, factuel sur `stream_msg.rs:50`).
14. **3.4** Code-splitting frontend (indépendant du profil backend, peut tourner en parallèle).
15. **3.2** ⏸ Cache `analyze_sql` déjà en place ; petit étendage à `returns_rows` + `split_sql_statements` à cumuler dans la PR 3.A.
16. **3.3** ⏸ Reporté faute de signal — Postgres décode déjà via decoder dédié, le coût est volume-dépendant. À ré-évaluer si profil BI/logs réel.
17. **3.E** Conditionnel au re-profil post-3.A.

## Critères de validation (par item)

Chaque item est mergé seulement si :

- Bench before/after chiffré dans la PR.
- Tests Rust + lint verts (`pnpm lint:fix`, `pnpm test`).
- Build release réussi sur macOS (ARM + Intel) et Linux x64 en CI.
- Pas de régression fonctionnelle manuelle sur un scénario « ouvrir 3 onglets, lancer 3 requêtes, exporter CSV ».
