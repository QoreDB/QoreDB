# SOC 2 Trust Service Criteria — Self-Assessment

> **Statut :** Self-assessment (non certifié)
> **Date :** 2026-03-23
> **Projet :** QoreDB — Desktop database client (Tauri 2 / Rust / React)

QoreDB est une application desktop local-first. Ce self-assessment mappe les controles existants aux critères SOC 2 pour démontrer l'alignement avec les attentes enterprise, sans certification formelle.

---

## CC1 — Control Environment

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC1.1 — Engagement envers l'intégrité et l'éthique | Open source (Apache 2.0), code auditable publiquement | ✅ |
| CC1.2 — Gouvernance | CLAUDE.md, RELEASE.md, THREAT_MODEL.md documentent les standards | ✅ |
| CC1.3 — Structure organisationnelle | Rôles définis dans le modèle open-core (core vs premium) | ✅ |
| CC1.4 — Compétences | Audit sécurité documenté (Jan 2026), code review systématique | ✅ |

---

## CC2 — Communication and Information

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC2.1 — Information interne | CHANGELOG.md auto-généré, release notes structurées (git-cliff) | ✅ |
| CC2.2 — Communication externe | SECURITY.md, disclosure responsable, release notes publiques | ✅ |
| CC2.3 — Canaux sécurisés | Vault chiffré (OS Keychain), logs redactés (`Sensitive<T>`) | ✅ |

---

## CC3 — Risk Assessment

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC3.1 — Objectifs de sécurité définis | `doc/security/THREAT_MODEL.md` : 4 menaces identifiées et mitigées | ✅ |
| CC3.2 — Identification des risques | Credential theft, data destruction, supply chain, data leaks | ✅ |
| CC3.3 — Évaluation de la fraude | N/A (application desktop, pas de service cloud multi-tenant) | N/A |
| CC3.4 — Changements significatifs | Audit sécurité post-changements majeurs, CI automatisé | ✅ |

---

## CC4 — Monitoring Activities

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC4.1 — Surveillance continue | CI : cargo-audit, pnpm audit, cargo-deny (advisories + licences) | ✅ |
| CC4.2 — Évaluation des déficiences | Structured logging (`tracing`), panic hooks, error boundaries (React) | ✅ |
| CC4.3 — Communication des résultats | Logs exportables (one-click), rotation 7 jours | ✅ |

---

## CC5 — Control Activities

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC5.1 — Sélection des controles | Politique de sécurité configurable par environnement (Dev/Staging/Prod) | ✅ |
| CC5.2 — Controles technologiques | SQL safety AST-based (`sqlparser`), read-only mode, dangerous query blocking | ✅ |
| CC5.3 — Politiques de déploiement | CI/CD GitHub Actions, builds signés, checksums SHA-256 | ✅ |

---

## CC6 — Logical and Physical Access Controls

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC6.1 — Accès logique | Credentials stockés dans OS Keychain (pas de fichiers plats) | ✅ |
| CC6.2 — Authentification | Master Password (Argon2), authentification OS (TouchID/Password) | ✅ |
| CC6.3 — Enregistrement des accès | Query ID + Session ID dans les logs structurés | ✅ |
| CC6.4 — Restriction des accès physiques | N/A (application desktop locale) | N/A |
| CC6.5 — Gestion des clés | Ed25519 pour licences, Argon2 pour master password, OS Keychain pour credentials | ✅ |
| CC6.6 — Menaces externes | CSP configuré, pas de `eval()` / `dangerouslySetInnerHTML`, Tauri sandbox | ✅ |
| CC6.7 — Transmission des données | SSH tunneling, TLS/SSL pour les connexions DB, `rediss://` pour Redis | ✅ |
| CC6.8 — Controle des données en transit | Credentials jamais transmis en clair, `Sensitive<T>` dans les logs | ✅ |

---

## CC7 — System Operations

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC7.1 — Détection des anomalies | Structured logging avec corrélation (session_id, query_id) | ✅ |
| CC7.2 — Surveillance des composants | cargo-audit, pnpm audit, cargo-deny dans CI | ✅ |
| CC7.3 — Évaluation des événements | Classification des requêtes (mutation, dangerous, safe) | ✅ |
| CC7.4 — Réponse aux incidents | Rollback documenté, auto-updater pour hotfixes | ✅ |
| CC7.5 — Récupération | Vault persiste dans OS Keychain, logs exportables | ✅ |

---

## CC8 — Change Management

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC8.1 — Gestion des changements | Git, conventional commits, CI automatisé, release draft review | ✅ |

---

## CC9 — Risk Mitigation

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| CC9.1 — Identification des risques fournisseurs | SBOM CycloneDX (Rust + Frontend), cargo-deny (licences + advisories) | ✅ |
| CC9.2 — Évaluation des risques | `deny.toml` bloque copyleft, registries inconnus, vulnérabilités connues | ✅ |

---

## Availability (A1)

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| A1.1 — Capacité de récupération | Application desktop locale, pas de dépendance cloud pour le fonctionnement de base | ✅ |
| A1.2 — Gestion des incidents environnementaux | Auto-updater Tauri, rollback documenté | ✅ |

---

## Confidentiality (C1)

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| C1.1 — Identification des données confidentielles | Credentials, SSH keys, query results classifiés comme sensibles | ✅ |
| C1.2 — Protection des données confidentielles | OS Keychain, `Sensitive<T>`, logs redactés, telemetry opt-in | ✅ |

---

## Processing Integrity (PI1)

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| PI1.1 — Exactitude du traitement | SQL parsing AST-based multi-dialectes, parameterized queries | ✅ |
| PI1.2 — Détection des erreurs | Error boundaries (React), panic hooks (Rust), structured error types | ✅ |

---

## Privacy (P1)

| Critère | Controle QoreDB | Statut |
|---------|-----------------|--------|
| P1.1 — Notice de confidentialité | Telemetry opt-in, consentement explicite dans l'onboarding | ✅ |
| P1.2 — Choix et consentement | Analytics désactivées par défaut, mode "no AI" global | ✅ |
| P1.3 — Collecte minimale | Local-first, pas d'exfiltration implicite, PostHog EU servers | ✅ |
| P1.4 — Utilisation et rétention | Rotation logs 14 jours, `resetIdentity()` sur opt-out analytics | ✅ |
| P1.5 — Accès aux données | Export logs one-click, pas de PII sans action explicite | ✅ |

---

## Gaps identifiés

| Gap | Priorité | Plan |
|-----|----------|------|
| Pas de certification SOC 2 formelle | Basse | Réévaluer quand le volume de clients enterprise le justifie |
| Session timeout / auto-lock du vault | Moyenne | Prévu pour une prochaine itération |
| Audit trail persistant séparé (mutations) | Moyenne | L'intercepteur de requêtes capture les mutations, mais pas dans un fichier d'audit dédié |

---

_Self-assessment généré le 2026-03-23. À mettre à jour lors de changements architecturaux majeurs._
