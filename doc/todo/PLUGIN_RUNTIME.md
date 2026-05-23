# Plan — Runtime de plugins exécutables (WASM)

Suite de la fondation déclarative livrée en v0.1.29. Ce document spécifie la
couche **exécutable** : des plugins qui étendent le *comportement* de QoreDB,
dans un bac à sable, sans dégrader la sécurité, les performances ni le poids.

## 1. Décision d'architecture

| Question | Décision | Raison |
| -------- | -------- | ------ |
| Runtime | **`wasmi`** (interpréteur WASM pur Rust) | Isolation mémoire constitutive ; ~1 Mo de binaire ; métrage natif. |
| Code exécutable | **WASM uniquement** — jamais de JS de plugin dans la webview | La webview porte le pont Tauri (accès vault, SQL, FS) ; on garde le code tiers loin d'elle. |
| UI de plugin | **Déclarative** (schéma typé), pas de code | Ne rouvre aucune surface JS. |
| Capabilities | **Médiées par le backend Rust**, défaut zéro | Le plugin demande, l'hôte vérifie et exécute. |
| Upgrade perf | Trait `PluginRuntime` → `wasmtime` possible plus tard | Décision repoussée, pas verrouillée. |

**Principe directeur** : un plugin sans capability accordée ne *peut* rien
faire de dangereux — par construction, pas par vérification.

## 2. Vue d'ensemble

```
Plugin = plugin.json (manifeste v0.1.29) + plugin.wasm
   │
   ▼
wasmi  (src-tauri/src/plugins/runtime/)   ← mémoire linéaire isolée
   │  fonctions hôte importées = surface de capability
   ▼
Backend Rust  ← exécute la capability (SQL, HTTP, FS scopé)
   ▲
   │  hooks branchés sur l'intercepteur
Pipeline d'intercepteur (interceptor/pipeline.rs) : pre_execute / post_execute
```

Un utilisateur sans plugin ne paie rien : le runtime n'est jamais instancié.
Seul coût universel : ~1 Mo de binaire.

## 3. Extension du manifeste

Le `PluginManifest` v0.1.29 reçoit un bloc **optionnel** `runtime`. Les plugins
déclaratifs existants (`runtime` absent) continuent sans changement.

```json
{
  "id": "acme.sql-linter",
  "name": "SQL Linter",
  "version": "1.0.0",
  "qoredb": ">=0.2.0",
  "runtime": {
    "abiVersion": 1,
    "entry": "plugin.wasm",
    "hooks": ["preExecute", "postExecute"],
    "capabilities": {
      "log": true,
      "queryRead": true,
      "http": { "allowedHosts": ["hooks.slack.com"] },
      "fs": { "scope": "pluginData" }
    }
  },
  "contributes": { "commands": [ ... ] }
}
```

## 4. Modèle de capabilities

Défaut : **rien**. Chaque capability = un jeu de fonctions hôte exposées à
l'instanciation. Un module qui importe une fonction non accordée **échoue à
l'instanciation** — il n'y a pas de global à oublier.

| Capability | Donne accès à | Risque | Gating |
| ---------- | ------------- | ------ | ------ |
| `log` | Écrire dans le journal du plugin | Nul | Défaut activable |
| `notify` | Afficher un toast QoreDB | Nul | Défaut activable |
| `storage` | Magasin clé-valeur géré par l'hôte | Nul | Défaut activable |
| `queryRead` | Lire lignes/métadonnées du résultat courant | Lecture de données | Consentement |
| `http` | Requêtes sortantes, **allow-list d'hôtes** | Exfiltration | Consentement + allow-list |
| `fs` | Lire/écrire **dans le dossier du plugin uniquement** | Limité | Consentement |
| `secrets` | Lire un secret nommé provisionné par l'utilisateur | Le secret lui-même | Consentement |
| `queryExec` | Exécuter une nouvelle requête | **Élevé** (DROP…) | Consentement fort, désactivé par défaut |

Double application : (1) la fonction hôte n'existe que si accordée ;
(2) la fonction hôte **revalide ses arguments** côté Rust (l'allow-list `http`,
le scope `fs`). Pas de confiance dans l'entrée du plugin.

Consentement : un dialogue à l'installation liste les capabilities demandées ;
révocables depuis Settings → Extensions. Réutilise le `PluginDetailDialog`.

## 5. Hooks — branchement sur l'intercepteur

Le pipeline `interceptor/pipeline.rs` est le seam existant.

- **`preExecute(ctx)`** — appelé dans `pipeline::pre_execute`, après les
  contrôles de sécurité. Le plugin retourne `Allow` | `Warn(message)` |
  `Block(reason)`. `ctx` reprend le `QueryContext` (requête, opération,
  environnement, mutation…). Plusieurs plugins → exécutés en série, un `Block`
  l'emporte.
- **`postExecute(ctx, resultMeta)`** — appelé dans `pipeline::post_execute`.
  Reçoit les métadonnées (succès, durée, lignes). L'accès au contenu des lignes
  exige la capability `queryRead`.
- **`command(id, args)`** — action invoquée par l'utilisateur (palette /
  bouton). Contribuée via `contributes.commands`.

Robustesse : un trap WASM ou un dépassement de budget dans un hook **n'échoue
jamais la requête** — le plugin est désactivé pour la session, l'erreur est
remontée dans l'UI, la requête continue.

Hors périmètre initial (chemin chaud) : `transformResult` ligne à ligne. Si
besoin un jour, capability dédiée, opt-in, explicitement marquée coûteuse.

## 6. Métrage & limites de ressources

| Limite | Mécanisme | Dépassement |
| ------ | --------- | ----------- |
| CPU | Fuel `wasmi` (budget d'instructions / invocation) | Trap → plugin désactivé |
| Mémoire | Plafond de la mémoire linéaire / instance | Trap |
| Temps mur | Watchdog par invocation | Interruption → trap |

Exécution sur `spawn_blocking` — jamais de blocage de la boucle tokio ni de
l'UI. Une instance réutilisée par plugin (ou pool court).

## 7. ABI & SDK auteur

Le coût de WASM, c'est le marshalling via mémoire linéaire. On l'absorbe :

- **ABI v1** : appels hôte/invité par `(ptr, len)`, charge utile en JSON
  (simple, débogable). MessagePack possible en ABI v2 si besoin de compacité.
  Champ `abiVersion` dans le manifeste pour la compatibilité.
- **SDK** — cache entièrement le marshalling de pointeurs :
  - `crates/qoredb-plugin-sdk` (Rust) — cible d'auteur recommandée.
  - paquet AssemblyScript (TS-like) — phase 3, pour élargir l'audience.

  L'auteur écrit une fonction typée :

  ```rust
  #[qoredb_plugin::hook(pre_execute)]
  fn check(ctx: QueryContext) -> Decision {
      if ctx.is_mutation && !ctx.query.to_uppercase().contains("WHERE") {
          return Decision::block("UPDATE/DELETE sans WHERE");
      }
      Decision::allow()
  }
  ```

## 8. Contributions UI déclaratives

Le plugin **décrit** l'UI, il ne l'exécute pas. Les rendus sont intégrés à
QoreDB ; le plugin ne fait que les câbler.

- `resultViewers` — `{ match: { columnType | namePattern }, renderer:
  "map" | "json-tree" | "image" | "chart", options }`.
- `commands` — `{ id, label, hook }`.
- `panels` déclaratifs — phase ultérieure.

Conséquence : **zéro code de plugin dans la webview**, modèle de sécurité
uniforme.

## 9. Découpage fichiers

```
src-tauri/src/plugins/
├── mod.rs            # + champ runtime: Option<PluginRuntime>
├── manifest.rs       # + parsing/validation du bloc runtime
├── registry.rs       # inchangé (scan/install/enable)
├── runtime/          # NOUVEAU
│   ├── mod.rs        # trait PluginRuntime + types Decision/HookContext
│   ├── wasmi_host.rs # implémentation wasmi
│   ├── abi.rs        # marshalling (ptr,len) ↔ types
│   ├── host_fns.rs   # catalogue des fonctions hôte (= capabilities)
│   └── metering.rs   # fuel, mémoire, watchdog
└── capabilities.rs   # déclaration, consentement, application

crates/qoredb-plugin-sdk/   # NOUVEAU — SDK auteur (Rust)
```

Intégrations : `interceptor/pipeline.rs` (appel des hooks), `lib.rs`
(`AppState` reçoit le registre runtime), `commands/plugins.rs` (consentement,
journaux), frontend (dialogue de consentement, vue des journaux de plugin,
registre des `resultViewers`).

## 10. Comment chaque contrainte est tenue

- **Sécurité** — isolation mémoire WASM ; capabilities défaut-zéro ;
  revalidation côté hôte ; aucun JS tiers dans la webview ; consentement
  explicite ; métrage obligatoire.
- **Performance** — hooks événementiels uniquement (jamais le chemin chaud) ;
  fuel + timeout bornent chaque invocation ; `spawn_blocking` ; coût nul sans
  plugin.
- **Poids plume** — `wasmi` ≈ 1 Mo, pas de codegen ; aucun moteur JS ajouté ;
  `wasmtime` (~5-10 Mo) explicitement écarté.

## 11. Roadmap par phases

| Phase | Contenu | Effort | Vérification |
| ----- | ------- | ------ | ------------ |
| **1 — Squelette** | Trait `PluginRuntime` + `wasmi` ; bloc `runtime` du manifeste ; instanciation ; métrage (fuel/mémoire/timeout) ; hook `preExecute` *pur* (zéro capability) ; SDK Rust v0. Dogfood : un linter SQL livré. | M-L | Le linter bloque un `DELETE` sans `WHERE` ; un plugin en boucle est trappé sans figer l'app. |
| **2 — Capabilities** | Catalogue de fonctions hôte ; dialogue de consentement ; `log` / `notify` / `storage` / `queryRead` ; `postExecute` câblé à l'intercepteur. | L | Un plugin sans `queryRead` ne voit pas les lignes ; consentement requis à l'installation. |
| **3 — Capabilities sensibles** | `http` (allow-list) ; `fs` (scopé au dossier du plugin) ; `secrets` ; SDK AssemblyScript. | M | Un `http` hors allow-list est refusé côté hôte ; `fs` ne sort pas du scope. |
| **4 — UI déclarative** | `resultViewers` (map, json-tree, image, chart) ; `commands`. | M | Une colonne GeoJSON s'affiche sur une carte via un plugin. |
| **5 — Distribution** | Signature des plugins ; registre/marketplace ; capability `queryExec` ; option `wasmtime`. | L+ | Vérification de signature ; installation depuis le registre. |

## 12. Licence

Runtime, SDK et fondation déclarative = **Core** (`Apache-2.0`). Point de
décision ouvert : le marketplace (phase 5) et certaines capabilities avancées
pourraient relever du Premium (`BUSL-1.1`) — à trancher avant la phase 5.

## 13. Non-objectifs (explicites)

- Aucun code JS de plugin, à aucune phase.
- Pas de marketplace avant la phase 5.
- Plugins « source de données » (un plugin comme driver) : envisageable mais
  hors de ce plan — surface de capability trop large, à spécifier séparément.
- `wasmtime` : chemin d'upgrade derrière le trait, pas un objectif initial.
 