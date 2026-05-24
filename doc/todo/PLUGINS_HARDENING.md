# Plan — Durcir le système de plugins (objectif 9/10+)

Plan d'action pour amener le système de plugins de **~6.3/10** à **9/10+**,
en combinant l'audit interne (Claude) et l'audit externe (Codex). Découpé en
6 phases priorisées, chacune avec ses critères d'acceptation et son impact
sur les notes.

> **État de départ** : v0.1.29, runtime `wasmi`, capabilities `log/notify/storage/queryRead/http/fs/secrets`, manifests validés, contributions namespacées. Tests `cargo test plugins --lib` : 39 passed.

---

## 0. Synthèse des notes actuelles

| Critère | Claude | Codex | Combiné |
| --- | --- | --- | --- |
| Fonctionnel | 7 | 6 | **6.5** |
| Sécurité | 8 | 7 | **7.5** |
| Robustesse | 7.5 | 5.5 | **6.5** |
| Flexibilité | 5.5 | 7 | **6** |
| Performances | 6 | 5 | **5.5** |
| DX / écosystème | 6 | 6 | **6** |
| Maintenabilité / tests | — | 6.5 | **6.5** |
| Distribution | 3 | — | **3** |
| **Global** | **7** | **6.3** | **~6.5** |

---

## 1. Problèmes identifiés

### 1.1 Écarts fonctionnels (« déclaré mais pas honoré »)

| # | Problème | Fichier | Sévérité |
| --- | --- | --- | --- |
| F1 | `Decision::Warn` calculé par le runtime mais **ignoré** par `execute_query`. Seul `Block` est traité. | `src-tauri/src/commands/query.rs:487` | Bug |
| F2 | `postExecute` n'est invoqué **que sur succès** ; les erreurs et timeouts court-circuitent le hook. | `src-tauri/src/commands/query.rs:780` (+ chemins streaming, error, timeout) | Bug |
| F3 | `connectionTemplates` agrégés par `get_contributions` mais **jamais consommés** par le formulaire de connexion. | `src/components/Connection/ConnectionModal.tsx` | Fonctionnalité morte |
| F4 | Renderers `map` et `chart` acceptés par la validation manifeste mais **retombent sur texte** côté UI. | `src/components/Grid/PluginCellRenderer.tsx:29-31` | Fonctionnalité morte |
| F5 | `ConsentDialog` ne se déclenche post-install que pour `log/notify/storage/queryRead` — `http/fs/secrets` passent à travers. | `src/components/Plugins/InstallPluginDialog.tsx:58-59` | Bug |
| F6 | `queryExec` parsé et exposé dans le manifeste mais host fn retourne `denied` (Phase 5 non implémentée). | `src-tauri/src/plugins/runtime/capabilities.rs:18-25` | Trompeur |

### 1.2 Robustesse

| # | Problème | Fichier | Sévérité |
| --- | --- | --- | --- |
| R1 | Installation **non atomique** : `remove_dir_all(target)` puis copie — si la copie échoue, l'ancien plugin est perdu. | `src-tauri/src/plugins/registry.rs:81-90` | Risque de perte |
| R2 | Pas de tests d'intégration WASM bout-en-bout (manifeste + hooks + host fns + capabilities). | `src-tauri/tests/` | Couverture |
| R3 | `Mutex` poisoning sur `instances` / `notify` non géré (`.unwrap()`). | `src-tauri/src/plugins/runtime/manager.rs` | Edge case |
| R4 | Pas de circuit breaker sur les hooks qui trap en boucle (re-trap à chaque requête). | `manager.rs` | UX dégradée |

### 1.3 Sécurité

| # | Problème | Fichier | Sévérité |
| --- | --- | --- | --- |
| S1 | **SSRF** : l'allowlist HTTP ne refuse pas les IPs privées / loopback / cloud-metadata après résolution DNS. | `src-tauri/src/plugins/runtime/host_fns.rs:236-254` | Élevée |
| S2 | Pas de signature ni de checksum `sha256` du `.wasm` — tout `plugin-id` est squattable. | `src-tauri/src/plugins/runtime/wasmi_host.rs` | Élevée |
| S3 | Pas de **wall-clock timeout** sur une invocation : fuel borne le calcul WASM, pas les host fns bloquantes cumulées (HTTP 10 s × N). | `host_fns.rs` | Moyenne |
| S4 | Refus de capability silencieux côté plugin (`ERR_DENIED`) — pas de log côté host. Plugin malveillant qui tente d'accéder à du non-accordé passe inaperçu. | `host_fns.rs` | Faible |

### 1.4 Performances

| # | Problème | Fichier | Impact |
| --- | --- | --- | --- |
| P1 | `wasmi` interpréteur, pas de JIT → 10-50× plus lent que `wasmtime` sur du calcul. `preExecute` tourne **à chaque requête**. | `runtime/wasmi_host.rs` | Latence query |
| P2 | Hooks dans le chemin async critique : un `postExecute` lent retarde la réponse au frontend. | `commands/query.rs:780` | Latence query |
| P3 | `Mutex` global sur `instances` → les hooks de plugins **différents** sérialisent. | `runtime/manager.rs:27` | Concurrence |
| P4 | `get_contributions()` re-scanne le disque à chaque appel frontend. | `plugins/registry.rs:124` | I/O inutile |
| P5 | Pas de pool d'instances WASM : `Store` neuf par invocation. | `runtime/wasmi_host.rs:179` | CPU |

### 1.5 Flexibilité / surfaces

| # | Problème | Impact |
| --- | --- | --- |
| X1 | Surfaces de contribution figées (5 types) — pas de mécanisme générique pour étendre. |
| X2 | Pas de hooks `preConnect` / `onSchemaBrowse` / `preExport`. |
| X3 | Pas de communication inter-plugins. |
| X4 | Result viewers limités à 4 renderers built-in — pas de composant React custom. |
| X5 | Pas de SDK pour AssemblyScript / TinyGo / JS (Rust→WASM uniquement). |

### 1.6 DX / Distribution

| # | Problème | Impact |
| --- | --- | --- |
| D1 | Pas de CLI `qoredb-plugin new / build / install`. |
| D2 | `plugins-dev/README.md` ne reflète pas les capabilities actuelles (http/fs/secrets/commands/result viewers manquants). |
| D3 | Pas de spec ABI séparée (`plugins-dev/ABI.md`). |
| D4 | Pas de mock host pour tester un plugin sans bâtir le WASM. |
| D5 | Install uniquement depuis un dossier local — pas de registry, pas d'URL d'install, pas d'auto-update. |

---

## 2. Plan d'exécution

### Phase 1 — Combler les écarts fonctionnels — ✅ livrée

> Toucher uniquement ce que la grille promet déjà. **Impact** : Fonctionnel 6.5 → 8.5 · Flexibilité 6 → 7.
>
> **Statut** : F1-F6 implémentés. Tests Rust : 220 passed (40 dans `plugins`, +1 nouveau test pour le rejet `queryExec`). TypeScript et Biome lint verts. Test E2E mini-plugin WASM reporté à la Phase 2 (item R2).

| Item | Action | Critère d'acceptation |
| --- | --- | --- |
| F1 | Surface le `Warn { message }` du plugin à l'utilisateur. Choix : émettre un `NotifyEvent { level: Warning }` via le canal `plugin-notify` existant — pas de modification de `QueryResponse`. | Un plugin renvoyant `Decision::Warn("...")` déclenche un `toast.warning("...")` côté frontend. Test E2E avec un mini-plugin Warn. |
| F2 | Déplacer `dispatch_plugin_post_execute` après le `match result`, alimenter `success: false, error: Some(_)` sur erreur/timeout. Idem pour le chemin streaming. | Un plugin reçoit un `postExecute` avec `success=false` en cas d'erreur DB ou timeout. Test E2E. |
| F3 | Composant `ConnectionTemplatePicker` ajouté à `ConnectionModal` (étape form). Filtré par driver actif, applique les `defaults` via le setter du formulaire. i18n FR/EN. | Sélection d'un template Postgres pré-remplit host/port/database. Vitest sur le composant. |
| F4 | Implémenter `MapCell` (Leaflet ou `maplibre-gl` s'il est déjà dans les deps, sinon placeholder explicite + log) et `ChartCell` (recharts). | Une cellule contribuée renderer=`map` avec `{lat,lon}` affiche une carte ; `chart` avec `{type:"bar",data}` affiche le graphe. Sinon, fallback explicite « renderer non disponible ». |
| F5 | Réécrire le test `wantsConsent` dans `InstallPluginDialog.tsx` pour utiliser `requestedCaps(plugin)` (déjà exporté/exportable depuis `ConsentDialog`). | L'installation d'un plugin demandant uniquement `http` déclenche le dialog post-install. |
| F6 | **Décision** : retirer `queryExec` du type `PluginCapabilities` (Rust + TS) tant que non câblé — éviter la confusion. Documenter dans `PLUGIN_RUNTIME.md` comme futur ajout. | `queryExec` n'apparaît plus dans le manifest schema ; un plugin l'ayant échoue à la validation avec un message clair. |

**Tests à ajouter** :
- `cargo test` : extension manifest pour F6 (rejet `queryExec`).
- `cargo test` : tests E2E avec mini-plugin WASM (préparé en Phase 2 item R2).
- Vitest : `InstallPluginDialog` déclenche consent pour chaque capability.

### Phase 2 — Robustesse de l'install et tests E2E — ✅ livrée

> **Impact** : Robustesse 6.5 → 9 · Maintenabilité 6.5 → 8.5.
>
> **Statut** : R1-R4 implémentés. Tests Rust : 48 dans `plugins` (lib) + 11 dans la nouvelle suite E2E `tests/plugins_e2e.rs`. Item « Tests UI (Vitest) » reporté : Vitest n'est pas installé dans le repo, l'introduction de l'infrastructure dépasse le scope de cette phase.

| Item | Action | Critère d'acceptation |
| --- | --- | --- |
| R1 ✅ | Refonte de `install_plugin` : copie vers `<id>.qoredb-staging`, rename atomique, ancien déplacé en `<id>.qoredb-backup` purgé sur succès. Rollback sur échec. Nettoyage des leftovers au démarrage de chaque install. | 5 nouveaux tests dans `registry.rs` : fresh install, overwrite, rollback sur budget dépassé, cleanup de staging/backup stale, `list_plugins` ignore les leftovers. |
| R2 ✅ | Suite `tests/plugins_e2e.rs` (11 tests) construite avec des modules WAT inline via le crate `wat` (dev-dep). Pas de toolchain `wasm32` requis. | E2E : preExecute allow/warn/block, module sans `pre_execute`, trap, fuel exhaustion, storage capability granted/denied, HTTP rejeté hors allowlist, FS rejeté hors scope, postExecute succès+erreur. |
| R3 ✅ | Helper `lock_recover` qui consigne le poisoning via `tracing::error!` et récupère via `into_inner()`. Tous les `.lock().unwrap()` du `PluginHost` remplacés. | Empoisonnement d'un Mutex n'arrête plus le `PluginHost`. |
| R4 ✅ | Compteur d'échecs par plugin (`failures: HashMap<String, u32>`). Au-delà de 3 erreurs consécutives, le plugin est retiré de `instances` et un toast `Warning` est émis. Succès remet le compteur à zéro. | 3 nouveaux tests unitaires dans `manager.rs` : circuit-breaker pre_execute, reset au succès, circuit-breaker post_execute. |
| — | Tests UI (Vitest) | **Reporté** : Vitest pas dans le repo, à traiter dans une PR dédiée infra-tests. |

### Phase 3 — Sortir les hooks du chemin critique — ✅ livrée

> **Impact** : Performances 5.5 → 8.5 · Robustesse 6.5 → 9.
>
> **Statut** : P2, P3, P4, S3 implémentés. Tests Rust : 51 dans `plugins` (lib) + 11 E2E. `run_pre_execute` / `run_post_execute` / `run_command` deviennent `async`, ce qui propage à `query.rs` (un seul `.await` ajouté en pre_execute) et à `plugins.rs::run_plugin_command`.

| Item | Action | Critère d'acceptation |
| --- | --- | --- |
| P2 ✅ | Nouvelle méthode `PluginHost::schedule_post_execute(self: &Arc<Self>, ...)` qui fire-and-forget via `tokio::spawn` sous un `Semaphore` borné (`POST_EXECUTE_QUEUE_DEPTH = 64`). `dispatch_plugin_post_execute` (query.rs) bascule sur cette voie. | Test `schedule_post_execute_returns_immediately_and_eventually_runs_the_hook` (<50 ms de retour, hook exécuté en arrière-plan). Drop avec log si queue saturée. |
| S3 ✅ | `tokio::time::timeout` autour de chaque invocation (`PRE_EXECUTE_TIMEOUT = 500ms` / `POST_EXECUTE_TIMEOUT = 5s`). Helper `run_with_timeout` qui aplatit timeout / panic / erreur plugin en un seul `HookOutcome::Failed`. | Test `pre_execute_timeout_treats_plugin_as_failed_without_stalling_the_caller` : un plugin qui sleep(2.5s) ne bloque plus la query au-delà du budget. |
| P3 ✅ | Verrou par plugin : `Mutex<HashMap<String, Arc<Mutex<Box<dyn PluginInstance>>>>>`. `snapshot_instances` clone les Arcs sous le verrou outer, le drop, puis chaque hook s'exécute dans `spawn_blocking` avec son propre verrou inner. | Plusieurs queries simultanées sur des plugins différents ne contendent plus sur un Mutex global. |
| P4 ✅ | Cache mémoire `contributions_cache: Mutex<Option<Arc<PluginContributions>>>` sur `PluginHost`. `reload()` (point d'invalidation unique : install/remove/enable/disable/consent) le vide. `get_plugin_contributions` consomme désormais le cache. | Test `contributions_cache_is_shared_across_calls_and_cleared_on_reload`. Frontend lit les contributions sans I/O après le premier appel. |

### Phase 4 — Sécurité avancée — ✅ livrée

> **Impact** : Sécurité 7.5 → 9.5.
>
> **Statut** : S1, S2, S4 et audit ordre des checks livrés. Tests Rust : 60 unit dans `plugins` + 12 E2E.

| Item | Action | Critère d'acceptation |
| --- | --- | --- |
| S1 ✅ | Helper `is_private_destination(IpAddr)` (loopback / RFC1918 / link-local / cloud metadata 169.254 / CGNAT 100.64-127 / IPv6 ULA + link-local + mapped). Après le filtre allowlist d'hôtes dans `qoredb_http_request`, résolution sync via `ToSocketAddrs` puis refus si une IP retombe dans une plage interne. Manifest : `http.allowPrivateNetworks: true` est l'escape hatch. ConsentDialog surface un encart Warning quand le flag est demandé. | 3 tests unitaires (private / public / mapped) + 1 E2E (`http_request_to_a_loopback_address_is_rejected_by_the_ssrf_guard`). |
| S2 ✅ | Champ `runtime.integrity: "sha256-<64 hex>"` ajouté à `RuntimeSpec`. Validé au parse manifest (préfixe + 64 hex lowercase). Vérifié à `reload()` via `sha2::Sha256` ; mismatch = refus de chargement + log warn. Badge UI `Signed / Unsigned` dans `PluginDetailDialog`. | 4 tests manifest (accept / no-prefix / wrong-length / uppercase) + 2 tests `verify_integrity` (match / tamper). |
| S4 ✅ | Helper `has_capability(&Caller, CapabilityKind)` qui consigne `tracing::warn!(plugin=..., capability=...)` à chaque refus. Toutes les host fns refactorées pour l'utiliser. | Inspection par diff : la dénégation produit désormais une ligne de log lisible. |
| Audit ✅ | Doc `doc/audits/PLUGIN_CAPABILITY_CHECKS.md` qui certifie que la vérification de capability est la **première** instruction de chaque host fn (11 host fns, table de conformité). | Doc livrée ; régression détectable au diff review. |

### Phase 5 — Performances ciblées

> **Impact** : Performances 8.5 → 9.

| Item | Action | Critère d'acceptation |
| --- | --- | --- |
| P1 | Migration `wasmi → wasmtime`. Le trait `PluginRuntime` est déjà conçu pour. Garder fuel + memory_size_bytes. | Bench preExecute sur un linter complexe : -90 % de temps CPU. |
| P5 | Optionnel : pool d'instances chaudes (2 par plugin) avec reset de fuel. À jauger sur le bench post-wasmtime. | Si gain mesuré ≥ 30 %, mergé. Sinon, on s'arrête à P1. |

### Phase 6 — DX, écosystème, gouvernance

> **Impact** : DX 6 → 8.5 · Distribution 3 → 6.

| Item | Action | Critère d'acceptation |
| --- | --- | --- |
| D1 | Binaire `qoredb-plugin` dans `plugins-dev/cli/` : `new <id>` (scaffolding), `build` (cargo + sha256 + copy `.wasm`), `install <path>` (Tauri IPC). | `qoredb-plugin new acme.foo && cd acme.foo && qoredb-plugin build && qoredb-plugin install` fonctionne. |
| D2 | Refonte `plugins-dev/README.md` : refléter http/fs/secrets/commands/result viewers, section debug. | Lecture du README permet d'écrire un plugin HTTP en partant de zéro. |
| D3 | `plugins-dev/ABI.md` : spec exports requis, format packed i64, codes d'erreur. | Document de référence partageable. |
| D4 | Crate `qoredb-plugin-sdk-test` : mock host pour `cargo test` côté plugin. | Le sql-linter peut tester son `check()` sans WASM. |
| D5 (partiel) | Manifest schema JSON publié (`plugin.schema.json`). Référence dans le `$schema` du manifeste exemple. | Autocompletion VS Code. |
| D5 | Trust model documenté (`doc/internals/PLUGIN_TRUST.md`) : badge « non signé », guide utilisateur. | Documentation visible dans le `PluginDetailDialog`. |

---

## 3. Hors-périmètre court terme

À considérer plus tard, **pas** dans le périmètre « 9/10 » :

- Registry distant (charge produit + légale).
- Renderers React custom via WASM (risque XSS).
- Hooks `preConnect` / `onSchemaBrowse` (surfaces utiles mais sans impact note).
- SDK AssemblyScript / TinyGo.

---

## 4. Projection des notes après plan complet

| Critère | Avant | Phase 1 | Phase 3 | Phase 5 | Final |
| --- | --- | --- | --- | --- | --- |
| Fonctionnel | 6.5 | 8.5 | 9 | 9 | **9** |
| Sécurité | 7.5 | 7.5 | 8 | 9.5 | **9.5** |
| Robustesse | 6.5 | 7 | 9 | 9 | **9** |
| Flexibilité | 6 | 7 | 7.5 | 7.5 | **7.5** |
| Performances | 5.5 | 5.5 | 8.5 | 9 | **9** |
| DX | 6 | 6.5 | 7 | 7 | **8.5** |
| Distribution | 3 | 3 | 3 | 3 | **6** |
| Doc | 6 | 7 | 7.5 | 8 | **8.5** |
| **Global** | **~6.5** | **7.3** | **8.2** | **8.8** | **~9** |

**Atteindre 9.5+** demanderait registry signé + auto-update. Hors périmètre.

---

## 5. Sprints suggérés

| Sprint | Contenu | Notes ciblées |
| --- | --- | --- |
| **S1 (1 semaine)** | Phase 1 + Phase 2 (R1, R2, R3) | Fonctionnel 8.5, Robustesse 9 |
| **S2 (1 semaine)** | Phase 3 + Phase 4 | Sécurité 9.5, Perfs 8.5 |
| **S3 (1 semaine)** | Phase 5 + Phase 6 | Perfs 9, DX 8.5, Distribution 6 |

---

## 6. Critères de revue par phase

Chaque phase est mergée seulement si :
- `cargo test plugins --lib` passe sans régression.
- `cargo test --test plugins_e2e` passe (dès Phase 2).
- Frontend : `pnpm lint`, `pnpm test` verts.
- `pnpm tauri dev` lance, l'install d'un plugin de fixture fonctionne bout en bout.
- Pas de nouveau warning `tracing::error!` au boot.
- `doc/todo/PLUGINS_HARDENING.md` mis à jour : items cochés, notes recalculées.

---

## 7. Références code

- Manifest : `src-tauri/src/plugins/manifest.rs`
- Registry : `src-tauri/src/plugins/registry.rs`
- Runtime : `src-tauri/src/plugins/runtime/`
- Commandes Tauri : `src-tauri/src/commands/plugins.rs`, `query.rs`
- Bindings TS : `src/lib/plugins/`
- UI : `src/components/Plugins/`
- SDK : `plugins-dev/sdk/src/lib.rs`
- Plan d'origine : `doc/todo/PLUGIN_RUNTIME.md`
