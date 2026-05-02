# Plan v0.1.27 — Power-user DataGrid & DDL

> **Axe principal** : faire de QoreDB un outil que les power-users adoptent au quotidien en comblant les derniers manques de la grille et de la gestion de schéma vs DBeaver/pgAdmin.
> **Approche** : qualité production sur Windows / macOS / Linux. Pas de quick win. Code irréprochable.

---

## 📦 Périmètre

Trois features Core (Apache-2.0) + un bonus.

| # | Feature | Effort | Licence | Statut roadmap |
| - | ------- | ------ | ------- | -------------- |
| 1 | Blob/Binary Viewer (finition) | S | Core | `v3.md` § UX/UI |
| 2 | Bulk Edit with Preview | M | Core (≤ 5 lignes) / Pro (au-delà) | `v3.md` § UX/UI |
| 3 | DDL Management UI (CREATE + ALTER) | L | Core | `v3.md` § UX/UI |
| 4 | Tab Groups by Connection (bonus) | S/M | Core | `v3.md` § Tab Improvements |

**Hors périmètre** explicite :
- MongoDB pour le DDL (pas de schéma rigide, traité en V3.1 via `CreateCollectionModal`).
- Redis pour le DDL (pas applicable).
- DDL via Sandbox (le Sandbox reste DML-only en V1, voir décisions).

---

## 🎯 Phase 1 — Blob/Binary Viewer

**Objectif** : finaliser la feature déjà à 90 % pour la marquer livrée.

**Fichiers**
- `src/lib/binaryUtils.ts` — ajouter `detectSvg`, élargir `IMAGE_SIGNATURES` (BMP, ICO), exposer `mimeFromSignature`.
- `src/components/Grid/BlobViewer.tsx` — onglet « SVG source », bouton « Copier comme Data URI », ouverture externe via `tauri-plugin-dialog`.
- `src/components/Grid/EditableDataCell.tsx` — raccourci `Space` / `Enter` sur cellule binaire focus pour ouvrir le viewer.
- `src/locales/fr.json` + `en.json` — clés `blobViewer.svgSource`, `blobViewer.copyDataUri`, `blobViewer.openExternal`.

**Events PostHog**
- `blob_viewer_opened` (props : `tab`, `column_type`, `size_bucket`)
- `blob_downloaded` (props : `mime`, `size_bucket`)

---

## 🎯 Phase 2 — Bulk Edit with Preview

**Objectif** : sélection multi-lignes du DataGrid → édition d'une colonne commune → preview SQL → apply (ou push dans Sandbox).

**Décision business** : seuil Core ≤ 5 lignes. Au-delà, gating Pro avec upsell clair.

**Fichiers à créer**
- `src/components/Grid/BulkEditDialog.tsx` (~250 lignes)
- `src/components/Grid/BulkEditButton.tsx` (~80 lignes)
- `src/lib/bulkEdit.ts` (~150 lignes) — helpers purs, génère N `SandboxChangeDto` (un par ligne).

**Fichiers à modifier**
- `src/components/Grid/DataGrid.tsx` — exposer `selectedRows` + `primaryKey` au header, prop `onBulkUpdate`.
- `src/components/Grid/DataGridHeader.tsx` — bouton Bulk Edit (`selectedCount >= 2 && !readOnly && mutationsSupported`).
- `src/components/Browser/TableBrowser.tsx` — câblage Sandbox ON / Sandbox OFF.

**Backend** : aucun nouveau binding. Réutilise `generate_migration_sql` + `apply_sandbox_changes` (transactionnel) déjà en place.

**Cas d'usage**
- Sandbox **ON** → push N changes dans le store, validation par `MigrationPreview` existant.
- Sandbox **OFF** → preview via `generate_migration_sql`, apply via `apply_sandbox_changes` avec `use_transaction = true`.
- **Pas de PK** → opération refusée, message clair (cohérent avec UPDATE/DELETE existants).
- **`selectedCount > 5` sans Pro** → upsell, action désactivée.

**i18n** : sous-arbre `bulkEdit.*` (15 clés environ).

**Events PostHog**
- `bulk_edit_opened` (props : `driver`, `selected_count`)
- `bulk_edit_applied` (props : `driver`, `affected_count`, `via_sandbox`)

---

## 🎯 Phase 3 — DDL Management UI

**Objectif** : interface visuelle CREATE TABLE enrichie + ALTER TABLE, preview SQL en temps réel, exécution directe (pas via Sandbox en V1).

**Décision technique** : génération SQL **côté front** (pas de parser, pas de back). Justification :
- La métadonnée est déjà structurée (`describeTable`), pas de SQL à parser.
- Convention existante (`column-types.ts` génère déjà du `CREATE TABLE`).
- Preview live sans IPC, source de vérité unique pour types/quoting/dialecte.
- Sécurité préservée : exécution via `executeQuery` qui passe par l'intercepteur.

**Découpage fichiers**

`src/lib/ddl/` (nouveau)
- `types.ts` — `TableDefinition`, `ForeignKeyDef`, `IndexDef`, `AlterOp`, `CheckConstraintDef`.
- `createTable.ts` — `buildCreateTableSQL(def, driver, namespace)`.
- `alterTable.ts` — `diffTableDefinitions(initial, edited): AlterOp[]` + `buildAlterTableSQL(...)`. Splitté par driver si > 500 lignes (`alterTable.postgres.ts`, `alterTable.mysql.ts`, `alterTable.sqlite.ts`).
- `typeDefinitions.ts` — refactor de `column-types.ts` (déplacé ici, étendu : ENUM/ARRAY PG, charsets MySQL).

`src/components/Schema/ddl/` (nouveau)
- `CreateTableModal.tsx` (~400 lignes) — remplace `Table/CreateTableModal.tsx`. Mode simple par défaut + onglet avancé (FK, indexes, storage).
- `AlterTableModal.tsx` (~400 lignes) — context menu sur une table, charge `describeTable`, tabs `Columns | Indexes | Foreign Keys | Storage`, preview live.
- `ColumnEditor.tsx` (~250 lignes) — ligne réutilisable.
- `ForeignKeyEditor.tsx` (~150 lignes) — picker autocomplete + ON DELETE/UPDATE.
- `IndexEditor.tsx` (~150 lignes) — type d'index par driver.
- `DDLPreview.tsx` (~80 lignes) — wrapper SqlPreview + warnings.

**Drivers supportés en V1**
- PostgreSQL, MySQL, MariaDB, SQLite, DuckDB, SQL Server, CockroachDB.
- Limitations SQLite (< 3.35) : warning explicite avant apply, pas de DROP/ALTER COLUMN auto.
- Mongo / Redis : boutons « Alter Table » désactivés.

**Backend** : aucun nouveau binding. Exécution statement par statement via `executeQuery` (compteur de progression, erreurs précises).

**i18n** : sous-arbre `ddl.*` (~50 clés).

**Events PostHog**
- `ddl_create_table_opened` / `ddl_create_table_applied`
- `ddl_alter_table_opened` / `ddl_alter_table_applied`

---

## 🎯 Phase 4 — Tab Groups by Connection (bonus)

**Fichiers**
- `src/lib/tabs.ts` — ajouter `connectionId?: string` à `OpenTab`.
- `src/components/Tabs/TabBar.tsx` — refactor en `TabBar` (root) + `TabGroup.tsx` (header pliable + Reorder children).
- Settings : toggle `qoredb_tabs_group_by_connection` (localStorage).
- i18n : `tabs.collapse`, `tabs.expand`, `tabs.groupByConnection`, `tabs.ungrouped`.

**Contraintes** : drag intra-groupe uniquement (pas de réassignation de connexion par drag).

---

## 🧱 Décisions techniques transverses

1. **Pas de parser SQL.** On part toujours de la métadonnée structurée fournie par le backend.
2. **Génération SQL côté front.** Cohérent avec l'existant, preview live, source de vérité unique en TS.
3. **DDL hors Sandbox en V1.** Le Sandbox stocke uniquement du DML. Le DDL a son propre dialog de preview/confirm.
4. **Pas de nouveau binding Tauri** pour les 3 phases. Réutilisation de `executeQuery`, `generate_migration_sql`, `apply_sandbox_changes`, `describeTable`.
5. **Bulk Edit** : seuil Pro à > 5 lignes (vs quota Sandbox standard à 3, inchangé pour les autres flows).
6. **MongoDB / Redis** exclus du DDL (V3.1 pour Mongo via `CreateCollectionModal`).

---

## 🔗 Dépendances et ordre

```
Phase 1  ──►  Phase 2  ──►  Phase 3  ──►  Phase 4 (si temps)
```

Indépendantes fonctionnellement (chacune mergeable seule), mais l'ordre suit la complexité croissante et la stabilisation progressive du DataGrid.

**Phase 3 sera splittée en 4 PR** :
- 3a. Refactor `column-types.ts` → `src/lib/ddl/` (no-op fonctionnel).
- 3b. CreateTable enrichi (FK + indexes + comments + check).
- 3c. AlterTable (le gros morceau).
- 3d. Drivers spécifiques + warnings + i18n complet.

---

## ✅ Checklist de release v0.1.27

### Documentation
- [ ] `doc/todo/v3.md` — cocher Blob Viewer, Bulk Edit, DDL Management UI ; cocher Tab groups si Phase 4 livrée.
- [ ] `doc/FEATURES.csv` — lignes `bulk_edit`, `ddl_create_advanced`, `ddl_alter`, `blob_viewer`.
- [ ] `doc/rules/FEATURES.md` — section dédiée par feature (UX, Sandbox, limitations driver).
- [ ] `doc/rules/DATABASES.md` — tableau « DDL support » par driver.
- [ ] `doc/release/EVENTS.md` — nouveaux events PostHog.
- [ ] `README.md` — bullet « Visual DDL editor ».

### Code
- [ ] Header SPDX `Apache-2.0` sur tous les nouveaux fichiers.
- [ ] Aucun fichier > 500 lignes (split au besoin).
- [ ] `pnpm lint:fix && pnpm format:write` clean.
- [ ] `pnpm test` (Rust) sans régression.
- [ ] i18n FR + EN exhaustive (zéro string en dur).
- [ ] Composants `src/components/ui/` réutilisés.
- [ ] Tests unitaires Vitest pour `bulkEdit.ts`, `createTable.ts`, `alterTable.ts`.
- [ ] Validation cross-OS : raccourcis clavier (Win/macOS/Linux), file dialog (download blob), clipboard (copy data URI).

### Release
- [ ] Bump `package.json`, `Cargo.toml` (workspace + crates), `tauri.conf.json` → `0.1.27`.
- [ ] `aur/PKGBUILD` mis à jour automatiquement par le workflow AUR.
- [ ] Release notes : v0.1.27 + limitations connues (SQLite < 3.35, Mongo exclu).
