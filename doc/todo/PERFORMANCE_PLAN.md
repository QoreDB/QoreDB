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

## Ce qui a été écarté (audit externe, points refutés ou déjà en place)

Ces items sont listés pour que les futurs audits ne reviennent pas avec les mêmes suggestions sans contexte.

| # | Suggestion audit | Réalité vérifiée | Décision |
|---|------------------|------------------|----------|
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

## Critères de validation (par item)

Chaque item est mergé seulement si :
- Bench before/after chiffré dans la PR.
- Tests Rust + lint verts (`pnpm lint:fix`, `pnpm test`).
- Build release réussi sur macOS (ARM + Intel) et Linux x64 en CI.
- Pas de régression fonctionnelle manuelle sur un scénario « ouvrir 3 onglets, lancer 3 requêtes, exporter CSV ».
