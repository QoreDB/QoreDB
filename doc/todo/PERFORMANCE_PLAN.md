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

### 2.1 Profile-Guided Optimization (PGO) en CI release

**Constat** : aucun des workflows `.github/workflows/*.yml` n'utilise PGO. Le build release est déjà agressif (`opt-level=3`, `lto="thin"`, `codegen-units=1`) mais on laisse 5–15 % de perf sur la table.

**Action (release uniquement, pas dev)**
1. Ajouter un workflow secondaire `pgo-release.yml` qui :
   - Build une première fois avec `RUSTFLAGS="-Cprofile-generate=$PGO_DATA"`.
   - Exécute un scénario représentatif headless (ouverture de connexion Postgres+MySQL+SQLite, 20 requêtes canoniques, export CSV+JSON+Parquet de 100k lignes chacun).
   - Merge les profils : `llvm-profdata merge -o merged.profdata $PGO_DATA`.
   - Build définitif avec `RUSTFLAGS="-Cprofile-use=merged.profdata -Cllvm-args=-pgo-warn-missing-function"`.
2. Stocker le binaire issu du 2e build comme artefact de release.
3. Ne **pas** activer PGO sur les builds de développement (le cycle complet est long).

**Bench** : même scénario SQL que pour mimalloc, comparer avant/après PGO seul, puis avec mimalloc+PGO.

**Risque** : faible côté runtime (c'est du même Rust, juste mieux réordonné). Coût CI : +10–20 min par release. Acceptable puisque les releases ne sont pas quotidiennes.

**Ordre d'implémentation** : faire après mimalloc pour avoir une baseline mimalloc-only, puis ajouter PGO et comparer.

---

### 2.2 `simd-json` ciblé sur JSONB Postgres + documents Mongo

**Constat** : l'audit proposait de remplacer serde_json partout — **mauvaise idée** (serde_json est utilisé pour la sérialisation frontend, cas où simd-json n'est pas drop-in). En revanche **ciblé sur la désérialisation de JSONB Postgres volumineux et de documents Mongo volumineux**, le gain 2–3× est réel.

**Action (conditionnelle, à valider par bench)**
1. Identifier les 2–3 hot paths de parsing JSON entrant :
   - Valeurs `JSONB` Postgres dans le driver (chemin actuel via `sqlx` / `serde_json::Value`).
   - Documents Mongo volumineux (`bson::Document` → conversion JSON).
2. Bench baseline avec `criterion` sur un JSONB de 10 ko, 100 ko, 1 Mo.
3. Si gain > 30 % mesuré : introduire `simd-json = "0.14"` **uniquement** sur ces chemins, avec feature-flag.
4. Sinon : ne rien faire, documenter la décision.

**Risque à surveiller** : `simd-json` a un historique d'UB résolus, mais il faut valider qu'on construit `OwnedValue` (pas `BorrowedValue`) et que les inputs viennent bien de buffers mutables.

**Décision** : **bloqué tant que le bench n'est pas fait**. Ne pas implémenter à l'aveugle.

---

### 2.3 `sccache` en CI + local

**Constat** : `src-tauri/.cargo/config.toml:10` suggère `sccache` en commentaire. Pas un gain runtime, mais divise les builds CI par 2–3.

**Action** : activer `sccache` avec backend GitHub Actions Cache, puis évaluer.

**Décision** : hors scope perf runtime, mais à faire en parallèle pour accélérer les itérations perf elles-mêmes.

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
7. **2.3** `sccache` (utilitaire, pas runtime).
8. **2.1** PGO en CI release (1–2 jours de setup, gain permanent).
9. **2.2** `simd-json` ciblé **uniquement si** bench JSONB/Mongo justifie.

## Critères de validation (par item)

Chaque item est mergé seulement si :
- Bench before/after chiffré dans la PR.
- Tests Rust + lint verts (`pnpm lint:fix`, `pnpm test`).
- Build release réussi sur macOS (ARM + Intel) et Linux x64 en CI.
- Pas de régression fonctionnelle manuelle sur un scénario « ouvrir 3 onglets, lancer 3 requêtes, exporter CSV ».
