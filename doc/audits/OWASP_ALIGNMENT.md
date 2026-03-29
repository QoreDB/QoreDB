# OWASP Top 10 — Alignment Assessment

> **Statut :** Self-assessment
> **Date :** 2026-03-23
> **Projet :** QoreDB — Desktop database client (Tauri 2 / Rust / React)
> **Référence :** OWASP Top 10:2021

QoreDB est une application desktop, pas un service web. Certaines vulnérabilités OWASP s'appliquent directement (injection, composants vulnérables), d'autres sont adaptées au contexte desktop/Tauri.

---

## A01:2021 — Broken Access Control

**Risque :** Accès non autorisé à des fonctions ou données.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Backend = source of truth | Le frontend ne peut pas bypasser les politiques backend | `src-tauri/src/commands/` |
| Read-only mode | Enforced côté Rust, pas côté UI | `src-tauri/src/engine/sql_safety.rs` |
| Environment classification | Production → restrictions automatiques | `src-tauri/src/policy.rs` |
| Capability system | Drivers déclarent leurs capacités (mutations, transactions, etc.) | `src-tauri/src/engine/traits.rs` |

**Statut : ✅ Mitigé**

---

## A02:2021 — Cryptographic Failures

**Risque :** Données sensibles exposées en clair ou chiffrées de manière inadéquate.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Credentials dans OS Keychain | Pas de fichiers plats, chiffrement natif OS | `src-tauri/src/vault/storage.rs` |
| Master Password Argon2 | Hashing robuste avec sel | `src-tauri/src/vault/lock.rs` |
| `Sensitive<T>` wrapper | Redaction automatique dans logs/serialization | `src-tauri/src/observability/sensitive.rs` |
| License verification Ed25519 | Signatures cryptographiques | `src-tauri/src/license/key.rs` |
| TLS pour connexions DB | SQLx + Rustls, `rediss://` pour Redis | `src-tauri/Cargo.toml` (tls-rustls) |

**Statut : ✅ Mitigé**

---

## A03:2021 — Injection

**Risque :** Injection SQL, commande, ou script.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Parameterized queries | INSERT/UPDATE/DELETE utilisent des requêtes paramétrées | Tous les drivers dans `src-tauri/src/engine/drivers/` |
| SQL classification AST | `sqlparser` pour analyser les requêtes (pas d'heuristiques) | `src-tauri/src/engine/sql_safety.rs` |
| Dangerous query blocking | DROP, TRUNCATE, ALTER, DELETE/UPDATE sans WHERE bloqués en prod | `src-tauri/src/engine/sql_safety.rs` |
| Pas de `eval()` | Aucun usage de `eval()`, `new Function()`, ou `dangerouslySetInnerHTML` | Audit frontend (Jan 2026) |
| CSP strict | `default-src 'self'; script-src 'self'` | `src-tauri/tauri.conf.json` |

**Note :** `execute_query` exécute du SQL brut par design (c'est un client DB). La mitigation repose sur la classification et le blocage, pas sur la prévention de l'exécution.

**Statut : ✅ Mitigé**

---

## A04:2021 — Insecure Design

**Risque :** Failles architecturales fondamentales.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Threat model documenté | 4 menaces identifiées et mitigées | `doc/security/THREAT_MODEL.md` |
| Trust boundaries | Frontend / Backend / Vault séparés | Architecture Tauri |
| Security audit | Audit professionnel (Jan 2026) | `doc/audits/SECURITY_AUDIT.md` |
| Privacy by Design | GDPR audit, telemetry opt-in, local-first | `doc/audits/GDPR_AUDIT.md` |

**Statut : ✅ Mitigé**

---

## A05:2021 — Security Misconfiguration

**Risque :** Configuration par défaut insécure.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| CSP configuré | Strict CSP (corrigé depuis audit Jan 2026) | `src-tauri/tauri.conf.json` |
| Telemetry off par défaut | Opt-in explicite requis | `src/lib/analytics.ts` |
| AI désactivée par défaut | Consentement explicite | `src/components/AI/` |
| Production safety par défaut | Dangerous queries bloquées en prod | `src-tauri/src/policy.rs` |

**Statut : ✅ Mitigé**

---

## A06:2021 — Vulnerable and Outdated Components

**Risque :** Dépendances avec des vulnérabilités connues.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| `cargo-audit` | Scan vulnérabilités Rust dans CI | `.github/workflows/ci.yml` |
| `pnpm audit` | Scan vulnérabilités npm dans CI | `.github/workflows/ci.yml` |
| `cargo-deny` | Licences + advisories + sources bloquants | `src-tauri/deny.toml` |
| SBOM CycloneDX | Bill of Materials publié avec chaque release | `.github/workflows/release.yml` |
| Checksums SHA-256 | Intégrité des binaires vérifiable | `.github/workflows/release.yml` |

**Statut : ✅ Mitigé**

---

## A07:2021 — Identification and Authentication Failures

**Risque :** Authentification faible ou contournable.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Master Password | Argon2 hashing, tentatives non limitées (app locale) | `src-tauri/src/vault/lock.rs` |
| OS-level auth | TouchID / Windows Hello / mot de passe OS pour accéder au Keychain | OS natif via `keyring` |
| SSH auth | Support clé + password, host key verification | `src-tauri/src/vault/credentials.rs` |

**Note :** QoreDB est une app locale single-user. L'authentification protège contre l'accès physique non autorisé, pas contre des attaquants réseau.

**Statut : ✅ Mitigé** (contexte desktop)

---

## A08:2021 — Software and Data Integrity Failures

**Risque :** Code ou données modifiés sans vérification.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Builds signés | Code signing macOS + Windows MSIX | `.github/workflows/release.yml` |
| Checksums SHA-256 | Publiés avec chaque release | Job `cleanup-sig` dans release.yml |
| Auto-updater sécurisé | Signatures Tauri vérifiées avant installation | `src-tauri/tauri.conf.json` (pubkey) |
| SBOM | Traçabilité des dépendances | Job `sbom` dans release.yml |
| Supply chain deny | Registries inconnus bloqués | `src-tauri/deny.toml` |

**Statut : ✅ Mitigé**

---

## A09:2021 — Security Logging and Monitoring Failures

**Risque :** Incidents non détectés par manque de logs.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Structured logging | `tracing` framework, JSON, rotation journalière | `src-tauri/src/observability.rs` |
| Corrélation | `session_id` + `query_id` dans chaque log | `src-tauri/src/engine/session_manager.rs` |
| Panic hooks | Crashs capturés et loggés | `src-tauri/src/observability.rs` |
| Log export | One-click depuis l'UI | Commande Tauri `collect_logs` |
| Redaction | `Sensitive<T>` empêche les credentials dans les logs | `src-tauri/src/observability/sensitive.rs` |
| Retention | 7 jours avec cleanup automatique | `src-tauri/src/observability.rs` |

**Statut : ✅ Mitigé**

---

## A10:2021 — Server-Side Request Forgery (SSRF)

**Risque :** L'application fait des requêtes vers des URLs contrôlées par l'attaquant.

| Controle | Implémentation | Fichier |
|----------|---------------|---------|
| Pas de proxy HTTP | QoreDB ne fait pas de requêtes HTTP au nom de l'utilisateur (sauf AI opt-in) | N/A |
| Connexions DB directes | L'utilisateur fournit explicitement host/port/credentials | `src-tauri/src/vault/credentials.rs` |
| AI provider validation | URLs des providers AI validées (OpenAI, Anthropic, local) | `src-tauri/src/ai/provider.rs` |
| CSP connect-src | Limité à `ipc:`, `localhost`, `*.posthog.com` | `src-tauri/tauri.conf.json` |

**Statut : ✅ Mitigé** (surface d'attaque SSRF minimale pour une app desktop)

---

## Résumé

| Vulnérabilité OWASP | Statut | Controle principal |
|---------------------|--------|-------------------|
| A01 — Broken Access Control | ✅ | Backend authority, read-only mode, environment policy |
| A02 — Cryptographic Failures | ✅ | OS Keychain, Argon2, Sensitive<T>, TLS |
| A03 — Injection | ✅ | Parameterized queries, AST SQL classification, CSP |
| A04 — Insecure Design | ✅ | Threat model, security audit, trust boundaries |
| A05 — Security Misconfiguration | ✅ | CSP strict, secure defaults (telemetry off, AI off) |
| A06 — Vulnerable Components | ✅ | cargo-audit, pnpm audit, cargo-deny, SBOM |
| A07 — Auth Failures | ✅ | Argon2, OS Keychain, SSH key verification |
| A08 — Integrity Failures | ✅ | Code signing, checksums, SBOM, supply chain deny |
| A09 — Logging Failures | ✅ | Structured logging, correlation, redaction, export |
| A10 — SSRF | ✅ | Pas de proxy HTTP, CSP connect-src restrictif |

---

## Gaps identifiés

| Gap | Priorité | Plan |
|-----|----------|------|
| Rate limiting sur les requêtes DB | Moyenne | Max rows/duration configurable (prévu) |
| Session timeout / auto-lock | Moyenne | Prochaine itération |
| Audit trail dédié pour les mutations | Basse | L'intercepteur capture les mutations dans les logs généraux |

---

_Self-assessment généré le 2026-03-23. À mettre à jour lors de changements architecturaux majeurs._
