# Jalon 0 — Extraction de `qore-service`

**Parent** : `QORE_PLATFORM_ROADMAP.md` (Jalon 0, clé de voûte)
**But** : dégager une couche de service **sans dépendance Tauri**, réutilisable par toutes les surfaces (desktop, CLI, MCP, serveur), à partir de la logique aujourd'hui inline dans `src/commands/`.

---

## 1. Objectif & définition of done

À la fin du jalon :

1. Une crate **`qore-service`** existe, Tauri-free, dans le workspace.
2. Le **data plane** (connexion, requête lecture, schéma/listing, mutation, export) passe par `qore-service`. Les `commands/` correspondants sont des **wrappers fins**.
3. L'app desktop fonctionne **à l'identique** — zéro régression, `cargo test` vert, requêtes streaming OK.
4. Un **binaire de smoke test** (`examples/headless.rs`) fait `connect → execute_query → print` **sans aucune dépendance Tauri**. C'est la preuve que le core est réutilisable.

> **Hors scope de CE jalon** : le « plan applicatif » (workspace, plugins, contracts, instant_api, time_travel, share, import) reste dans `commands/` pour l'instant. MCP et CLI n'en ont pas besoin. Ces groupes suivront la même recette, plus tard.

---

## 2. Principe d'architecture

### Direction de dépendance (non négociable)

```
qoredb (app + bin Tauri)
   └── qore-service          ← NOUVEAU (Tauri-free)
         └── qore-core · qore-drivers · qore-sql · qore-query   (déjà extraits)
```

`qore-service` **ne dépend jamais de Tauri ni de l'app**. Toute la glue Tauri reste dans `qoredb/src/commands/` et `qoredb/src/lib.rs`.

### Le rôle de chaque couche

- **`qore-service`** contient :
  - `ServiceContext` (l'actuel `AppState`, déplacé) — détient les `Arc<...>` de services ;
  - la **logique métier** extraite de `commands/` (validation, safety, interceptor, rate limit, policy, metrics, orchestration) ;
  - les traits d'**événements** (`EventSink`) et le type d'**erreur** (`ServiceError`).
- **`commands/`** devient une couche d'adaptation fine. Chaque commande :
  1. `state.lock().await` → récupère le `ServiceContext` ;
  2. construit un sink Tauri (`TauriEventSink` / branche le `StreamDispatcher`) ;
  3. appelle `qore_service::…(ctx, params, sink)` ;
  4. mappe `ServiceError → String`.

### Deux familles d'événements (le point délicat)

| Famille | Déjà abstrait ? | Stratégie |
| --- | --- | --- |
| **Streaming** (colonnes, lignes, done, erreur) | ✅ **Oui** : `qore-core::StreamSender` (`mpsc::Sender<StreamEvent>`) est déjà Tauri-free. Le driver écrit dedans. | `qore-service` prend un `StreamSender` en paramètre. L'app draine le récepteur via le `StreamDispatcher` existant (`commands/stream_msg.rs`) → `Channel` IPC. CLI/MCP drainent le même `mpsc::Receiver` autrement (stdout, SSE…). **Rien à inventer.** |
| **Discret** (progress backup, health connexion, plugin-notify) | ⚠️ Partiel : `backup::runner::EventSink` (`fn emit(&self, job_id, event)`) existe déjà pour le backup. | Généraliser ce pattern en un trait `qore-service::EventSink`. L'app fournit une impl `TauriEventSink` (emit Tauri) ; CLI/MCP une impl no-op ou logger. |

C'est l'insight qui dé-risque le jalon : **le morceau réputé le plus dur (le streaming de requête) a déjà son abstraction Tauri-free.** Seul l'adaptateur `StreamDispatcher` est couplé, et il reste côté app.

---

## 3. Ce qui aide déjà (ne pas réinventer)

- `SessionManager` (`qore-drivers/src/session_manager.rs`) : déjà Tauri-free. `get_driver`, `is_read_only`, `connection_key`, `get_environment` — tout est pur.
- `qore-core::{StreamEvent, StreamSender}` : abstraction streaming prête.
- `backup::runner::EventSink` : pattern d'événements discrets prouvé.
- Tous les services tenus par `AppState` (`InterceptorPipeline`, `SafetyPolicy`, `QueryCache`, `QueryRateLimiter`, `QueryManager`, `LicenseManager`, `VirtualRelationStore`, `PluginHost`, `ExportPipeline`…) sont **déjà du Rust pur** : leur déplacement est mécanique.
- `sql_safety::analyze_sql` / `split_sql_statements` (`engine/sql_safety.rs`) : pures.

---

## 4. Découpage en étapes (l'app compile à CHAQUE étape)

### Étape 1 — Squelette de la crate + déplacement des modules purs ✅ FAIT
**Réalisé** :
- Crate `crates/qore-service` créée (deps : `qore-core`, `qore-sql`, `qore-drivers`, + `keyring`/`argon2`/`ed25519`/`sha2`/`csv`… selon les modules), avec son propre `build.rs` (injection de `PUBLIC_KEY_BASE64` pour `license/key.rs`).
- Modules déplacés (`git mv`, historique préservé) : `paths`, `metrics`, `ratelimit`, `policy`, `cache`, `sensitive`, `vault`, `license`, `interceptor`, `virtual_relations`.
- Imports `crate::engine::*` des modules déplacés redirigés vers les vrais crates (`qore_core::`, `qore_sql::safety::`) ; `crate::observability::Sensitive` → `crate::sensitive::Sensitive`.
- `qoredb/src/lib.rs` re-exporte ces modules (`pub use qore_service::{…}`) → **zéro churn** dans `commands/`.
- Note : `engine/sql_safety` et `engine/query_manager` n'étaient pas des fichiers app mais des re-exports de `qore-sql`/`qore-drivers` (façade `engine/mod.rs`) — rien à déplacer.

**Vérif** : `cargo check` app vert ; `cargo test -p qore-service` → 96 tests ; `cargo tree -i tauri` vide.

### Étape 2 — `ServiceContext` (composition) ✅ FAIT
**Décision de design** : `ServiceContext` est une struct **possédée et constructible seule** (`ServiceContext::new()`), pas un AppState déplacé. Raison : une surface CLI/MCP/serveur n'a pas d'`AppState` — elle veut instancier directement le strict nécessaire pour parler aux bases. Tout fusionner dans `qore-service` y ferait entrer des dépendances 100 % desktop (`wasmi`/plugins, spawn process/backup, file-watcher/workspace, jetons de confirmation Tauri). On **compose** plutôt : `ServiceContext` = data plane partagé ; l'`AppState` desktop l'embarque + ses services desktop.

**Réalisé** :
- `qore-service/src/context.rs` : `ServiceContext` détient registry + 13 drivers, session_manager, query_manager, rate_limiter, cache, policy, interceptor, virtual_relations, vault_lock, license_manager. `ServiceContext::new()` reprend la construction correspondante de l'ex-`AppState::new()`.
- L'app `AppState` devient `{ service: ServiceContext, plugin_host, export_pipeline, share_manager, ai_manager, changelog_store, backup_*, confirmation_tokens }`.
- **Pont transitoire** : `impl Deref/DerefMut for AppState { Target = ServiceContext }` → les ~40 fichiers `commands/` accèdent toujours à `state.session_manager` / `state.policy` / … sans modification. Le pont sera retiré une fois toutes les commandes data-plane routées via `qore_service` (fin Étapes 3-5).
- Un seul site corrigé : `commands/governance.rs` (`crate::QueryManager` → `crate::engine::QueryManager`).

**Vérif** : `cargo check` vert, 96 tests, `qore-drivers` feature `tauri` **non** activée pour `qore-service` standalone.

> `ServiceError` et `EventSink` ne sont **pas** créés à ce stade (aucun consommateur encore → code spéculatif). Ils arrivent à l'Étape 3, avec la première commande extraite.

### Étape 3 — Groupe « no-lift » (extraction mécanique)
Commandes à logique pure, sans événements : `cache`, `driver`, `license`, `policy` + `governance`, `snapshots`, `connection_url`, `interceptor`, `metrics`.

**Travail** : déplacer le corps de chaque commande en `fn` de `qore-service` ; la commande Tauri ne garde que `lock + appel + map erreur`. Exemple :
```rust
// qore-service
pub async fn list_drivers(ctx: &ServiceContext) -> Result<Vec<DriverInfo>, ServiceError> { … }
// commands/driver.rs
#[tauri::command]
pub async fn list_drivers(state: State<'_, SharedState>) -> Result<Vec<DriverInfo>, String> {
    let ctx = state.lock().await;
    qore_service::list_drivers(&ctx).await.map_err(|e| e.sanitized())
}
```

**Vérif** : build + tests ; les onglets correspondants fonctionnent dans l'app.

### Étape 4 — `connection` + `vault` (EventSink léger)
**Travail** : extraire `connect / disconnect / test_connection / list_sessions` et les opérations vault. Le seul couplage est l'émission d'événements de **santé de connexion** → passe par `&dyn EventSink`. La commande construit un `TauriEventSink` et le passe.

**Vérif** : l'app se connecte/déconnecte à toutes les BDD ; les events de health remontent toujours dans l'UI.

### Étape 5 — Data plane streaming : `query` (lecture) + `mutation` + `export`
Le cœur. `execute_query` (~700 lignes) devient :
```rust
// qore-service
pub async fn execute_query(
    ctx: &ServiceContext,
    params: ExecuteQueryParams,
    stream: StreamSender,        // déjà Tauri-free (qore-core)
    events: &dyn EventSink,      // pour les events discrets éventuels
) -> Result<QueryResponse, ServiceError> { … }
```
**Travail** :
- Déplacer toute l'orchestration inline (rate limit, read-only, sql_safety, interceptor pre/post, plugin hooks, concurrence, timeout, cancel, multi-statement) dans `qore-service`. Ces appels visent déjà des services purs.
- La commande Tauri garde **uniquement** : la création du `mpsc` + le `StreamDispatcher` qui draine vers le `Channel` IPC (`commands/stream_msg.rs` inchangé), puis l'appel au service.
- Idem pour `mutation` et `export` (l'`ExportPipeline` reçoit un `StreamSender`/`EventSink` au lieu de `window`).

**Vérif (critique)** : requêtes streaming, gros résultats, annulation, timeout, multi-statement, read-only — tout testé dans le desktop. C'est l'étape qui peut révéler des couplages cachés.

### Étape 6 — Smoke test headless + gel du reste
**Travail** :
- `crates/qore-service/examples/headless.rs` : `connect` (config en dur) → `execute_query` avec un `StreamSender` drainé en stdout → afficher les lignes. **Sans Tauri.**
- Documenter la **recette** (les étapes 3-5) pour les groupes restés dans `commands/` (workspace, plugins, contracts, instant_api, time_travel, share, import, ai, federation), à extraire au fil des surfaces qui en auront besoin.

**Vérif** : `cargo run --example headless` affiche des lignes d'une vraie BDD.

---

## 5. Le contrat `qore-service` (API publique cible)

Surface minimale visée à la fin du jalon (le data plane) :

```rust
// connexion
pub async fn test_connection(ctx, config) -> Result<(), ServiceError>;
pub async fn connect(ctx, config, events) -> Result<SessionId, ServiceError>;
pub async fn disconnect(ctx, session) -> Result<(), ServiceError>;
pub async fn list_sessions(ctx) -> Result<Vec<SessionInfo>, ServiceError>;

// requête / schéma
pub async fn execute_query(ctx, params, stream, events) -> Result<QueryResponse, ServiceError>;
pub async fn cancel_query(ctx, query_id) -> Result<(), ServiceError>;
pub async fn list_namespaces(ctx, session) -> Result<…>;
pub async fn describe_table(ctx, session, table) -> Result<…>;

// mutation / export
pub async fn insert_row / update_row / delete_row(ctx, …) -> Result<…>;
pub async fn start_export(ctx, params, stream, events) -> Result<…>;

// transverse
pub async fn list_drivers(ctx) -> Result<…>;
pub async fn license_status(ctx) -> Result<…>;
```

C'est ce contrat que MCP (Jalon 1), CLI (Jalon 2) et `qore-server` (Jalon 4) consommeront tel quel.

---

## 6. Risques spécifiques au Jalon 0

- **Dépendances circulaires pendant les déplacements** (Étape 1). Mitigation : déplacer feuille par feuille (les modules sans dépendance d'abord : `paths`, `metrics`, `ratelimit`, `policy`, `cache` ; puis `vault`, `interceptor` ; enfin `query_manager`). Re-export depuis `lib.rs` pour éviter le churn de chemins.
- **Le verrou global `Arc<Mutex<AppState>>`**. Ne pas changer la sémantique de locking pendant l'extraction — déplacer la struct, pas le modèle de concurrence. Optimisations de lock = un autre chantier.
- **La densité de `execute_query`** (timeout/cancel/multi-statement). Mitigation : extraire en conservant la structure exacte, s'appuyer sur les tests existants ; ne pas « améliorer » au passage (modifications chirurgicales).
- **Feature flags `pro`**. `ai`, `federation`, `contracts`, `instant_api` sont `#[cfg(feature = "pro")]`. Préserver les gates lors du déplacement ; le smoke test se compile en Core.
- **`paths.rs` et le vault** : le vault dépend des chemins app data. Déplacer `paths.rs` dans `qore-service` (ou une micro-crate `qore-paths`) pour que le vault soit autoportant.
- **Headers SPDX** : chaque nouveau fichier `qore-service` = `// SPDX-License-Identifier: Apache-2.0`.

---

## 7. Checklist de fin de jalon

- [ ] `crates/qore-service` compile, sans `tauri` dans son arbre de dépendances (`cargo tree -p qore-service | grep tauri` → vide).
- [ ] Data plane (connection, query lecture, schéma, mutation, export) servi par `qore-service`.
- [ ] `commands/` correspondants = wrappers (`lock + appel + map`), aucun > 500 lignes.
- [ ] `cargo test` vert en Core **et** `--features pro`.
- [ ] Desktop : connexion, requête streaming, annulation, timeout, mutation, export — vérifiés manuellement, sans régression.
- [ ] `cargo run -p qore-service --example headless` affiche des lignes d'une vraie BDD, sans Tauri.
- [ ] Recette documentée pour les groupes restants.

---

*Plan d'implémentation. À cocher au fil de l'avancement.*
