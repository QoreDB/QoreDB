# QoreDB — Prochaine mise à jour (v0.1.31)

**Date** : juin 2026
**Base** : desktop `v0.1.30` (cadence `v0.1.x`)
**Sources** : `doc/todo/v3.md` (roadmap produit) + `doc/private/QORE_PLATFORM_ROADMAP.md` (plateforme)
**Méthode** : ce document s'appuie sur un **audit du code réel** (pas seulement les cases cochées des docs). Les écarts trouvés sont consignés en §1.

---

## 0. TL;DR — Scope arrêté

Décisions validées par l'utilisateur (juin 2026) :

- **Volume** : **P0 + P1** (release riche, ~2 semaines).
- **Feature Pro phare** : **Data Generator** (nouveauté autonome monétisable). → Federation pushdown reporté.
- **Périmètre** : release **desktop v0.1.31** centrée sur la **valeur utilisateur visible**, en capitalisant sur l'infra déjà en place (IA, query library). Volet **Pro** monétisable inclus.

**Contenu arrêté de v0.1.31** (détails §3) :

| # | Item | Tier | Effort |
| --- | --- | --- | --- |
| P0.1 | Réalignement `v3.md` ↔ code | Core | ~0.5 j |
| P0.2 | Queries paramétrées (Query Library) | Pro | ~2-3 j |
| P0.3 | Cellule `ai` dans les Notebooks | Pro | ~3-4 j |
| P1.1 | Natural Language Filters (DataGrid) | Pro | ~2-3 j |
| **Pro phare** | **Data Generator** | Pro | ~5-8 j |

**Total estimé** : ~13-18 j de dev effectif (prévoir buffer solo).

- **Préalable non négociable** : commencer par **P0.1** — réaligner `v3.md` avec la réalité du code (§1). Plusieurs features y sont marquées « à faire » alors qu'elles sont livrées — dette documentaire qui fausse toute planification.
- **Plateforme R2** (cadence propre, indépendant du desktop) : _à confirmer_ — tag `qore-server v0.1` après smoke test Docker (cf. §4).

---

## 1. Écarts docs ↔ code (à corriger dans `v3.md`)

L'audit a révélé que `v3.md` sous-estime l'avancement réel. **Action : mettre à jour les cases ci-dessous dans le même commit que la release.**

| Feature | `v3.md` | Réalité (preuve code) | Action doc |
| --- | --- | --- | --- |
| **Data Time-Travel** [Pro] | `[ ]` | ✅ **COMPLET** — timeline, diff temporel, rollback SQL, filtres user/session/temps, capture before/after + redaction (`src-tauri/src/time_travel/*`, `src/components/TimeTravel/*`) | Cocher tous les sous-points |
| **Connection Resilience (SSH)** | `[ ]` | ✅ **COMPLET** — tunnel OpenSSH persistant, Ed25519/ssh-agent, ProxyJump, host-key TOFU/Strict, keep-alive 30 s, reconnexion auto à 2 échecs (`qore-drivers/src/ssh_tunnel.rs`, `session_manager.rs`) | Cocher SSH + keep-alive |
| **Plugin System (WASM)** | « WASM reste à faire » | ✅ **Runtime WASM livré** — `wasmi`, budget fuel/memory, hooks `preExecute`/`postExecute`, capabilities + integrity sha256 (`src-tauri/src/plugins/runtime/*`) | Cocher WASM + sandboxing ; garder « hooks context-menu » et « renderers custom » à faire |
| **Environment-Aware Workspaces** | `[ ]` | ⚠️ **~75 %** — détection `.qoredb/`, `WorkspaceProvider`, switcher sidebar, sync query library OK ; **manque** diff Prod/Staging (`src-tauri/src/workspace/*`, `src/providers/WorkspaceProvider.tsx`) | Cocher les 4 premiers sous-points |
| **Query Library Advanced** [Pro] | `[ ]` | ⚠️ **~85 %** — dossiers, tags, favoris, recherche, import/export JSON OK ; **manque** queries paramétrées (`src/lib/query/queryLibrary.ts`) | Cocher 3/4 sous-points |
| **OIDC/SSO** (R3 plateforme) | « en cours » | ✅ **COMPLET** — PKCE, JWKS, JIT, validé Keycloak réel (`qore-server/src/controlplane/oidc.rs`) | Déjà à jour dans le roadmap |

**Note** : `MariaDB` et `ClickHouse` sont bien complets (13 drivers enregistrés au total, tous opérationnels).

---

## 2. État réel par domaine (synthèse de l'audit)

Légende : ✅ Complet · ⚠️ Partiel · ❌ Absent

### Killer features
| Feature | État | Détail |
| --- | --- | --- |
| Data Time-Travel [Pro] | ✅ | Complet (cf. §1) |
| Cross-DB Federation Pro | ⚠️ | Syntaxe qualifiée + exécution + MongoDB + UI ✅ ; **pushdown de prédicats jamais rempli** (`planner.rs:70` `// v1: no pushdown`) ; **pas d'auto-matérialisation/cache** |
| Query Replay Lab [Pro] | ❌ | Aucun code (l'intercepteur audit existe mais ne fait pas record/replay) |
| Instant Data API [Pro] | ✅ | Livré v0.1.28 |
| Data Contracts [Pro] | ✅ | Livré v0.1.28 |

### Drivers & résilience
| Item | État | Détail |
| --- | --- | --- |
| 13 drivers (PG, MySQL, MariaDB, Mongo, Redis, SQLite, DuckDB, SQL Server, ClickHouse, CockroachDB, Neon, TimescaleDB, Supabase) | ✅ | Tous enregistrés/opérationnels |
| TimescaleDB | ⚠️ | Wrapper pg_compat OK ; **hypertables/continuous aggregates pas first-class** (commentaire « future work ») |
| Elasticsearch | ❌ | Zéro code |
| SSH resilience / keep-alive / indicateur visuel | ✅ | Complet (cf. §1) |

### IA
| Item | État | Détail |
| --- | --- | --- |
| Assistant IA (6 providers, streaming, safety, redaction PII) | ✅ | Complet, Pro |
| Contextual AI + semantic memory | ⚠️ | Contexte généré par requête ; **pas de `.qore-context.json` persistant** ni apprentissage historique |
| Cellule `ai` dans Notebooks | ❌ | Types existants : sql, mongo, markdown, contract, chart |
| Natural Language Filters (DataGrid) | ❌ | Aucun code |
| Suggestions d'index | ❌ | Slow-query tracking existe mais ne recommande rien |

### UX / UI
| Item | État | Détail |
| --- | --- | --- |
| Explain Plan, Transaction UI, CSV Import, Blob Viewer, Column Pin, Bulk Edit, DDL UI, Tabs, Breadcrumb | ✅ | Livrés |
| Dockable/Resizable Panels | ⚠️ | Resize sidebar + hauteur éditeur (localStorage) ; **pas de dock/repositionnement** |
| Accessibilité WCAG 2.1 AA | ⚠️ | ~30 % — 101 attrs ARIA, skip-link, 1 live-region ; **manque** nav clavier exhaustive, live-regions résultats/erreurs, mode contraste élevé |

### Perf & stabilité
| Item | État | Détail |
| --- | --- | --- |
| Query Result Caching | ✅ | Livré v0.1.29 (invalidation sur mutation) |
| Row Count Optimization | ✅ | Livré |
| Connection Pooling Tuning (exposé UI + métriques) | ❌ | Pools configurés en interne mais **pas exposés** |
| Memory Profiling | ❌ | Dev-metrics seulement |
| Startup Time Optimization | ⚠️ | Peu de lazy-loading |
| DuckDB Async Wrapper | n/a | DuckDB est CPU-bound (in-memory) → `spawn_blocking` non requis. **Item à retirer de la roadmap** (faux besoin) |

### Core engineering
| Item | État | Détail |
| --- | --- | --- |
| Settings Refactoring, Backup/Restore, Raccourcis perso | ✅ | Livrés |
| Plugin System | ⚠️ | Manifest + registry + **runtime WASM** ✅ ; **manque** hooks context-menu, renderers custom, actions post-query programmatiques |
| Query Library Advanced [Pro] | ⚠️ | ~85 % (manque queries paramétrées) |
| Data Generator [Pro] | ❌ | Zéro code |

### Plateforme (`qore-server`, cadence semver propre)
| Release | État | Détail |
| --- | --- | --- |
| R1 (MCP + CLI lecture) | ✅ | Prête (reste tests d'intégration réels) |
| R2 (web + register/login + RBAC + TLS + Docker) | ✅ | Prête — **reste smoke Docker avant tag** |
| R3 (SSO) | ⚠️ | OIDC ✅ validé Keycloak ; **SAML absent** |
| R4 (SCIM) | ❌ | Greenfield |
| R5 (gouvernance) | ❌ | Audit hash-chain, masking colonne, seats offline, Prometheus, SBOM serveur : tous absents |

---

## 3. Périmètre proposé pour v0.1.31

Objectif : **valeur visible + finitions à fort ROI + un cran de monétisation**, sans ouvrir de chantier lourd. Items classés par priorité. Chaque item a un **critère de vérification** (cf. principe « exécution guidée par l'objectif » du `CLAUDE.md`).

### P0 — À faire (cœur de la release)

**P0.1 — Réalignement `v3.md`** _(Core, ~0.5 j)_
Corriger les cases listées en §1.
→ **Vérif** : `v3.md` reflète l'état audité ; aucune feature livrée encore marquée `[ ]`.

**P0.2 — Queries paramétrées dans la Query Library** [Pro] _(~2-3 j)_
Dernier 15 % d'une feature à 85 %. Support `{{variable}}` / `$1` avec inputs typés à l'exécution (réutiliser le système de variables des Notebooks, déjà livré).
→ **Vérif** : sauvegarder une query avec `{{customer_id}}`, la relancer depuis la library ouvre un prompt de variables et substitue avant exécution. Test sur PG + MySQL.

**P0.3 — Cellule `ai` dans les Notebooks** [Pro] _(~3-4 j)_
Réutilise l'infra IA existante (`provider.rs`, extraction de query, safety). Nouveau type de cellule `ai` : prompt NL → génère une cellule SQL adjacente. Bonus faible coût : action « Summarize results » sur une cellule.
→ **Vérif** : dans un notebook connecté, une cellule `ai` « top 10 clients par CA » produit une cellule SQL exécutable et correcte ; opération destructrice générée → bloquée par la safety existante.

### P1 — Retenu

**P1.1 — Natural Language Filters (DataGrid)** [Pro] _(~2-3 j)_
Réutilise l'infra de génération de query. Barre de filtre NL → clause `WHERE` avec **preview avant application**.
→ **Vérif** : « commandes de la semaine dernière > 100€ » génère le `WHERE` correct, affiché en preview, appliqué sur confirmation.

**P1.2 — Data Generator (feature Pro phare)** [Pro] _(~5-8 j)_
Génération de données de test/seed. Aucun code existant → vrai chantier, mais feature autonome à forte valeur monétisable. Périmètre :
- Respect du schéma : types, contraintes, FK (réutiliser l'introspection `describe_table` / DDL existante)
- Données réalistes : noms, emails, dates, adresses, UUIDs
- Volume configurable (nombre de lignes)
- Export SQL INSERT **ou** exécution directe (passer par le sandbox/mutation preflight existant)
→ **Vérif** : sur une table avec FK, générer N lignes cohérentes (FK valides, types respectés) ; preview du SQL avant application ; testé sur PG + MySQL + SQLite.

> **Reporté** (initialement candidat Pro) : **Federation pushdown de prédicats** — remplir `pushdown_predicates` (`planner.rs:70`). Bon gain perf mais arbitré au profit du Data Generator. Candidat naturel pour v0.1.32.

### P2 — Hors scope v0.1.31 (candidats futurs)

**P2.1 — TimescaleDB first-class** _(~3-4 j)_ — hypertables + continuous aggregates dans l'arbre de navigation (le wrapper pg_compat existe déjà).

**P2.2 — Accessibilité (lot 1)** _(~3-5 j)_ — live-regions sur résultats/erreurs + audit nav clavier. Pose les bases WCAG sans viser la conformité complète.

---

## 4. Plateforme — actions parallèles (cadence `qore-server`)

Indépendant de la release desktop. À traiter selon disponibilité :

- **Tag R2** : ne reste qu'un **smoke test du packaging Docker** (`docker-compose.server.yml`) avant de tagger `qore-server v0.1`. Bloquants navigateur + TLS déjà levés.
- **R3 / SAML** : prochain front plateforme. Prérequis : abstraction `IdentityProvider` avant d'implémenter SAML (OIDC sert de référence).
- **Hors scope v0.1.31** : SCIM (R4), gouvernance R5 (hash-chain, masking colonne, seats, Prometheus, SBOM serveur).

---

## 5. Hors scope explicite (reporté)

Pour éviter la dispersion solo (risque R3 du roadmap), **ne pas** ouvrir ces chantiers dans v0.1.31 :

- **Query Replay Lab** — gros, greenfield, dépend du diff visuel.
- **Elasticsearch** — driver REST complet, mapping nav spécifique.
- **Suggestions d'index** [Pro] — nécessite analyse plans/historique.
- **Contextual AI semantic memory** (`.qore-context.json`) — feature IA lourde.
- **Dockable panels (repositionnement)** — refonte layout (ajouter `react-resizable-panels`).
- **DuckDB async wrapper** — **faux besoin** (CPU-bound), à retirer de la roadmap.
- **Memory profiling / connection pool tuning exposé** — observabilité, faible valeur perçue.

---

## 6. Checklist de release v0.1.31

- [ ] P0.1 : `v3.md` réaligné sur l'audit (cases corrigées)
- [ ] Items P0.2, P0.3, P1.1, P1.2 (Data Generator) implémentés + critères de vérif validés
- [ ] Headers SPDX corrects sur tout fichier nouveau/déplacé (Pro → `BUSL-1.1`, sinon `Apache-2.0`)
- [ ] i18n : nouvelles clés ajoutées aux **9 locales**, français accentué
- [ ] Gating Pro vérifié sur P0.2 / P0.3 / P1.1 / Data Generator (tous Pro)
- [ ] `doc/FEATURES.csv` + README mis à jour pour les features livrées
- [ ] `cargo check` clean · `tsc --noEmit` clean · `biome check` clean · `pnpm build` OK
- [ ] Bump version `package.json` + `src-tauri/Cargo.toml` → `0.1.31`
- [ ] (Plateforme, si confirmé) smoke Docker R2 → tag `qore-server v0.1`

---

## 7. Ordre d'attaque suggéré

1. **P0.1** — réaligner `v3.md` (zéro risque, débloque la planification).
2. **P0.2** — queries paramétrées (quick win, réutilise les variables Notebook).
3. **P0.3** — cellule `ai` Notebook (réutilise l'infra IA).
4. **P1.1** — NL Filters (réutilise la génération de query de P0.3).
5. **P1.2 Data Generator** — le gros morceau, en dernier car autonome et le plus long.

> **Plateforme R2** : indépendant, à insérer quand tu veux (juste un smoke test Docker). À confirmer.
