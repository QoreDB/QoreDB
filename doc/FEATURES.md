# QoreDB — Liste exhaustive des features "niveau DBeaver"

> **Légende** : `✅ POC` = fait | `V1` = priorité haute | `V2` = power user | `V3` = plateforme | `ENT` = enterprise | `—` = hors scope

---

## 0) Fondations produit

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | **Multi-DB** (SQL + NoSQL) avec ajout progressif de moteurs | POC |
| ✅ | **Local-first** + offline | POC |
| ✅ | **Performances** (UI fluide, streaming, virtualisation) | POC |
| ⬜ | **Stabilité** (crash recovery, logs, diagnostics) | V1 |
| ✅ | **Cross-platform** (Win/macOS/Linux) | POC |
| ✅ | **i18n** (langues) | POC |
| ✅ | **Thèmes** (dark/light + high contrast) | POC |
| ⬜ | **Accessibilité** (clavier, focus, ARIA si webviews) | V2 |

---

## 1) Connexions & sécurité

### Connexion

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Gestion multi-connexions + profils | POC |
| ✅ | Test de connexion | POC |
| ✅ | Paramètres par driver (PG/MySQL/Mongo/…) | POC |
| ⬜ | Connexion via URL DSN | V1 |
| ⬜ | Connexion via paramètres avancés (timeouts, keepalive, app_name) | V2 |
| ⬜ | Connexions "favorites", tags, recherche | V1 |

### Sécurité

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | **Vault chiffré** (credentials, clés) | POC |
| ✅ | Unlock à l'ouverture (PIN/OS keychain) | POC |
| ✅ | Masquage & copy sécurisé | POC |
| ⬜ | Redaction des logs (pas de secrets) | V1 |
| ⬜ | Modes lecture seule | V1 |

Notes:
* Le backend est la source de vérité pour `environment` et `read_only` (métadonnées vault).
* Les garde-fous de production (confirmation/blocks) sont appliqués côté backend.

### Réseau

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | SSL/TLS avancé (certs CA, client cert, verify) | POC |
| ✅ | SSH tunnel (password, key, jump host/bastion) | POC |
| ⬜ | Proxy / corporate network | V2 |
| ⬜ | Retry / reconnexion / keepalive | V1 |

---

## 2) Navigation & exploration des objets

### Arbre DB (Object Explorer)

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Databases, schemas, tables, views, functions, triggers, indexes | POC |
| ⬜ | Recherche dans l'arbre | V1 |
| ⬜ | Filtres (system schemas, etc.) | V1 |
| ⬜ | Favoris d'objets | V2 |

### Métadonnées

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | "Describe" table/collection | POC |
| ✅ | Colonnes & types | POC |
| ✅ | PK/FK/unique | POC |
| ⬜ | Indexes | V1 |
| ⬜ | Contraintes | V1 |
| ⬜ | Triggers | V2 |
| ⬜ | Comments / descriptions | V2 |
| ⬜ | DDL preview (CREATE TABLE, etc.) | V1 |

### NoSQL

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Databases / collections | POC |
| ⬜ | Indexes | V1 |
| ⬜ | Document structure sampling | V2 |
| ⬜ | Explain/plan (si supporté) | V2 |

---

## 3) SQL editor (cœur)

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Multi-tabs | POC |
| ⬜ | Sessions / connexions par onglet | V1 |
| ✅ | Exécution sélection / exécution script | POC |
| ✅ | Stop/cancel query | POC |
| ⬜ | Paramètres de requête (limit, timeout) | V1 |
| ✅ | Historique (par connexion) | POC |
| ✅ | Favoris / snippets | POC |
| ⬜ | Templates (INSERT, UPDATE, SELECT, joins) | V1 |
| ⬜ | Formatter SQL (dialects) | V1 |
| ⬜ | Autocomplétion contextuelle (schema aware) | V1 |
| ⬜ | Hover info (type, doc, constraints) | V2 |
| ⬜ | Lint / warnings | V2 |
| ⬜ | Gestion transactions (autocommit on/off) | V1 |
| ✅ | Query profiler / timings | POC |
| ⬜ | Explain plan / visual | V1 |
| ⬜ | Bind variables / paramètres | V2 |
| ⬜ | Exécution en batch | V2 |
| ⬜ | Résultats multiples (plusieurs statements) | V1 |
| ⬜ | Export du script | V1 |
| ⬜ | Macros / variables d'environnement | V3 |

---

### Modèle d'annulation des requêtes

* Postgres: `pg_cancel_backend(pid)` via une connexion dédiée.
* MySQL: `KILL QUERY <id>` (fallback `KILL CONNECTION` si nécessaire).
* MongoDB: best effort (abort task + fermeture cursor/session), pas de garantie.
* Les timeouts déclenchent une annulation côté driver quand c'est possible.

---

## 4) Résultats & data grid

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Affichage performant (virtualisation) | POC |
| ✅ | Tri multi-colonnes | POC |
| ⬜ | Filtres (par colonne, global) | V1 |
| ⬜ | Recherche dans résultats | V1 |
| ✅ | Pagination / streaming | POC |
| ✅ | Copy/paste (cell, row, selection) | POC |
| ✅ | Affichage types spéciaux (JSON, arrays, bytes, dates) | POC |
| ✅ | Viewer JSON (pretty / collapse) | POC |
| ⬜ | Viewer blobs (hex/base64, télécharger) | V2 |
| ⬜ | Viewer dates (format local/UTC) | V1 |
| ⬜ | Column resizing + reorder | V1 |
| ⬜ | Freeze columns | V2 |
| ⬜ | Masquer/afficher colonnes | V1 |
| ✅ | "NULL" distinct visuellement | POC |
| ⬜ | Résultats persistants (keep results) | V2 |
| ⬜ | Comparaison de résultats | V3 |
| ✅ | Export CSV/JSON/XLSX/SQL inserts | POC |
| ⬜ | Import CSV | V2 |

---

## 5) Data editing / CRUD

### SQL

| Status | Feature | Version |
|--------|---------|---------|
| ⬜ | Edition inline | V1 |
| ⬜ | Insert row | V1 |
| ⬜ | Update cell/row | V1 |
| ⬜ | Delete row(s) | V1 |
| ⬜ | Bulk edit | V2 |
| ⬜ | Validation type | V1 |
| ⬜ | Commit / rollback | V1 |
| ⬜ | Mode safe update (limit, confirmations) | V1 |
| ⬜ | Protection prod (double confirm, read-only) | V1 |

### NoSQL

| Status | Feature | Version |
|--------|---------|---------|
| ⬜ | Edition document JSON | V1 |
| ⬜ | Insert / delete document | V1 |
| ⬜ | Bulk update (with preview) | V2 |
| ⬜ | Validation schema optionnelle | V2 |
| ⬜ | Pagination/limit | V1 |

---

## 6) DDL & schema management

| Status | Feature | Version |
|--------|---------|---------|
| ⬜ | Create/alter table | V1 |
| ⬜ | Create/alter view | V2 |
| ⬜ | Index management | V1 |
| ⬜ | Contraintes (PK/FK) | V1 |
| ⬜ | Triggers & functions (PG surtout) | V2 |
| ⬜ | Visual diff schema | V3 |
| ⬜ | Migrations helpers | V3 |
| ⬜ | DDL export / copy | V1 |

---

## 7) Import / Export / Data tools

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Export results (CSV/JSON/SQL) | POC |
| ⬜ | Export table (dump partiel) | V1 |
| ⬜ | Import CSV → table | V2 |
| ⬜ | Data transfer (copy table between DBs) | V2 |
| ⬜ | Backup/restore helpers | V3 |
| ⬜ | Data generator (seed/fake data) | V3 |

---

## 8) Collaboration & partage

| Status | Feature | Version |
|--------|---------|---------|
| ⬜ | Partage de requêtes (fichiers / workspace) | V2 |
| ⬜ | Query library (team) | V2 |
| ⬜ | Snippets partagés | V2 |
| ⬜ | Templates | V2 |
| ⬜ | Self-host sync | V3 |
| ⬜ | Audit trail (team) | ENT |

---

## 9) IA (différenciateur QoreDB)

| Status | Feature | Version |
|--------|---------|---------|
| ⬜ | Génération SQL depuis langage naturel | V1 |
| ⬜ | Explication requête/résultat | V1 |
| ⬜ | "What does this table do?" (schema summary) | V2 |
| ⬜ | Suggestions d'indexes (prudentes) | V2 |
| ⬜ | Détection requêtes dangereuses | V1 |
| ⬜ | Assistance navigation (command palette + IA) | V2 |
| ⬜ | Privacy modes (local-only, anonymization, consent) | V1 |

---

## 10) UX productivity

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Command palette (type Raycast) | POC |
| ✅ | Raccourcis clavier partout | POC |
| ⬜ | Quick switch connections | V1 |
| ⬜ | Quick open table/collection | V1 |
| ⬜ | Tabs management (pin, reopen closed) | V1 |
| ⬜ | Recent items | V1 |
| ⬜ | Breadcrumbs (namespace/table) | V1 |
| ✅ | Notifications non-intrusives | POC |
| ⬜ | Layout dockable (panels) | V2 |
| ✅ | Search everywhere | POC |

---

## 11) Logs, diagnostics, qualité

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Error journal par connexion | POC |
| ✅ | Logs filtrables | POC |
| ⬜ | Report bug (export diagnostics) | V1 |
| ⬜ | Crash recovery (reopen session) | V1 |
| ⬜ | Health checks drivers | V2 |
| ⬜ | Versioning config + migration config | V2 |

---

## 12) Plugins / extensibilité

| Status | Feature | Version |
|--------|---------|---------|
| ✅ | Registry de drivers | POC |
| ✅ | API driver stable | POC |
| ⬜ | Capability system (transactions, schema, streaming) | V2 |
| ⬜ | Hooks UI (menus contextuels, actions) | V2 |
| ⬜ | Command registration | V2 |
| ⬜ | Export/import plugins | V3 |
| ⬜ | Marketplace | V3 |

---

## 13) Compliance / entreprise

| Status | Feature | Version |
|--------|---------|---------|
| ⬜ | Désactivation télémétrie | V1 |
| ⬜ | Modes stricts (no network) | V2 |
| ⬜ | Policy config (locks) | ENT |
| ⬜ | SSO | ENT |
| ⬜ | Role-based access | ENT |
| ⬜ | Audit logs | ENT |
| ⬜ | Support & SLA | ENT |

---

## Résumé par version

| Version | Features à faire | Focus |
|---------|-----------------|-------|
| **POC** | ✅ ~45 done | Fondations, connexions, query basique |
| **V1** | ~50 features | CRUD, autocomplétion, IA basique, UX polish |
| **V2** | ~35 features | Power user, collaboration, import/export avancé |
| **V3** | ~15 features | Plateforme, plugins, sync |
| **ENT** | ~7 features | SSO, RBAC, audit, support |
