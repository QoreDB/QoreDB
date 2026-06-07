# QoreDB — Roadmap Plateforme & Enterprise

**Date** : mai 2026
**Portée** : repo QoreDB (app + surfaces + serveur) + site vitrine (autre repo)
**Statut** : document de structuration interne — prémice de l'écosystème Qore

---

## 0. Vision directrice

QoreDB cesse d'être « une app desktop » pour devenir **QorePlatform** : un cœur de données réutilisable (`qore-*`) exposé à travers plusieurs surfaces, et capable de fonctionner dans un cadre entreprise (sécurité, identité, gouvernance, conformité).

Cette transition est la **prémice de l'écosystème Qore** (QoreORM et autres briques à venir). Tout ce qui est construit ici doit être pensé comme une fondation partagée, pas comme une feature ponctuelle.

### Le constat qui structure tout

Deux demandes — « rendre QoreDB enterprise » et « étendre la surface (web, terminal, MCP) » — **convergent vers un seul artefact**.

- **Le web ne peut pas être local-first.** Un navigateur ne peut pas ouvrir une socket Postgres brute. Il faut un serveur : `navigateur → qore-server → DB`.
- **L'enterprise réel ne peut pas être local-first non plus.** SSO, RBAC centralisé, audit inviolable, politiques d'accès, gestion des sièges : tout exige un point central.

→ **Le serveur web et le control plane enterprise sont le même composant** : `qore-server`, daemon **auto-hébergeable (on-prem)**. Les surfaces locales (CLI, MCP) restent des binaires fins posés directement sur le core, sans serveur.

### Décisions actées

- **Déploiement** : self-hosted on-prem. Les données ne quittent jamais le réseau du client. Cohérent avec l'ADN privacy-first.
- **Première surface** : MCP server (la moins chère, valide l'extraction du core, levier IA maximal). Mais **tout le programme sera livré, rien n'est mis de côté.**
- **Piliers enterprise prioritaires** : SSO/SAML/SCIM · RBAC + audit + conformité · Protection des données (masking/PII).

---

## 0 bis. État des lieux (audit du code existant)

Avant de proposer du neuf : une grande partie de la fondation existe déjà. Ce tableau distingue ce qui est **acquis**, **partiel** (à étendre), et **greenfield** (à construire).

| Brique proposée | État | Réalité du code |
| --- | --- | --- |
| Moteur découplé (`qore-core`, `qore-drivers`, `qore-query`) | ✅ **Acquis** | `DataEngine` (`crates/qore-core/src/traits.rs`), zéro Tauri. Workspace propre. |
| Couche de service `qore-service` | ✅ **Acquis** | Extraction faite : `crates/qore-service` porte connexion, query (`preflight` + `execute` streaming inclus), mutation preflight, cache, policy, ratelimit, metrics, interceptor, license, vault, governance. `commands/query.rs` réduit à ≈ **1658 lignes** (bridge vers les free functions). |
| Serveur HTTP | ✅ **Acquis (à étendre)** | **« Instant API »** (`src/api/`, axum + tower) : serveur local read-only, TLS auto-signé, auth bearer token, rate limiting, OpenAPI. `qore-server` se **greffe dessus**. |
| Policy engine | ⚠️ **Partiel** | `crates/qore-service/src/policy.rs` : `SafetyPolicy` (confirmation prod, blocage SQL dangereux, limites durée/lignes/concurrence). Pas de masking colonne, pas d'ABAC. |
| Rate limiting | ✅ **Acquis** | `crates/qore-service/src/ratelimit.rs` (token bucket par session) + `src/api/rate_limit.rs`. In-memory. |
| Observability / metrics | ⚠️ **Partiel** | `src/observability.rs` (tracing + logs roulants, resté dans l'app), `crates/qore-service/src/metrics.rs` (compteurs in-memory). **Pas de Prometheus.** |
| License tiers | ✅ **Acquis (à exploiter)** | `LicenseTier { Core, Pro, Team, Enterprise }` + vérif Ed25519 (`crates/qore-service/src/license/`). **Le tier `Enterprise` existe déjà** — il manque les features gated dessus et la notion de **sièges / licence serveur**. |
| Audit log | ⚠️ **Partiel** | `crates/qore-service/src/interceptor/audit.rs` : JSONL append-only + rotation + ring buffer. **Pas de chaînage par hash** → la tamper-evidence est le travail neuf. |
| Redaction / masking | ⚠️ **Partiel** | `crates/qore-service/src/interceptor/redaction.rs` (redaction des requêtes dans l'audit) + redaction PII du schéma avant envoi LLM. **Pas de masking des résultats** au niveau colonne. |
| Federation (cross-source) | ✅ **Acquis** | `src/federation/` : JOIN multi-connexions via DuckDB éphémère. Premium, complet. |
| RBAC / rôles / permissions | ❌ **Greenfield** | Aucun concept de rôle. Accès par `SessionId` (UUID), pas par utilisateur. |
| SSO / SAML / OIDC / SCIM | ❌ **Greenfield** | Zéro. Seul `src/api/auth.rs` fait du bearer token pour l'Instant API. |
| Multi-utilisateur / comptes | ❌ **Greenfield** | Un seul workspace local, pas de compte ni de tenant. |
| MCP server | ✅ **Acquis** | `crates/qore-mcp` (rmcp 1.7, stdio, 5 outils read-only) sur le même vault. Lecture en Core. |
| CLI / TUI | ⚠️ **Partiel** | CLI faite (`crates/qore-cli`, binaire `qore`, clap, sortie JSON). **TUI** (ratatui) pas encore. |
| Abstraction transport frontend | ✅ **Acquis** | `src/lib/transport.ts` (`isWeb`, `invoke` Tauri/HTTP, `webExecuteQuery` SSE, shim `listen`). **27 fichiers** basculés vers le transport (le « 148 » comptait les *appels*, pas les imports). Boot navigateur vérifié headless. |

> ⚠️ Cette table était la photo de l'existant **avant** les Jalons 0-4. Elle a été **rafraîchie après le Jalon 4** : les lignes marquées ✅ (qore-service, MCP, CLI, transport) sont désormais faites ; les chemins pointent vers `crates/qore-service/` après extraction.

**Lecture (post-Jalon 4)** : la fondation est désormais bien plus avancée — `qore-service` extrait, surfaces **MCP** et **CLI** livrées, **transport frontend** + `qore-server` v0 (bridge HTTP/SSE) en place, **boot navigateur vérifié**. Les chantiers réellement neufs restants sont l'**identité/RBAC/multi-utilisateur** et la **gouvernance** (audit hash-chain, masking colonne) — les **piliers enterprise (Jalons 5-7)**, plus le durcissement complet du boot web. Les jalons ci-dessous référencent l'existant à étendre plutôt que de tout reconstruire.

---

## 1. Architecture cible

```
   qore-core · qore-sql · qore-drivers · qore-query   (workspace déjà extrait ✅)
                          │
                 ┌────────┴────────┐
                 │  qore-service   │   ← À EXTRAIRE : couche de service sans Tauri
                 │ (contrat unique)│      (la logique encore inline dans commands/)
                 └────────┬────────┘
   ┌──────────┬──────────┼───────────────┬──────────────────┐
   ▼          ▼          ▼               ▼                  ▼
 Desktop    CLI/TUI    MCP server    qore-server ────────  Web frontend
 (Tauri)    (local)    (local)       (control plane)        (React via HTTP)
                                     enterprise + web
```

### La clé de voûte : `qore-service`

Le **moteur** est déjà extrait (`qore-core`/`DataEngine`, `qore-drivers`, `qore-query`, zéro Tauri). Mais la **logique métier**, elle, vit encore inline dans `src-tauri/src/commands/` (~40 fichiers, `query.rs` ≈ 2320 lignes), mêlée à la glue Tauri (`State<>`, `AppHandle`, `emit`). L'extraction est donc à **moitié faite** : le socle moteur est bon, la couche de service reste à dégager.

**Le geste fondateur** : extraire cette logique de `commands/` vers une crate **`qore-service`** (sans aucune dépendance Tauri), qui expose un **contrat unique** d'opérations :

- `connect / disconnect / list_sessions`
- `execute_query / stream_query / cancel`
- `mutate / sandbox_*`
- `describe_schema / list_tables / full_text_search`
- `export / backup`
- `vault_* / credentials`

Chaque surface devient alors un **adaptateur fin** au-dessus de `qore-service` :

| Surface | Rôle de l'adaptateur |
| --- | --- |
| Tauri commands | extraire `State`, appeler `qore-service`, émettre les events (inchangé côté frontend) |
| CLI / TUI | parser les args, appeler `qore-service`, formater la sortie terminal |
| MCP server | mapper les tools MCP → `qore-service`, appliquer le scope/sécurité |
| qore-server | router HTTP/gRPC → `qore-service`, + couches auth/RBAC/audit/policy |

Les events temps réel (streaming, health, progress) sont abstraits derrière un trait `EventSink` : Tauri l'implémente avec `emit`, le serveur avec du SSE/WebSocket, le CLI avec un writer stdout.

**Sans ce jalon, chaque surface dupliquerait la logique.** C'est le prérequis absolu de tout le reste.

---

## 2. Les quatre workstreams

### Workstream A — Fondation core (`qore-service`)

Extraction de la couche de service, définition du contrat, abstraction des events. Le desktop continue de tourner à l'identique : on déplace la logique, on ne la réécrit pas.

### Workstream B — Surfaces locales (MCP, CLI/TUI)

Binaires fins, pas de serveur. Réutilisent le safety engine existant (blocage DROP/TRUNCATE en prod). Premier levier d'écosystème et de validation de l'extraction.

### Workstream C — Le control plane (`qore-server` + web)

Le gros morceau — mais **pas du greenfield** : il se greffe sur le serveur HTTP existant (« Instant API », `src/api/`, axum + TLS + auth bearer + rate limiting). On passe d'un serveur read-only mono-utilisateur à un control plane multi-utilisateur. Serveur auto-hébergeable qui :

- héberge `qore-service` et **broke les connexions DB côté serveur** (les credentials et sockets DB ne touchent jamais le navigateur) ;
- sert le frontend React en mode web ;
- porte l'identité (SSO/SCIM), l'autorisation (RBAC), la gouvernance (audit, masking, policy) ;
- stocke ses métadonnées (utilisateurs, rôles, audit, connexions partagées) dans **SQLite par défaut** (mono-binaire), **Postgres** en prod/HA ;
- se déploie en **un binaire + docker-compose**, activable par **licence offline** (Ed25519, même mécanisme que les clés Pro).

### Workstream D — Frontend transport-agnostique

Refactor de `src/lib/tauri.ts` (2040 lignes, monolithique) en :

1. **Découpage par domaine** : `lib/transport/connection.ts`, `query.ts`, `mutation.ts`, `schema.ts`, `export.ts`, `vault.ts`… (aligné sur la règle « pas de fichier > 500 lignes »).
2. **Interface `Transport`** : `invoke(cmd, args)` + `subscribe(event)`. Deux implémentations :
   - `TauriTransport` → `invoke()` + `Channel` (desktop, comportement actuel) ;
   - `HttpTransport` → `fetch` + SSE/WebSocket (web, vers qore-server).
3. Le reste du frontend ne connaît que l'interface. **Le même code React tourne en desktop et en web.**

Ce refactor n'est pas cosmétique : c'est ce qui permet au web d'exister sans dupliquer le frontend.

---

## 3. Les piliers enterprise (détaillés)

Tous vivent dans `qore-server`. Tier **Enterprise** (licence `BUSL-1.1`).

### 3.1 — Identité : SSO / SAML / SCIM

- **OIDC + SAML 2.0** : authentification via l'IdP du client (Okta, Entra ID, Google Workspace, Keycloak). C'est souvent le **premier critère bloquant** d'un achat enterprise.
- **SCIM 2.0** : provisioning/déprovisioning automatique des comptes depuis l'IdP. Quand un employé part, son accès QoreDB disparaît sans intervention manuelle.
- Sessions serveur : expiration configurable, MFA délégué à l'IdP, révocation centralisée.
- **À ne pas sous-estimer** : la variété des IdP. Prévoir une abstraction `IdentityProvider` et tester contre Keycloak (gratuit, self-hostable) dès le départ.

### 3.2 — Autorisation, audit & conformité : RBAC + journal inviolable

- **RBAC** : rôles (`admin`, `analyst`, `read-only`…) et permissions fines : par connexion, par base, jusqu'au niveau requête (qui peut écrire, qui ne peut que lire, qui voit quelles connexions).
- **Audit append-only inviolable** : l'audit existe déjà (`interceptor/audit.rs` : JSONL append-only + rotation + ring buffer), mais **sans tamper-evidence**. Le travail neuf = ajouter le **chaînage par hash** (chaque entrée référence le hash de la précédente → altération détectable) + l'export SOC 2 / ISO 27001, et le centraliser côté serveur.
- **Rétention** : politique de conservation configurable, archivage.

### 3.3 — Protection des données : masking / PII

- **Masquage de colonnes** : règles déclaratives (`users.email`, `users.ssn` → redacted/hashed/partiel) appliquées **dans le chemin de requête côté serveur**, avant que les données n'atteignent le client. La brique de redaction existe (`interceptor/redaction.rs`, pour les logs d'audit et le contexte IA) mais **ne masque pas encore les résultats** au niveau colonne — c'est le travail neuf.
- **Détection PII** : suggestion automatique des colonnes sensibles à masquer.
- **Read-only prod renforcé** : garde-fous serveur non contournables (au-delà du flag local actuel).
- **Policy engine** : moteur de règles centralisé (masking + read-only + restrictions) appliqué uniformément à toutes les surfaces qui passent par le serveur.

---

## 4. Ce que tu ne vois peut-être pas venir (checklist enterprise readiness)

Tu as dit qu'il y a « pas mal de choses dont tu n'as probablement pas conscience ». Voici la liste des exigences enterprise *non évidentes* qui font capoter des deals si elles manquent. Toutes ne sont pas pour tout de suite, mais le **plan doit en tenir compte** pour ne pas se peindre dans un coin.

**Déploiement & exploitation**
- Story d'installation propre : binaire unique + `docker-compose` + Helm chart (k8s) plus tard.
- Support **air-gapped** (réseau isolé, sans Internet) → licence offline obligatoire, pas de phone-home.
- Sauvegarde/restauration des métadonnées serveur (la base Postgres du control plane).
- Observabilité : métriques Prometheus, logs structurés, healthchecks.
- Haute disponibilité / montée en charge (au moins ne pas l'empêcher architecturalement).

**Sécurité & secrets**
- Intégration **gestionnaires de secrets externes** (HashiCorp Vault, AWS Secrets Manager) en plus du vault interne — les entreprises ne veulent pas que tu stockes leurs credentials DB.
- Gestion des clés de chiffrement / KMS pour les données au repos côté serveur.
- IP allowlisting, rate limiting, politiques de session.
- Signature des binaires (notarization macOS, signing Windows) et **SBOM** (Software Bill of Materials).

**Conformité & juridique (cycle de vente)**
- Questionnaire de sécurité (les prospects en envoient systématiquement).
- DPA (Data Processing Agreement), modèle de menaces public, page sécurité.
- Trajectoire **SOC 2 Type II** (même si la certif vient plus tard, l'audit trail doit être prêt).
- Politique de divulgation des vulnérabilités, SLA de support.

**Gestion de flotte (pilier non priorisé mais à garder en vue)**
- Déploiement/config centralisée des postes desktop (MDM, group policy).
- Approbations 4-yeux pour requêtes destructrices, accès just-in-time.

> Ces points ne sont pas un quatrième pilier à construire maintenant — c'est une **grille de lecture** pour que les jalons ci-dessous n'oublient pas les prérequis structurels (licence offline, abstraction secrets, audit exportable).

---

## 5. Séquencement par jalons

Chaque jalon a un livrable et un **critère de vérification** clair. Estimations en jours de dev effectif (prévois un buffer en solo). Pas d'ordre figé semaine par semaine : on avance jalon par jalon, en alternant avec le produit.

### Jalon 0 — `qore-service` : la clé de voûte
**Livrable** : crate `qore-service` sans Tauri ; `commands/` réduits à des wrappers fins ; trait `EventSink`.
**Vérif** : l'app desktop fonctionne à l'identique (zéro régression) **et** un binaire de test peut faire `connect → query` sans aucune dépendance Tauri.
**Estim.** : 5-8 j. *(C'est le jalon le plus risqué — il peut révéler des couplages cachés. À faire en premier, seul.)*
**Plan d'implémentation détaillé** : voir `doc/private/JALON_0_QORE_SERVICE.md`.

### Jalon 1 — MCP server *(première surface)* — MVP fonctionnel ✅
**Livrable** : binaire `qore-mcp` exposant des tools (`list_connections`, `run_query`, `describe_schema`, `search`…), lecture seule par défaut, safety engine réutilisé.
**Vérif** : un agent IA (Claude…) se connecte via MCP, interroge une base, et toute opération destructrice est bloquée.
**Estim.** : 3-5 j.

**Fait (MVP)** : crate `src-tauri/crates/qore-mcp` (Core/Apache-2.0, **tauri-free**, zéro warning). Transport **stdio**, SDK **rmcp 1.7**. Réutilise `qore-service` tel quel : `ServiceContext::new()` + `VaultStorage` (project `default`, dir `~/.config/com.rapha.qoredb`, override `QOREDB_CONFIG_DIR`) sur le **même keyring OS** que le desktop — pas de second système de credentials (le `VaultLock` est une barrière UI desktop, `get_credentials` lit le keyring directement, donc rien à déverrouiller côté serveur). **5 tools read-only** : `list_connections`, `list_namespaces`, `list_tables`, `describe_table`, `run_query` (force `config.read_only = true` → mutations bloquées par les gates existants `preflight`/`execute`). Cache de sessions par `connection_id`. Handshake MCP validé (`initialize` + `tools/list` renvoient les 5 schémas corrects). README fourni (`crates/qore-mcp/README.md`) avec la commande d'enregistrement client.
**Reste** : tester `run_query`/`list_*` contre une connexion sauvegardée réelle ; tests d'intégration ; éventuellement un tool `search`.

### Jalon 2 — CLI / TUI — CLI MVP fonctionnel ✅ (TUI différé)
**Livrable** : `qore` CLI scriptable (CI/CD, headless) + TUI (ratatui) pour usage interactif SSH.
**Vérif** : exécuter une requête + export depuis le terminal contre les drivers principaux.
**Estim.** : 5-8 j.

**Fait (CLI MVP)** : crate `src-tauri/crates/qore-cli` (Core/Apache-2.0, tauri-free), binaire `qore`, parser **clap**, sortie **JSON** stdout (erreurs sur stderr + exit code). Réutilise le socle de `qore-mcp` (`ServiceContext` + `VaultStorage` même keyring/config que le desktop + `preflight`/`execute`). Commandes : `connections`, `query`, `tables`, `describe`. Respecte le `read_only` de la connexion (comme le desktop, pas de forçage) ; gates de sécurité existants appliqués. **Validé en réel** : `qore connections` liste les vraies connexions sauvegardées du vault ; `qore query` se connecte/échoue proprement (DB locale down → timeout pool 15 s sanitizé). README fourni. Le glue vault/connect/exec est dupliqué avec `qore-mcp` (~40 lignes, 2 copies) — à factoriser dans `qore-service` si une 3ᵉ surface arrive.
**Reste** : `export` depuis le terminal ; TUI (ratatui) ; mutations avec confirmation interactive.

### Jalon 3 — Frontend transport-agnostique — split fait ✅ (interface Transport faite au Jalon 4)
**Livrable** : `tauri.ts` éclaté par domaine derrière l'interface `Transport` ; `TauriTransport` opérationnel ; `HttpTransport` en squelette.
**Vérif** : le desktop tourne inchangé via `TauriTransport` ; aucun fichier transport > 500 lignes.
**Estim.** : 4-6 j.

**Fait (split)** : `src/lib/tauri.ts` (2040 lignes) éclaté en **14 modules domaine** sous `src/lib/tauri/` (`connection`, `query`, `schema-objects`, `schema-browse`, `transactions`, `mutations`, `data-io`, `logs`, `sandbox`, `search`, `maintenance`, `snapshots`, `workspace`, `time-travel`) + `types.ts` (22 types core partagés). `tauri.ts` devient un **barrel** (`export *`) → les **119 fichiers consommateurs (`@/lib/tauri`) sont inchangés**. Chaque module < 500 lignes (max 432). Convention déjà en place (`backup.ts`, `interceptor.ts` cohabitaient déjà, laissés hors barrel). Vérif : `tsc --noEmit` clean, `biome check` clean, `pnpm build` (tsc + vite) OK.
**Transport — FAIT au Jalon 4** : `src/lib/transport.ts` introduit `isWeb` + `invoke` générique (Tauri en desktop / `POST /api/invoke` en web) + `webExecuteQuery` (SSE) + un shim `listen` (no-op web). **27 fichiers** basculés de `@tauri-apps/api/*` vers `@/lib/transport` (le « 148 » comptait les *appels*, pas les imports). Desktop inchangé (`TauriTransport` = appel direct). Boot navigateur vérifié headless.

### Jalon 4 — `qore-server` v0 (mono-utilisateur, self-hosted) — fondation faite ✅
**Décision** : crate standalone `src-tauri/crates/qore-server` (BUSL-1.1, bin `qore-server`) — **pas** l'extension de l'Instant API (loopback-only, gated pro, couplé Tauri). Réutilise les patterns (auth bearer, TLS, rate-limit) mais découplé.
**Fait (fondation, compilé + smoke-testé)** :
- **Backend** : axum, host:port configurable, `/health` public, middleware auth bearer (compare const-time), CORS permissive. **Modèle session-id** (comme le desktop) : `SessionManager` = registre, pas de cache par connexion. Bridge générique `POST /api/invoke` (miroir des commandes Tauri : `list_saved_connections`, `connect_saved_connection`, `disconnect`, `list_namespaces`, `list_collections`, `describe_table`, `query_table`, `execute_query` bufferisé) + **SSE** `POST /api/stream/execute_query`. Service du SPA (`QORE_SERVER_WEB_DIR`) avec injection du token (`window.__QORE_TOKEN__`).
- **Frontend** : `src/lib/transport.ts` (`isWeb` via `window.__QORE_WEB__`, `invoke` générique → Tauri en desktop / `fetch /api/invoke` en web, `webExecuteQuery` SSE). Les **26 imports `invoke`** basculés vers le transport ; `query.ts` branche le web.
- **Vérif** : `cargo check` + `tsc --noEmit` + `pnpm build` clean. Smoke curl : health=ok, 401/200 auth, `list_saved_connections` renvoie le vrai vault, commande inconnue → 400.
- **Boot navigateur VÉRIFIÉ** (Chrome headless via CDP, page servie par qore-server) : l'app **monte réellement** (0 exception, 0 warning), affiche les **vraies connexions chargées via le bridge** (`pulse`, `tcg nexus`, `supabase`, `Clickhouse`) + l'empty-state. Crash bloquant trouvé+corrigé : `CustomTitlebar` appelait `getCurrentWindow()` au niveau module → garde `isWeb`. Durcissements : shim `listen` no-op web (9 fichiers), `WorkspaceProvider` skip détection FS en web, `SessionProvider` skip updater en web.
- **Stockage** : keyring par défaut (comme mcp/cli) **+ provider fichier chiffré** (Point 5 ✅) pour headless/Docker — `EncryptedFileProvider` (XChaCha20Poly1305, clé dérivée Argon2id depuis `QORE_VAULT_KEY`), factory `vault::backend::default_provider()` choisie par env, 3 tests. **Packaging Docker** : `Dockerfile` multi-stage (SPA + binaire, runtime debian-slim, rustls → zéro OpenSSL système) + `docker-compose.server.yml` + `.dockerignore`, volume `/data`.
**Durcissement web (en cours)** : liens externes web-aware — `openExternal` dans `transport.ts` (Tauri `openUrl` en desktop / `window.open` en web), 6 fichiers basculés (upgrade/pricing/activation/discovery/share/newsletter). Restent en erreur call-time en web : surface `dialog`+`fs` (pickers/FS — features secondaires, nécessite une UX web : download navigateur / `<input file>`) ; `updater`/`process` (no-op web, call-time seulement, pas bloquant boot).
**Reste (durcissement)** : valider le parcours **connexion → requête → résultat** complet dans le navigateur (nécessite une DB live) ; TLS activable ; surface `dialog`/`fs` web. (Point 5 packaging Docker + provider chiffré ✅.)
**Estim. restante** : ~6-10 j (durcissement boot web + flows secondaires).

### Jalon 5 — Identité & accès (SSO/SAML/SCIM + RBAC)
**Livrable** : OIDC + SAML, SCIM, rôles/permissions fins.
**Vérif** : login SSO via Keycloak de test ; un rôle restreint l'accès à une connexion ; SCIM provisionne et déprovisionne un utilisateur.
**Estim.** : 10-15 j.

**Slice 1 — Identité + RBAC local (backend) — FAIT ✅** (tout dans `qore-server`, BUSL-1.1) :
- **Control plane SQLite** (`controlplane/store.rs`, sqlx) : tables `users / roles / user_roles / connection_grants`, schéma auto-créé sous `<config_dir>/control.db`, résolution des grants effectifs (write > read entre rôles), seed admin via `QORE_ADMIN_EMAIL`/`QORE_ADMIN_PASSWORD`.
- **Auth** (`controlplane/auth.rs`) : hash/verify Argon2 + JWT HS256 (signé avec le token serveur, TTL 12 h). `POST /api/auth/login` (public) → JWT.
- **Modèle** : `GrantLevel{read,write}`, `AuthContext{Admin|User{grants}}`. Middleware : token partagé → **Admin**, JWT valide → **User** (grants chargés du store), injecté en extension.
- **Provisioning admin** : `POST /api/admin/users|roles|assign|grants`, `GET /api/admin/users` (gardés admin → 403 sinon).
- **Enforcement RBAC sur le bridge** : `list_saved_connections` filtré aux connexions accordées ; `connect_saved_connection` refusé sans grant, **forcé read-only** si grant = `read` (réutilise le read-only moteur). Écriture bloquée ensuite par le moteur read-only.
- **Vérif** : 3 tests unitaires (store) + smoke HTTP end-to-end (login admin/bob, 401 sans token, 401 mauvais mot de passe, 403 bob sur route admin, liste filtrée, connect non-accordé refusé, seed au boot). ⚠️ `cargo test` ne régénère pas l'exécutable runtime — refaire `cargo build -p qore-server` avant tout smoke.

**Slice 2 — Auth web (backend + plomberie) — FAIT ✅** : **jamais de credentials via env** (seed admin supprimé). Bootstrap par **register** : `POST /api/auth/register` autorisé seulement à 0 utilisateur (crée le 1er admin), fermé ensuite (403). `GET /api/auth/status` → `{setupRequired}` pour router register vs login. Le serveur **n'injecte plus de token** dans le HTML (seulement `window.__QORE_WEB__`) ; `QORE_SERVER_TOKEN` reste l'accès machine/admin hors-bande. Plomberie front `transport.ts` : store JWT (sessionStorage) + `setAuthToken`/`isAuthenticated`/`webAuthStatus`/`webRegister`/`webLogin`. Vérifié : smoke register→status→403→login→bridge admin, tsc clean. **Les écrans (prompt « setup », register, login) sont faits côté produit par l'utilisateur** — le backend + helpers sont prêts.

**Reste Jalon 5** : **OIDC/SSO** (Keycloak) qui se branche sur le control store, puis **SCIM**, puis **SAML**.

### Jalon 6 — Gouvernance (audit + masking/PII + policy)
**Livrable** : **ajout du chaînage par hash** à l'audit existant + export ; **masking des résultats** au niveau colonne (au-delà de la redaction de logs déjà présente) ; read-only prod serveur ; extension du `policy.rs` existant.
**Vérif** : chaque requête journalisée de façon immuable ; une colonne masquée renvoie une valeur redactée ; export d'audit type SOC 2.
**Estim.** : 8-12 j.

### Jalon 7 — Durcissement & conformité
**Livrable** : licence offline serveur, abstraction secrets externes, signing + SBOM, mise à jour du threat model, observabilité.
**Vérif** : déploiement air-gapped fonctionnel ; revue de sécurité passée ; SBOM généré ; releases signées.
**Estim.** : continu, à étaler.

---

## 6. Licensing & packaging

| Composant | Tier | Licence |
| --- | --- | --- |
| `qore-service`, CLI, MCP (lecture) | Core | Apache-2.0 |
| MCP écriture / fonctions avancées | Pro | BUSL-1.1 |
| `qore-server` + tous les piliers enterprise | **Enterprise** | BUSL-1.1 |

- Le tier **Enterprise** existe déjà dans l'enum `LicenseTier` (vérif Ed25519 faite) mais **aucune feature n'y est encore rattachée**. Le travail = y gater les piliers serveur, et étendre le mécanisme de clé à une **licence serveur offline** (sièges, expiration, machine_id) — la notion de sièges n'existe pas aujourd'hui.
- MCP : **lecture en Core** (driver d'adoption / écosystème), écriture et multi-connexion en Pro.
- Headers SPDX à poser dès la création de chaque nouveau fichier (`qore-service` = Apache, `qore-server` = BUSL).

---

## 7. Impact site vitrine (autre repo)

Conformément à la structure multi-repo, ces points sont à traiter dans le repo du site, **pas ici** :

- Page **Enterprise** (pricing « contactez-nous » assumé pour ce tier, contrairement au Pro).
- Émission des **licences serveur** (extension du backend de génération de clés Stripe → licences sièges/serveur).
- Page sécurité, threat model public, DPA téléchargeable, SOC 2 trust center.
- Docs de déploiement self-hosted (docker-compose, configuration IdP).

---

## 8. Risques

- **R1 — Jalon 0 révèle des couplages plus profonds que l'audit statique.** Mitigation : le faire en premier et seul ; ajuster le reste si nécessaire.
- **R2 — Le control plane est un changement de nature** (de local-first à client/serveur multi-utilisateur). Sécurité, sessions, multi-tenant : surface d'attaque nouvelle. Mitigation : threat model dédié avant le Jalon 4 ; broker côté serveur pour que les credentials DB ne fuient jamais vers le navigateur.
- **R3 — Dispersion solo.** Quatre workstreams, c'est beaucoup. Mitigation : livrer chaque jalon de bout en bout avant le suivant ; MCP + CLI donnent des wins rapides et valident le core avant d'attaquer le serveur, plus lourd.
- **R4 — Promesse enterprise non tenue.** Un deal capote sur une case manquante (SSO, audit export, air-gap). Mitigation : la checklist §4 est la grille de lecture de chaque jalon.

---

## 9. Décisions actées

1. **MCP** : **Core** (ouvert, sous la marque Qore). Lecture en Core pour maximiser la diffusion écosystème ; écriture / multi-connexion / fonctions avancées en Pro.
2. **Frontend web** : **même codebase React partagée** (via le Jalon 3, abstraction transport). Une seule UI à maintenir, au prix de la rigueur transport.
3. **Métadonnées serveur** : **SQLite par défaut** (déploiement mono-binaire) + **Postgres** en prod/HA. Abstraction du store dès le départ.
4. **Nom de la plateforme** : **QorePlatform**.

---

*Document de structuration. À mettre à jour au fil des jalons.*
