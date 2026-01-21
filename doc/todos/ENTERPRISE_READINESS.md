# QoreDB — Enterprise Readiness

> Objectif : atteindre un niveau enterprise crédible sans certification payante, en s’alignant sur les attentes réelles des équipes sécurité.

---

## 🧱 Architecture & Trust Model

| Domaine           | Action                                               | Statut | Notes                 |
| ----------------- | ---------------------------------------------------- | ------ | --------------------- |
| Backend authority | Backend = source of truth (env, read-only, policies) | ✅     | UI jamais trusted     |
| Trust boundaries  | Frontend / Backend / Vault clairement séparés        | ✅     | Documenté             |
| Capability system | Drivers déclarent leurs capacités                    | ✅     | Enforced côté backend |
| Unsafe paths      | APIs “unsafe/dev-only” explicitement isolées         | ✅     | Jamais en prod        |

---

## 🔐 Secrets & Credentials

| Domaine   | Action                              | Statut | Notes          |
| --------- | ----------------------------------- | ------ | -------------- |
| Vault     | Secrets stockés chiffrés localement | ✅     | Pas en clair   |
| Redaction | Secrets jamais loggés               | ⬜     | `SecretString` |
| Access    | Accès secrets backend uniquement    | ✅     | UI jamais      |
| Export    | Pas d’export secrets par défaut     | ✅     | Confirmations  |

---

## 🧯 SQL / Query Safety

| Domaine       | Action                                    | Statut | Notes             |
| ------------- | ----------------------------------------- | ------ | ----------------- |
| SQL parsing   | Classification via AST (pas heuristiques) | ✅     | `sqlparser`       |
| Read-only     | Enforcement backend (prod)                | ✅     | Non bypassable    |
| Dangerous ops | DROP / ALTER / UPDATE sans WHERE bloqués  | ✅     | Règles explicites |
| Tests         | Table de requêtes safe / unsafe           | ✅     | Multi-dialectes   |

---

## ⛔ Query Control & Reliability

| Domaine        | Action                            | Statut | Notes             |
| -------------- | --------------------------------- | ------ | ----------------- |
| Query tracking | `QueryId` par exécution           | ✅     | Multi-parallèle   |
| Cancellation   | Annulation réelle PG / MySQL      | ✅     | Mongo best-effort |
| Timeouts       | Timeout → cancel + cleanup        | ✅     | Driver-aware      |
| Limits         | Max rows / duration configurables | ⬜     | Politique prod    |

---

## 👁️ Observabilité & Auditabilité

| Domaine     | Action                      | Statut | Notes        |
| ----------- | --------------------------- | ------ | ------------ |
| Logging     | Logs structurés (`tracing`) | ✅     | JSON         |
| Correlation | `session_id`, `query_id`    | ✅     | Sans secrets |
| Persistence | Logs locaux avec rotation   | ✅     | Exportable   |
| Support     | Export logs depuis l’UI     | ✅     | One-click    |

---

## 🧪 Qualité & Supply Chain

| Domaine      | Action                         | Statut | Notes              |
| ------------ | ------------------------------ | ------ | ------------------ |
| Tests        | Unit + intégration DB (docker) | ✅     | PG / MySQL / Mongo |
| CI           | Tests automatiques Linux       | ✅     | GitHub Actions     |
| Dependencies | SBOM générée (deps + versions) | ⬜     | Gratuit            |
| Licences     | Licences OSS documentées       | ⬜     | Clair              |

---

## 🧠 IA & Données

| Domaine      | Action                          | Statut | Notes                    |
| ------------ | ------------------------------- | ------ | ------------------------ |
| Opt-in       | IA désactivée par défaut        | ✅     | Consentement explicite   |
| Local-first  | Pas d’exfiltration implicite    | ✅     | Argument clé UE          |
| Transparency | Ce qui est envoyé est documenté | ⬜     | Par feature              |
| Disable      | Mode “no AI” global             | ✅     | Environnements sensibles |

---

## 🌍 GDPR / Privacy by Design

| Domaine   | Action                          | Statut | Notes                   |
| --------- | ------------------------------- | ------ | ----------------------- |
| Data flow | Flux documentés                 | ⬜     | Local / optional remote |
| Telemetry | Off by default                  | ✅     | Opt-in                  |
| Retention | Logs & données temporaires      | ✅     | Clear policy            |
| Export    | Aucun PII sans action explicite | ✅     | Safe default            |

---

## 📄 Documentation & Posture Sécurité

| Document               | Objectif                        | Statut |
| ---------------------- | ------------------------------- | ------ |
| `SECURITY.md`          | Vue d’ensemble sécurité         | ⬜     |
| `THREAT_MODEL.md`      | Menaces & mitigations           | ⬜     |
| `PRODUCTION_SAFETY.md` | Garde-fous prod                 | ⬜     |
| Self-assessment        | Alignement SOC 2 (non certifié) | ⬜     |
| OWASP                  | Alignement Top 10               | ⬜     |

---

## 🏁 Release & Distribution

| Domaine   | Action                   | Statut | Notes             |
| --------- | ------------------------ | ------ | ----------------- |
| Integrity | Checksums des builds     | ⬜     | SHA               |
| Releases  | Changelog clair          | ⬜     | Sécurité incluse  |
| Updates   | Process update documenté | ⬜     | Rollback possible |

---

## 🧭 Positionnement Officiel (sans certif)

| Élément                        | Statut |
| ------------------------------ | ------ |
| SOC 2 aligned (not certified)  | ⬜     |
| Local-first security posture   | ⬜     |
| Open-source auditable          | ✅     |
| Enterprise-ready (sans certif) | ⬜     |
