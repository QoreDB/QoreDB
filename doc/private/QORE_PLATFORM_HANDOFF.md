# QorePlatform — Passation à l'agent suivant

**But de ce document** : permettre à un autre agent IA de reprendre le chantier QorePlatform et de le finir correctement. Lis-le en entier avant de coder.

---

## 1. Contexte en une phrase

QoreDB devient **QorePlatform** : un cœur Rust partagé (`qore-*`) exposé sur plusieurs surfaces (desktop, MCP, CLI, web) + un control plane auto-hébergeable (`qore-server`) qui apporte l'identité (SSO/SCIM), le RBAC, l'audit et le masking. Le plan complet vit dans **`doc/private/QORE_PLATFORM_ROADMAP.md`** — c'est ta source de vérité. Ce fichier-ci n'est qu'un point d'entrée.

## 2. Lis ça d'abord (dans l'ordre)

1. `doc/private/QORE_PLATFORM_ROADMAP.md` — plan maître : §0bis (audit de l'existant), §5 (jalons + todo ✅/reste), **§5bis (plan de releases R1–R5)**.
2. `CLAUDE.md` (racine) — principes de collaboration + règles open-core.
3. `src-tauri/crates/qore-server/README.md` — usage du serveur.
4. `doc/private/JALON_0_QORE_SERVICE.md` — détail de l'extraction du core (déjà fait, pour comprendre l'archi).

## 3. Contraintes permanentes (NE PAS violer)

- **JAMAIS de credentials via variables d'environnement.** Toujours un écran de login. (`QORE_SERVER_TOKEN` est un accès machine/break-glass hors-bande, jamais envoyé au navigateur — ce n'est pas une exception à la règle.)
- **Commentaires de code quasi nuls.** L'utilisateur supprime même les notes « pourquoi ». N'en mets que si le code est faux ou incompréhensible sans.
- **Header SPDX obligatoire** en tête de chaque `*.ts/*.tsx/*.rs` : `Apache-2.0` (Core : frontend, transport, qore-service/mcp/cli) ou `BUSL-1.1` (qore-server + tout l'enterprise).
- **Ne commit/push JAMAIS sans que l'utilisateur le demande.** Il commit lui-même.
- **Changements chirurgicaux, simplicité d'abord, zéro abstraction spéculative** (règle de trois avant de factoriser).
- **i18n systématique** : toute chaîne UI passe par i18next ; traductions dans **les 9 locales** (`src/locales/{de,en,es,fr,ja,ko,pt-BR,ru,zh-CN}.json`). L'init i18n est `src/i18n.ts` (PAS `src/lib/i18n.ts`).
- **Pas de fichier composant > 500 lignes.**
- **Pas de planning par semaines** (S1/S2…). La cadence est en jours.

## 4. État actuel (ce qui est FAIT ✅)

Branche de travail : `fix/web-app`. Tout le code ci-dessous est **committé** (dernier : `9af0f99`).

- **Jalon 0** — `qore-service` extrait, tauri-free (data-plane partagé).
- **Jalon 1** — `qore-mcp` (5 tools read-only, stdio, rmcp 1.7).
- **Jalon 2** — `qore-cli` (binaire `qore`, clap, sortie JSON).
- **Jalon 3** — `src/lib/tauri.ts` éclaté en 14 modules + `src/lib/transport.ts`.
- **Jalon 4** — `qore-server` v0 : bridge HTTP `POST /api/invoke` + SSE, sert le SPA, Docker (Dockerfile multi-stage + compose), provider vault chiffré (XChaCha20Poly1305/Argon2id) pour headless. Boot navigateur vérifié headless.
- **Jalon 5 Slice 1** — Identité + RBAC local : control plane SQLite (`<config_dir>/control.db`), Argon2 + JWT HS256, grants (role,connexion,read/write), enforcement sur le bridge (filtre les connexions, force read-only si grant=read).
- **Jalon 5 Slice 2** — Auth web backend : bootstrap par `POST /api/auth/register` (autorisé seulement à 0 user), `GET /api/auth/status` → `{setupRequired, ssoEnabled}`, plus aucun token injecté dans le HTML.
- **Jalon 5 Slice 3** — OIDC/SSO backend : Authorization Code + PKCE à la main (reqwest rustls + jsonwebtoken/JWKS), JIT provisioning, redirect `/?sso_token=`. **Validé contre Google réel ; le callback reste à valider sur un Keycloak réel.**
- **Jalon 5 Slice 4** — **Écrans web d'auth** (cette session) : `AuthGate` (web-only, enveloppe tout l'arbre de providers) + `AuthScreen` (setup/login/SSO), i18n 9 langues. tsc/biome/build = 0. **Non encore testé en navigateur live.**

## 5. Ce qu'il RESTE à faire (par priorité de release)

### 🟡 R2 — `qore-server` v0.1 (première release monétisable, presque prête)
1. **Valider le parcours bout-en-bout en navigateur** : `cargo build -p qore-server` → `pnpm build` → lancer le serveur avec `QORE_SERVER_WEB_DIR=./dist` → register 1er admin → login → connexion DB live → requête → résultat streamé. C'est LE bloquant restant de R2.
2. **Option TLS** activable côté serveur.
3. Surfaces web secondaires encore en erreur call-time : `dialog`/`fs` (pickers → download navigateur / `<input file>`). Secondaire, ne bloque pas le boot.

### ⬜ R3 — SSO complet (server v0.2)
4. **Valider le callback OIDC sur un Keycloak de test réel** (échange de code + JWKS + JIT). Le code existe, seul le chemin callback n'a jamais tourné contre un vrai IdP avec client secret.
5. **SAML 2.0** : prévoir une abstraction `IdentityProvider` (OIDC et SAML derrière le même trait) avant de coder SAML.

### ⬜ R4 — SCIM (server v0.3)
6. **SCIM 2.0** provisioning/déprovisioning. Décisions à remonter à l'utilisateur AVANT de coder : auth de l'endpoint SCIM (bearer token dédié ?), mapping groupes IdP → rôles QoreDB.

### ⬜ R5 — Gouvernance & GA (server v1.0 = Jalons 6-7)
7. Audit **chaînage par hash** (tamper-evidence) + export SOC 2 — l'audit JSONL existe déjà (`crates/qore-service/src/interceptor/audit.rs`), il manque le chaînage.
8. **Masking colonne** dans le chemin de requête serveur (la redaction de logs existe, pas le masking des résultats).
9. Licence serveur offline (sièges, expiration, machine_id — l'enum `LicenseTier::Enterprise` existe déjà, vérif Ed25519 faite, mais aucune feature gatée ni notion de sièges).
10. Secrets externes (HashiCorp Vault / AWS SM), SBOM, signing binaires, Prometheus.

## 6. Pièges connus (GOTCHAS — fais gagner du temps)

- **Binaire serveur périmé** : `cargo test -p qore-server` ne régénère PAS `target/debug/qore-server`. **Toujours `cargo build -p qore-server` avant un smoke test**, sinon tu testes un vieux binaire.
- **biome `noAutofocus` n'est PAS activé** ici → un `// biome-ignore lint/a11y/noAutofocus` est signalé comme suppression inutilisée. Ne l'ajoute pas.
- **i18n** : init dans `src/i18n.ts` (pas `lib/`). Ajoute toute nouvelle clé aux **9** fichiers locale.
- **AuthGate doit envelopper TOUS les providers** dans `App.tsx` : en web, les providers appellent `invoke` au montage → 401 si pas de JWT. Ne déplace pas le gate sous un provider.
- **`reqwest` doit rester `default-features=false, features=[json,rustls-tls]`** : pas de native-tls (casse la propriété Docker zéro-OpenSSL). Idem, ne ré-introduis pas la crate `openidconnect` (elle traîne native-tls + churn d'API).
- **`jsonwebtoken`** : pas de feature Cargo `jwk` (le module est toujours dispo) — `jsonwebtoken = "9"` tout court.
- **Mutations & time-travel** : les corps d'exécution des mutations restent dans `commands/` (couplage Premium time-travel) ; seul le `preflight` est partagé dans qore-service. Ne tente pas de tout extraire.

## 7. Méthode attendue

Avant d'implémenter un jalon : **énonce tes hypothèses, expose les compromis, demande si ambigu** (cf. CLAUDE.md §1). Transforme chaque tâche en critère vérifiable (« écrire un test qui reproduit, puis le faire passer »). Vérifie systématiquement : `cargo check`/`cargo test` côté Rust, `tsc --noEmit` + `biome check` + `pnpm build` côté front. Mets à jour `QORE_PLATFORM_ROADMAP.md` (todo ✅/reste) à la fin de chaque jalon. **Ne commit pas** — laisse l'utilisateur le faire.
