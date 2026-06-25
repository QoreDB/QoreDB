# Plan v0.1.29 — Extensibilité, performance & parcours produit

---

## 📦 Périmètre

| # | Feature | Effort | Licence | Source |
| - | ------- | ------ | ------- | ------ |
| 0 | Pro update + onboarding + SQLite (déjà sur la branche) | — | mixte | branche `feat/pro-update` |
| 1 | Query Result Caching | M | Core (`Apache-2.0`) | `v3.md` § Performance |
| 2 | Security Hardening (FS scope + Rate Limiting) | M | Core (`Apache-2.0`) | `v3.md` § Sécurité + `SECURITY_AUDIT.md` 5.6 |
| 3 | Plugin System Foundation (déclaratif) | L | Core (`Apache-2.0`) | `v3.md` § Core Engineering |

**Hors périmètre explicite** :
- Plugins exécutables : runtime WASM, hooks de code, sandboxing, marketplace — chantiers ultérieurs. v0.1.29 ne livre **que** des plugins déclaratifs (zéro exécution de code).
- Elasticsearch, TimescaleDB, Federation Pro, Data Time-Travel, AI Notebooks — releases dédiées.
- Query Library Advanced [Pro], Data Generator [Pro] — non retenus pour cette release.
- Accessibilité WCAG, Dockable Panels — chantiers transverses.

**Les trois features ajoutées sont 100 % Core.** C'est délibéré : elles équilibrent le lot Pro de la branche en apportant de la valeur à tout le monde, gratuitement.

---

## 🎯 Phase 0 — Lot déjà présent sur la branche

`feat/pro-update` contient déjà ~43 fichiers (+2271 / −267), à **finaliser et vérifier**, pas à recréer :

- **Incitations Pro** : `ProDiscoveryPanel`, `UpgradePrompt`, `FounderBadge`, `licenseTracking.ts`, `pricing.ts`, `usageBanner.tsx`, `usageCounter.ts`, évolutions `LicenseGate` / `LicenseActivation` / `LicenseBadge` / `LicenseSection`, flag founder côté backend (`license/key.rs`, `mod.rs`, `status.rs`).
- **Acquisition & parcours** : refonte `OnboardingModal` (+486), `NewsletterPromptModal` + `newsletter.ts`, `AnalyticsService`.
- **Communication** : `WhatsNewModal`, `useWhatsNew.ts`, `changelog.ts`.
- **Core fonctionnel** : `sqlite.rs` (+127, extraction de valeurs typées dynamiques) ; refactor du color scheme vers les variables CSS `--q-*`.
- **Infra repo** : packaging Homebrew / WinGet, `FUNDING.yml`.

**À solder dans cette phase** :
1. **i18n** : la branche n'a mis à jour que `en.json` et `fr.json`. Les **7 autres locales** (de, es, pt-BR, ru, ja, ko, zh-CN) doivent recevoir les ~198 nouvelles clés. Bloquant release.
2. **Headers SPDX** : vérifier chaque nouveau fichier (`UpgradePrompt`, `ProDiscoveryPanel`, `FounderBadge`, `LicenseGate` restent **Core** `Apache-2.0` — ce sont des composants de gating, pas du code Premium).
3. **`changelog.ts`** : ajouter l'entrée v0.1.29 une fois les phases 1-3 terminées (le `WhatsNewModal` la consomme).
4. Vérifier qu'aucun fichier ne dépasse 500 lignes (`OnboardingModal` à +486 est à surveiller — splitter si nécessaire).

---

## 🎯 Phase 1 — Query Result Caching [Core]

**Objectif** : cache local des résultats de lecture récents, servis instantanément, invalidés sur mutation via l'intercepteur. Perf ressentie par tous, sans configuration obligatoire.

### Décisions

1. **Périmètre du cache** : uniquement les lectures **non-streamées** entièrement matérialisées — `execute_query` avec `stream != true`, `preview_table`, `query_table`. Le chemin streaming (gros résultats) n'est **pas** touché. Un hit de cache renvoie un `QueryResponse` matérialisé directement, sans toucher le driver ni le canal de stream.
2. **Éligibilité** : la requête doit être classée `Read` (`sql_safety::analyze_sql` → `is_mutation == false` ; `mongo_safety::classify` / `redis_safety::classify` → `Read`). Toute requête mutation/dangerous/unknown contourne le cache (ni lecture ni écriture).
3. **Clé de cache** : `sha256(connection_id ‖ driver_id ‖ namespace ‖ requête exacte normalisée whitespace)`. **Pas** le `fingerprint` de v0.1.28 : celui-ci remplace les littéraux par `?` et ferait collisionner `WHERE id = 1` et `WHERE id = 2`. Le cache exige la requête littérale exacte. Pour `preview_table` / `query_table`, la clé inclut table + pagination + tri + filtres.
4. **Scope par connexion** (`connection_id`, pas `session_id`) : une mutation sur une connexion invalide les lectures de cette connexion même depuis une autre session.
5. **Invalidation** : sur tout `post_execute` de l'intercepteur où `context.is_mutation && result.success`, purge de **toutes** les entrées de cette `connection_id`. Invalidation grossière mais correcte ; l'extraction des tables touchées est repoussée (v0.1.30).
6. **Bornes** : LRU borné par nombre d'entrées **et** taille mémoire totale (défaut : 100 entrées / 64 Mo). Les résultats au-delà d'un seuil (défaut : 5 000 lignes ou 8 Mo) ne sont pas mis en cache.
7. **TTL** : défaut 60 s, configurable. Garde-fou contre les mutations externes (hors QoreDB) que l'intercepteur ne voit pas.
8. **Limitation assumée** : les mutations faites en dehors de QoreDB (autre client, job) ne sont pas détectées. Le TTL borne la péremption ; un bouton « rafraîchir » force le contournement.

### Découpage fichiers

`src-tauri/src/cache/` (nouveau, Core, `Apache-2.0`)
- `mod.rs` (~120 lignes) — types (`CacheConfig`, `CacheEntry`, `CacheStats`), re-exports.
- `store.rs` (~260 lignes) — `QueryCache` : LRU borné (entrées + octets), TTL, `get`, `put`, `invalidate_connection`, `clear`, `stats`. `Mutex<…>` dans `AppState`.

`src-tauri/src/commands/cache.rs` (nouveau, Core, ~120 lignes)
- `get_cache_config`, `set_cache_config`, `clear_query_cache`, `get_cache_stats`.

**Intégrations chirurgicales** :
- `src-tauri/src/commands/query.rs` — dans `execute_query` (chemin non-stream), `preview_table`, `query_table` : lookup avant exécution, `put` après lecture réussie. Dans le `post_execute` mutation → `invalidate_connection`. Ajout des champs `cached: Option<bool>` et `cached_age_ms: Option<u64>` à `QueryResponse`.
- `src-tauri/src/lib.rs` — champ `query_cache: Arc<QueryCache>` dans `AppState`, enregistrement des 4 commandes.

`src/lib/cache.ts` (nouveau, Core) — bindings TS + types `CacheConfig` / `CacheStats`.

**Frontend** :
- `src/lib/tauri.ts` — `QueryResponse` enrichi (`cached?`, `cached_age_ms?`).
- Panneau de résultats (`src/components/Results/`) — badge discret « Mis en cache · il y a Ns » + bouton de rafraîchissement (re-run forçant le contournement). Composant à localiser précisément à l'implémentation.
- `src/components/Settings/sections/DataSection.tsx` — sous-carte « Cache de requêtes » : toggle activé/désactivé, TTL, taille max, bouton « Vider le cache » + stats (hit rate). Pas de nouvelle section Settings (chirurgical).

**i18n** : sous-arbre `cache.*` (~20 clés), 9 locales.

**Events PostHog** : `query_cache_cleared`. Hit rate suivi via `get_cache_stats` (pas d'event par hit — trop verbeux).

**Tests** : `cache::store` (LRU, éviction par octets, TTL, invalidation par connexion) ; test d'intégration : lecture → hit ; mutation → miss ; TTL expiré → miss.

---

## 🎯 Phase 2 — Security Hardening [Core]

Deux items : finalise le finding **5.6** de `SECURITY_AUDIT.md` reporté de v0.1.28, et ajoute le Rate Limiting tracé en `v3.md` § Sécurité.

### 2.1 FS Capability Scope Restriction

**État actuel** (`src-tauri/capabilities/default.json`) : `fs:allow-write-text-file` + `fs:allow-write-file` avec un **deny-list** de 22 chemins sensibles (`.ssh`, `.aws`, `.kube`, `/etc`, historiques shell, etc.).

**Cible v0.1.29** : ajouter un **allow-list positif**, le deny-list restant en défense en profondeur (le `deny` prime sur le `allow`).

**Audit préalable obligatoire** — distinguer les écritures :
- **Frontend via `tauri-plugin-fs`** → soumises à `fs:scope`. Ce sont elles à scoper.
- **Backend via `std::fs` Rust** (workspace manager, intercepteur, vault, snapshots, time-travel) → **non concernées** par `fs:scope`. À ne pas confondre.
- **Fichiers choisis par l'utilisateur** (exports, backups) via `tauri-plugin-dialog` → scope runtime accordé par le picker, pas besoin d'entrée statique.

**Allow-list proposé** (à confirmer après l'audit chemin par chemin) :
```json
{
  "identifier": "fs:scope",
  "allow": [
    { "path": "$APPLOCALDATA/**" },
    { "path": "$APPCONFIG/**" },
    { "path": "$APPDATA/**" }
  ],
  "deny": [ /* … les 22 chemins sensibles existants, conservés … */ ]
}
```
- `$APPLOCALDATA` = `com.qoredb.app` (répertoire app réel — cf. `paths.rs`).
- **Cas des workspaces** : les répertoires `.qoredb/` vivent dans des dossiers projet arbitraires. Si une écriture `.qoredb/` passe par le frontend `fs`, accorder un scope runtime à l'ouverture du workspace (`tauri::scope` / `FsScope::allow_directory`). Si elle passe par le backend Rust, rien à faire. **L'audit tranche ce point.**

**Fichier** : `src-tauri/capabilities/default.json` (+ éventuel hook runtime à l'ouverture de workspace).

### 2.2 Query Rate Limiting [Core]

**Objectif** : protéger contre les boucles de requêtes accidentelles (script qui spamme des `SELECT`). Anti-loop, pas throttling : seuil large.

**Décisions** :
1. Le token bucket existant (`src-tauri/src/api/rate_limit.rs`) est **Pro-gated** (`#![cfg(feature = "pro")]`) — non réutilisable en Core. Création d'un module Core indépendant (~80 lignes dupliquées, acceptable vs. partage cross-licence).
2. **Par connexion** : `HashMap<ConnectionId, TokenBucket>` dans `AppState`.
3. **Seuil** : défaut 60 requêtes / 10 s par connexion (large — usage humain jamais atteint, boucle accidentelle stoppée). Au-delà → `Err` avec message explicite.
4. Persisté dans `SafetyPolicy` (déjà dans `config.json`) : nouveau champ + toggle dans Settings → Security pour les power users.

**Découpage fichiers** :
- `src-tauri/src/ratelimit.rs` (nouveau, Core, ~120 lignes) — token bucket + registre par connexion.
- `src-tauri/src/commands/query.rs` — check dans `execute_query` juste après validation de session (chirurgical).
- `src-tauri/src/lib.rs` — champ `connection_rate_limiters` dans `AppState`.
- `SafetyPolicy` (type + persistance) — nouveau champ `max_queries_per_window`.
- `src/components/Settings/sections/SecuritySection.tsx` — toggle « Protection anti-boucle de requêtes ».

**i18n** : clés `security.rateLimit.*` et `cache.*` regroupées (~10 clés sécurité), 9 locales.

**Events PostHog** : `query_rate_limited` (props : `driver`).

**Tests** : `ratelimit` (consommation, recharge, refus, oubli) ; test d'intégration : N requêtes rapides → la (N+1)ᵉ refusée.

---

## 🎯 Phase 3 — Plugin System Foundation (déclaratif) [Core]

**Objectif** : poser la fondation d'un système de plugins — manifeste, registry, lifecycle, panneau Settings — avec **un type de plugin réel et utilisable dès v0.1.29 : les plugins déclaratifs**. Aucune exécution de code → aucun sandbox nécessaire. Le runtime WASM/hooks viendra se brancher sur cette fondation plus tard.

### Ce qu'un plugin déclaratif peut contribuer

Trois types de contributions, toutes purement données :
1. **Snippet packs** — collections de snippets SQL réutilisables (étend `src/lib/query/sqlSnippets.ts`).
2. **Templates de connexion** — presets pré-remplis dans le dialogue de nouvelle connexion.
3. **Thèmes** — jeux de variables CSS `--q-*` (light + dark) injectés dans `:root`.

### Format du manifeste (`plugin.json`)

```json
{
  "id": "acme.postgres-pack",
  "name": "PostgreSQL Power Pack",
  "version": "1.0.0",
  "author": "ACME",
  "description": "Snippets et thème pour PostgreSQL.",
  "qoredb": ">=0.1.29",
  "contributes": {
    "snippets": [
      { "id": "explain-analyze", "label": "EXPLAIN ANALYZE",
        "description": "Plan d'exécution détaillé", "template": "EXPLAIN (ANALYZE, BUFFERS) ${query};" }
    ],
    "connectionTemplates": [
      { "id": "local-pg", "name": "PostgreSQL local", "driver": "postgres",
        "defaults": { "host": "localhost", "port": 5432 } }
    ],
    "themes": [
      { "id": "midnight", "name": "Midnight", "description": "Thème sombre froid",
        "light": { "--q-accent": "#3b5bdb" }, "dark": { "--q-accent": "#748ffc" } }
    ]
  }
}
```

Emplacement disque : `$APPLOCALDATA/plugins/<plugin-id>/plugin.json` (dans l'allow-list FS de la Phase 2).

### Découpage fichiers

`src-tauri/src/plugins/` (nouveau, Core, `Apache-2.0`)
- `mod.rs` (~120 lignes) — types (`PluginManifest`, `PluginContributions`, `InstalledPlugin`).
- `manifest.rs` (~200 lignes) — parsing + **validation** stricte : format de l'`id`, semver, compat `qoredb`, validation de chaque contribution (un thème ne peut écrire que des clés `--q-*` connues, un template ne référence qu'un `driver` connu). Erreurs détaillées.
- `registry.rs` (~240 lignes) — scan de `$APPLOCALDATA/plugins/`, chargement des manifestes, état enabled/disabled persisté dans `plugins/index.json`, `install` (copie de dossier), `remove`.

`src-tauri/src/commands/plugins.rs` (nouveau, Core, ~140 lignes)
- `list_plugins`, `install_plugin(source_path)`, `remove_plugin(id)`, `set_plugin_enabled(id, enabled)`, `get_plugin_contributions()`.

`src/lib/plugins/` (nouveau, Core)
- `types.ts` (~120 lignes) — miroir des types.
- `index.ts` (~150 lignes) — bindings Tauri + agrégation des contributions des plugins activés.

`src/providers/PluginProvider.tsx` (nouveau, Core, ~150 lignes)
- Charge au démarrage les contributions des plugins activés, les expose via contexte (`usePluginContributions`).

`src/components/Plugins/` (nouveau, Core)
- `PluginCard.tsx` (~120 lignes) — une ligne plugin (nom, version, auteur, toggle, badge contributions).
- `PluginDetailDialog.tsx` (~200 lignes) — détail du manifeste + liste des contributions.
- `InstallPluginDialog.tsx` (~150 lignes) — sélection d'un dossier plugin via picker, validation, installation.

`src/components/Settings/sections/PluginsSection.tsx` (nouveau, Core, ~220 lignes)
- Liste des plugins installés, état vide pédagogique, bouton « Installer un plugin ».

**Intégrations** :
- `settingsConfig.ts` — nouvelle section `'plugins'` (id, `labelKey`, icône `Puzzle`, keywords).
- `SettingsPage.tsx` + `sections/index.ts` — câblage de la nouvelle section.
- Snippet packs → consommés par le picker de snippets (merge `SQL_SNIPPETS` + contributions).
- Templates de connexion → presets dans le dialogue de nouvelle connexion.
- Thèmes → enregistrés auprès du système de thème ; injection des variables CSS `--q-*` dans `:root` à la sélection.

**Gating** : la fondation plugins est **Core** — ne **pas** l'ajouter à l'enum `ProFeature`. (Si un plugin contribue plus tard une feature Pro, c'est cette contribution-là qui sera gatée.)

**i18n** : sous-arbre `plugins.*` (~50 clés), 9 locales.

**Events PostHog** : `plugin_installed` (props : `contributions` — types contribués), `plugin_enabled`, `plugin_removed`.

**Tests** : `manifest` (parsing, rejets : id invalide, semver invalide, thème avec clé CSS inconnue, driver inconnu) ; `registry` (scan, install, enable/disable, remove).

---

## 🧱 Décisions techniques transverses

1. **Mono-PR, mono-branche** `feat/pro-update`. Commits scoped par phase.
2. **SPDX** : Phases 1, 2, 3 → `Apache-2.0` partout (tout est Core).
3. **Aucun fichier > 500 lignes** — splittage prévu d'avance. Surveiller `OnboardingModal.tsx` (déjà +486 sur la branche).
4. **i18n exhaustive sur 9 locales** — inclut le rattrapage des 7 locales manquantes du lot Phase 0. Zéro string en dur.
5. **Composants `src/components/ui/`** réutilisés (Dialog, Card, Input, Toggle, Button, Tooltip).
6. **Tests Vitest** pour la logique pure frontend ; **tests cargo** pour `cache::store`, `ratelimit`, `plugins::manifest`, `plugins::registry`.
7. **Cargo features** : tout est Core — `cargo build` sans `--features pro` doit compiler et passer les tests. `cargo build --features pro` aussi.
8. **PostHog events** documentés dans `doc/release/EVENTS.md` au fil des phases.

---

## 🔗 Dépendances et ordre

```
Phase 0 (finalisation branche + 7 locales) ─── indépendante
Phase 2 (FS scope) ──► Phase 3 (plugins lisent $APPLOCALDATA/plugins, doit être dans l'allow-list)
Phase 1 (cache) ────── indépendante
```

**Ordre recommandé** :
1. **Phase 2** (sécurité) — l'allow-list FS doit inclure `$APPLOCALDATA/plugins/` avant la Phase 3.
2. **Phase 3** (plugins) — exploite l'allow-list.
3. **Phase 1** (cache) — indépendante, en parallèle.
4. **Phase 0** — finalisation continue ; les 7 locales et `changelog.ts` se bouclent en dernier.

---

## ✅ Checklist de release v0.1.29

### Code
- [ ] Header SPDX correct sur tous les nouveaux fichiers (`Apache-2.0`).
- [ ] Aucun fichier > 500 lignes.
- [ ] `pnpm lint:fix && pnpm format:write` clean.
- [ ] `pnpm test` (Rust) sans régression.
- [ ] `cargo build` (Core) et `cargo build --features pro` compilent.
- [ ] Tests Vitest verts.
- [ ] i18n 9 locales exhaustive — **inclut le rattrapage des 7 locales du lot Pro de la branche**.
- [ ] Composants `src/components/ui/` réutilisés (audit visuel).
- [ ] Validation cross-OS : chemins FS scope (`$APPLOCALDATA` Win/macOS/Linux), répertoire `plugins/`.

### Documentation
- [ ] `doc/todo/v3.md` — cocher Query Result Caching, Rate Limiting, File System Scope Restriction, Plugin System Foundation (partiel — déclaratif uniquement).
- [ ] `doc/FEATURES.csv` — lignes `query_result_caching`, `query_rate_limiting`, `plugin_system`.
- [ ] `doc/rules/FEATURES.md` — section par feature.
- [ ] `doc/audits/SECURITY_AUDIT.md` — finding 5.6 marqué résolu avec date.
- [ ] `doc/internals/UNIVERSAL_QUERY_INTERCEPTOR.md` — section invalidation de cache.
- [ ] `doc/release/EVENTS.md` — nouveaux events PostHog.
- [ ] `README.md` — bullets pour les 3 features Core.
- [ ] `src/data/changelog.ts` — entrée v0.1.29 (consommée par `WhatsNewModal`).
- [ ] `doc/release/RELEASE_NOTES_v0.1.29.md` — release notes finales.

### Release
- [ ] Bump `package.json`, `Cargo.toml` (workspace + crates) → `0.1.29` (`tauri.conf.json` lit `package.json`).
- [ ] `aur/PKGBUILD` + templates Homebrew/WinGet mis à jour.
- [ ] Release notes : 3 features Core mises en avant, lot Pro/onboarding mentionné, limitations connues.

---

## 🚧 Limitations connues à documenter

- **Query Result Caching** : ne détecte pas les mutations faites hors QoreDB (autre client, job) — le TTL borne la péremption. Invalidation par connexion entière (pas par table) en v1. Lectures streamées non cachées.
- **Rate Limiting** : protection anti-boucle (seuil large), pas un throttling fin ; par connexion, désactivable dans Settings.
- **Plugin System** : **déclaratif uniquement** en v0.1.29 — packs de snippets, templates de connexion, thèmes. Pas d'exécution de code, pas de hooks, pas de WASM, pas de marketplace. Installation manuelle (dossier local), pas de mise à jour automatique.
- **FS Scope** : passage à l'allow-list ; les fichiers hors périmètre restent accessibles via le file picker (médiation runtime).
