# Data Time-Travel — Plan d'implémentation

> **Feature** : "Git blame" pour les données — timeline des mutations, diff temporel, rollback SQL.
> **Tier** : Pro (BUSL-1.1)
> **Scope** : Mutations faites via QoreDB uniquement (pas de WAL/binlog)

---

## Table des matières

1. [Analyse de l'existant et gap](#1-analyse-de-lexistant-et-gap)
2. [Architecture globale](#2-architecture-globale)
3. [Phase 1 — Changelog Store (Backend Rust)](#phase-1--changelog-store-backend-rust)
4. [Phase 2 — Capture des mutations](#phase-2--capture-des-mutations)
5. [Phase 3 — Commandes Tauri Time-Travel](#phase-3--commandes-tauri-time-travel)
6. [Phase 4 — Frontend : Timeline visuelle](#phase-4--frontend--timeline-visuelle)
7. [Phase 5 — Frontend : Diff temporel](#phase-5--frontend--diff-temporel)
8. [Phase 6 — Rollback SQL Generator](#phase-6--rollback-sql-generator)
9. [Phase 7 — Filtres avancés (user/session/plage)](#phase-7--filtres-avancés-usersessionplage)
10. [Phase 8 — Settings, i18n, License gating](#phase-8--settings-i18n-license-gating)
11. [Phase 9 — Performance & limites](#phase-9--performance--limites)
12. [Phase 10 — Tests](#phase-10--tests)
13. [Phase 11 — Documentation](#phase-11--documentation)
14. [Decisions d'architecture (ADR)](#decisions-darchitecture)
15. [Risques et mitigations](#risques-et-mitigations)
16. [Fichiers créés/modifiés (récapitulatif)](#fichiers-créésmodifiés-récapitulatif)

---

## 1. Analyse de l'existant et gap

### Ce qui existe

| Composant | État actuel | Utilisable ? |
|-----------|------------|--------------|
| **Interceptor pipeline** (`src-tauri/src/interceptor/`) | Capture chaque query avec timestamp, session_id, operation_type, row_count, success | Partiellement — pas de before/after data |
| **AuditStore** (`interceptor/audit.rs`) | Persiste en JSONL, rotation automatique, cache 1000 entries | Oui pour la timeline de queries |
| **Mutation commands** (`commands/mutation.rs`) | `insert_row`, `update_row`, `delete_row` passent par l'interceptor | Point d'injection idéal |
| **Visual Data Diff** (`src/components/Diff/`) | DiffResultsGrid, DiffStatsBar, diffUtils.ts, useDiffSources | Réutilisable pour le diff temporel |
| **Snapshot Store** (`src-tauri/src/snapshots/`) | Persiste des QueryResult complets en JSON | Pattern de stockage réutilisable |
| **SQL Parser** (`sqlparser 0.60`) | Parse AST multi-dialectes, rewriting dans federation | Pour générer le SQL de rollback |
| **Tab system** (`src/lib/tabs.ts`) | Factory functions, 7 types de tabs | Étendre avec `'time-travel'` |
| **License gating** | `#[cfg(feature = "pro")]` + `LicenseGate` component | Pattern à suivre |

### Gap critique

**Le système actuel ne capture PAS les valeurs avant/après mutation.**

L'`AuditLogEntry` stocke :
- Le texte de la query (redacté)
- Le nombre de rows affectées
- Le succès/échec

Il manque :
- **Before-image** : état de la row AVANT update/delete
- **After-image** : état de la row APRÈS insert/update
- **Primary key** : identification de la row mutée
- **Column-level changes** : quelles colonnes ont changé

C'est le coeur de ce plan : créer un **Changelog Store** dédié qui capture les snapshots row-level à chaque mutation.

---

## 2. Architecture globale

```
┌─────────────────────────────────────────────────────────────┐
│                        Frontend                              │
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────────────┐  │
│  │ TimeTravel   │  │ Temporal     │  │ Rollback          │  │
│  │ Timeline     │  │ DiffViewer   │  │ Preview           │  │
│  │ (new tab)    │  │ (reuse Diff) │  │ (SQL generator)   │  │
│  └──────┬───────┘  └──────┬───────┘  └──────┬────────────┘  │
│         │                  │                  │               │
│         └──────────┬───────┴──────────┬───────┘               │
│                    │  Tauri Commands  │                       │
├────────────────────┴─────────────────┴───────────────────────┤
│                        Backend (Rust)                         │
│                                                              │
│  ┌──────────────────────────────────────────────────────┐    │
│  │              Time-Travel Module                       │    │
│  │  ┌─────────────┐ ┌─────────────┐ ┌───────────────┐  │    │
│  │  │ Changelog   │ │ Diff Engine │ │ Rollback      │  │    │
│  │  │ Store       │ │ (temporal)  │ │ Generator     │  │    │
│  │  └──────┬──────┘ └─────────────┘ └───────────────┘  │    │
│  └─────────┼────────────────────────────────────────────┘    │
│            │                                                  │
│  ┌─────────▼──────────────────────────────────────────┐      │
│  │           Mutation Commands (enriched)              │      │
│  │  insert_row() → capture after-image                │      │
│  │  update_row() → capture before + after image       │      │
│  │  delete_row() → capture before-image               │      │
│  └────────────────────────────────────────────────────┘      │
│                                                              │
│  ┌──────────────────┐  ┌────────────────────────┐           │
│  │ Interceptor      │  │ DataEngine trait        │           │
│  │ (audit timeline) │  │ (fetch before-image)    │           │
│  └──────────────────┘  └────────────────────────┘           │
└──────────────────────────────────────────────────────────────┘

Storage:
  {data_dir}/com.qoredb.app/time-travel/
  ├── changelog.jsonl          # Append-only changelog
  ├── changelog.idx            # Index by table+PK (binaire)
  └── time-travel.json         # Configuration
```

---

## Phase 1 — Changelog Store (Backend Rust)

> **Objectif** : Créer le système de stockage persistant pour les changements row-level.

### 1.1 Types (`src-tauri/src/time_travel/types.rs`)

```rust
// SPDX-License-Identifier: BUSL-1.1

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Un enregistrement de changement sur une seule row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangelogEntry {
    /// Identifiant unique du changement
    pub id: Uuid,
    /// Timestamp UTC de la mutation
    pub timestamp: DateTime<Utc>,
    /// ID de session (connexion active)
    pub session_id: String,
    /// Driver ayant exécuté la mutation
    pub driver_id: String,
    /// Namespace (database + optional schema)
    pub namespace: Namespace,
    /// Nom de la table
    pub table_name: String,
    /// Type d'opération
    pub operation: ChangeOperation,
    /// Colonnes de la clé primaire et leurs valeurs
    pub primary_key: HashMap<String, serde_json::Value>,
    /// État AVANT la mutation (None pour INSERT)
    pub before: Option<HashMap<String, serde_json::Value>>,
    /// État APRÈS la mutation (None pour DELETE)
    pub after: Option<HashMap<String, serde_json::Value>>,
    /// Colonnes modifiées (vide pour INSERT/DELETE)
    pub changed_columns: Vec<String>,
    /// Nom de connexion (display name, pour le filtre "par utilisateur")
    pub connection_name: Option<String>,
    /// Environnement (development/staging/production)
    pub environment: String,
    /// Référence optionnelle vers l'audit entry correspondante
    pub audit_entry_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOperation {
    Insert,
    Update,
    Delete,
}

/// Résumé agrégé pour la timeline
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    /// Timestamp de l'événement
    pub timestamp: DateTime<Utc>,
    /// Type d'opération
    pub operation: ChangeOperation,
    /// Nombre de rows affectées dans cet événement
    pub row_count: usize,
    /// Session ayant fait la mutation
    pub session_id: String,
    /// Nom de connexion
    pub connection_name: Option<String>,
    /// IDs des changelog entries pour drill-down
    pub entry_ids: Vec<Uuid>,
}

/// Résultat d'un diff temporel entre deux points
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiff {
    /// Colonnes de la table
    pub columns: Vec<String>,
    /// Rows avec leur status de changement
    pub rows: Vec<TemporalDiffRow>,
    /// Statistiques
    pub stats: TemporalDiffStats,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiffRow {
    /// Clé primaire de la row
    pub primary_key: HashMap<String, serde_json::Value>,
    /// État au point T1 (None si la row n'existait pas)
    pub state_at_t1: Option<HashMap<String, serde_json::Value>>,
    /// État au point T2 (None si la row a été supprimée)
    pub state_at_t2: Option<HashMap<String, serde_json::Value>>,
    /// Colonnes modifiées entre T1 et T2
    pub changed_columns: Vec<String>,
    /// Status global de la row
    pub status: DiffRowStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DiffRowStatus {
    Added,
    Modified,
    Removed,
    Unchanged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalDiffStats {
    pub added: usize,
    pub modified: usize,
    pub removed: usize,
    pub total_changes: usize,
}

/// Configuration du time-travel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeTravelConfig {
    /// Activer/désactiver la capture
    pub enabled: bool,
    /// Nombre max d'entries dans le changelog (rotation)
    pub max_entries: usize,
    /// Durée de rétention en jours (0 = illimité)
    pub retention_days: u32,
    /// Taille max du fichier changelog en MB
    pub max_file_size_mb: u64,
    /// Tables exclues de la capture (pattern glob)
    pub excluded_tables: Vec<String>,
    /// Capture uniquement les mutations en production ?
    pub production_only: bool,
}

impl Default for TimeTravelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_entries: 50_000,
            retention_days: 30,
            max_file_size_mb: 500,
            excluded_tables: vec![],
            production_only: false,
        }
    }
}

/// Filtres pour requêter le changelog
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ChangelogFilter {
    pub table_name: Option<String>,
    pub namespace: Option<Namespace>,
    pub operation: Option<ChangeOperation>,
    pub session_id: Option<String>,
    pub connection_name: Option<String>,
    pub environment: Option<String>,
    pub from_timestamp: Option<DateTime<Utc>>,
    pub to_timestamp: Option<DateTime<Utc>>,
    pub primary_key: Option<HashMap<String, serde_json::Value>>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}
```

### 1.2 Changelog Store (`src-tauri/src/time_travel/store.rs`)

**Responsabilités** :
- Persistance append-only en JSONL (même pattern que `AuditStore`)
- Cache in-memory des N dernières entries (configurable, défaut 5000)
- Rotation automatique quand le fichier dépasse `max_entries`
- Rétention temporelle : purge des entries > `retention_days`
- Index en mémoire par `(namespace, table_name)` pour lookup rapide
- Index secondaire par `(namespace, table_name, primary_key)` pour le diff d'une row

```rust
pub struct ChangelogStore {
    entries: RwLock<VecDeque<ChangelogEntry>>,
    table_index: RwLock<HashMap<String, Vec<usize>>>,  // table_key → entry indices
    log_path: PathBuf,       // {data_dir}/time-travel/changelog.jsonl
    config: RwLock<TimeTravelConfig>,
    config_path: PathBuf,    // {data_dir}/time-travel/time-travel.json
    file_line_count: AtomicUsize,
    max_cache_entries: usize, // 5000
}
```

**Méthodes publiques** :

| Méthode | Description |
|---------|-------------|
| `new(data_dir: PathBuf) -> Self` | Initialise, crée le dossier, charge le cache |
| `record(entry: ChangelogEntry)` | Append entry au fichier + cache + index |
| `get_timeline(filter: &ChangelogFilter) -> Vec<TimelineEvent>` | Agrège les entries en événements timeline |
| `get_entries(filter: &ChangelogFilter) -> Vec<ChangelogEntry>` | Retourne les entries filtrées |
| `get_row_history(namespace, table, pk) -> Vec<ChangelogEntry>` | Historique d'une row spécifique |
| `compute_temporal_diff(namespace, table, t1, t2) -> TemporalDiff` | Diff entre deux timestamps |
| `get_row_state_at(namespace, table, pk, timestamp) -> Option<HashMap>` | Reconstruit l'état à un instant T |
| `clear_table(namespace, table)` | Purge le changelog d'une table |
| `clear_all()` | Purge tout le changelog |
| `load_config() / save_config()` | Persistance de la config |
| `rotate()` | Rotation du fichier JSONL |
| `purge_expired()` | Purge les entries expirées (retention_days) |
| `export(filter) -> String` | Export JSON des entries filtrées |

### 1.3 Module declaration (`src-tauri/src/time_travel/mod.rs`)

```rust
// SPDX-License-Identifier: BUSL-1.1
pub mod store;
pub mod types;
pub mod rollback;

pub use store::ChangelogStore;
pub use types::*;
```

### 1.4 Fichiers créés

| Fichier | Lignes estimées |
|---------|-----------------|
| `src-tauri/src/time_travel/mod.rs` | ~10 |
| `src-tauri/src/time_travel/types.rs` | ~180 |
| `src-tauri/src/time_travel/store.rs` | ~500 |

### 1.5 Checklist de vérification

- [ ] Le store crée le dossier `time-travel/` au démarrage
- [ ] L'écriture JSONL est append-only et thread-safe (RwLock)
- [ ] La rotation conserve 75% des entries (même logique que AuditStore)
- [ ] Le cache in-memory est borné à `max_cache_entries`
- [ ] Les filtres par table/namespace/timestamp fonctionnent correctement
- [ ] `get_row_history` retourne les entries ordonnées par timestamp DESC
- [ ] La config se persiste et se recharge au redémarrage

---

## Phase 2 — Capture des mutations

> **Objectif** : Enrichir les mutation commands existantes pour capturer les before/after images.

### 2.1 Stratégie de capture

Pour chaque type de mutation, la stratégie est :

| Opération | Before-image | After-image | Méthode |
|-----------|-------------|-------------|---------|
| **INSERT** | N/A | Row insérée (= `data` param) | Directe depuis les params de la commande |
| **UPDATE** | Fetch via PK avant update | Row updatée (before + data merged) | `SELECT * WHERE pk = ?` avant mutation |
| **DELETE** | Fetch via PK avant delete | N/A | `SELECT * WHERE pk = ?` avant mutation |

### 2.2 Helper : `fetch_row_by_pk`

Ajouter une fonction utilitaire dans `src-tauri/src/time_travel/capture.rs` :

```rust
// SPDX-License-Identifier: BUSL-1.1

/// Récupère l'état actuel d'une row via sa clé primaire.
/// Utilisé pour capturer le before-image avant UPDATE/DELETE.
pub async fn fetch_row_by_pk(
    driver: &Arc<dyn DataEngine>,
    session_id: SessionId,
    namespace: &Namespace,
    table: &str,
    primary_key: &RowData,
) -> Option<HashMap<String, serde_json::Value>> {
    // 1. Construire un filtre sur les colonnes PK
    // 2. Appeler driver.query_table() avec les filtres
    // 3. Convertir le Row résultat en HashMap<String, Value>
    // 4. Return None si pas de résultat (row already deleted, etc.)
}

/// Construit un ChangelogEntry à partir des données de mutation.
pub fn build_changelog_entry(
    session_id: &str,
    driver_id: &str,
    namespace: &Namespace,
    table: &str,
    operation: ChangeOperation,
    primary_key: &RowData,
    before: Option<HashMap<String, serde_json::Value>>,
    after: Option<HashMap<String, serde_json::Value>>,
    connection_name: Option<&str>,
    environment: &str,
    audit_entry_id: Option<Uuid>,
) -> ChangelogEntry {
    let changed_columns = compute_changed_columns(&before, &after);
    ChangelogEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        session_id: session_id.to_string(),
        driver_id: driver_id.to_string(),
        namespace: namespace.clone(),
        table_name: table.to_string(),
        operation,
        primary_key: rowdata_to_json_map(primary_key),
        before,
        after,
        changed_columns,
        connection_name: connection_name.map(String::from),
        environment: environment.to_string(),
        audit_entry_id,
    }
}

/// Détermine les colonnes modifiées entre before et after.
fn compute_changed_columns(
    before: &Option<HashMap<String, serde_json::Value>>,
    after: &Option<HashMap<String, serde_json::Value>>,
) -> Vec<String> {
    match (before, after) {
        (Some(b), Some(a)) => {
            a.iter()
                .filter(|(k, v)| b.get(*k) != Some(v))
                .map(|(k, _)| k.clone())
                .collect()
        }
        _ => vec![], // INSERT ou DELETE : pas de "changed" columns
    }
}
```

### 2.3 Modification de `commands/mutation.rs`

**Principe** : Injecter la capture AVANT et APRÈS chaque mutation, sans casser le flow existant.

**Pour `update_row()`** :
```
1. [existant] Safety checks, interceptor pre_execute
2. [NOUVEAU] fetch_row_by_pk() → before_image
3. [existant] driver.update_row()
4. [NOUVEAU] Construire after_image = before_image.merge(data)
5. [NOUVEAU] changelog_store.record(entry)
6. [existant] interceptor post_execute
```

**Pour `delete_row()`** :
```
1. [existant] Safety checks, interceptor pre_execute
2. [NOUVEAU] fetch_row_by_pk() → before_image
3. [existant] driver.delete_row()
4. [NOUVEAU] changelog_store.record(entry) avec before_image
5. [existant] interceptor post_execute
```

**Pour `insert_row()`** :
```
1. [existant] Safety checks, interceptor pre_execute
2. [existant] driver.insert_row()
3. [NOUVEAU] Construire after_image depuis data param
4. [NOUVEAU] changelog_store.record(entry) avec after_image
5. [existant] interceptor post_execute
```

### 2.4 Capture des mutations via `execute_query()`

Les mutations directes via SQL (ex: `UPDATE users SET name = 'x' WHERE id = 1`) passent par `commands/query.rs`, PAS par `mutation.rs`. Pour ces cas :

**Stratégie** : On ne capture PAS les before/after pour les queries SQL brutes.

**Justification** :
- Parser le SQL pour extraire les rows affectées est extrêmement complexe (subqueries, JOINs, expressions)
- Le SELECT avant mutation serait un équivalent du WHERE clause — risque de divergence
- Cela nuirait aux performances sur les updates en masse
- Le scope V3 dit explicitement "mutations faites via QoreDB" → les mutations via DataGrid sont le focus

**Ce qu'on fait quand même** : L'audit trail existant (`AuditStore`) continue de logger les queries SQL avec leur type et row_count. La timeline les affichera comme "raw SQL mutation" sans before/after data.

### 2.5 Intégration dans AppState

Ajouter `ChangelogStore` dans `AppState` :

```rust
// Dans src-tauri/src/lib.rs
pub struct AppState {
    // ... existant ...
    pub changelog_store: Arc<ChangelogStore>,  // NOUVEAU
}
```

Initialisation au même endroit que l'InterceptorPipeline, dans le même data_dir.

### 2.6 Fichiers créés/modifiés

| Fichier | Action | Lignes estimées |
|---------|--------|-----------------|
| `src-tauri/src/time_travel/capture.rs` | Créer | ~150 |
| `src-tauri/src/commands/mutation.rs` | Modifier | +80 lignes |
| `src-tauri/src/lib.rs` | Modifier | +15 lignes |

### 2.7 Checklist de vérification

- [ ] `insert_row` capture l'after-image
- [ ] `update_row` capture before + after + changed_columns
- [ ] `delete_row` capture le before-image
- [ ] Le fetch before-image est best-effort (échec silencieux si row introuvable)
- [ ] La capture ne bloque PAS la mutation si le changelog_store fail (fire-and-forget logging)
- [ ] Les queries SQL brutes via `execute_query` ne tentent pas de capture row-level
- [ ] Le changelog_store est gated derrière `#[cfg(feature = "pro")]`
- [ ] Le ChangelogEntry inclut le audit_entry_id pour corrélation

---

## Phase 3 — Commandes Tauri Time-Travel

> **Objectif** : Exposer l'API backend au frontend via des commandes Tauri.

### 3.1 Commandes (`src-tauri/src/commands/time_travel.rs`)

```rust
// SPDX-License-Identifier: BUSL-1.1

// --- Timeline ---

/// Retourne la timeline agrégée des mutations pour une table.
#[tauri::command]
pub async fn get_table_timeline(
    state: State<'_, SharedState>,
    session_id: String,
    namespace: Namespace,
    table_name: String,
    from_timestamp: Option<String>,  // ISO 8601
    to_timestamp: Option<String>,
    limit: Option<usize>,            // default 100
    offset: Option<usize>,
) -> Result<TimelineResponse, String>

/// Retourne l'historique complet d'une row spécifique.
#[tauri::command]
pub async fn get_row_history(
    state: State<'_, SharedState>,
    namespace: Namespace,
    table_name: String,
    primary_key: HashMap<String, serde_json::Value>,
    limit: Option<usize>,
) -> Result<RowHistoryResponse, String>

// --- Diff temporel ---

/// Calcule le diff entre deux points dans le temps pour une table.
#[tauri::command]
pub async fn compute_temporal_diff(
    state: State<'_, SharedState>,
    namespace: Namespace,
    table_name: String,
    timestamp_from: String,  // ISO 8601
    timestamp_to: String,
    limit: Option<usize>,    // max rows dans le diff
) -> Result<TemporalDiffResponse, String>

/// Reconstruit l'état d'une row à un instant T.
#[tauri::command]
pub async fn get_row_state_at(
    state: State<'_, SharedState>,
    namespace: Namespace,
    table_name: String,
    primary_key: HashMap<String, serde_json::Value>,
    timestamp: String,
) -> Result<RowStateResponse, String>

// --- Rollback ---

/// Génère le SQL de rollback pour restaurer l'état à un point donné.
#[tauri::command]
pub async fn generate_rollback_sql(
    state: State<'_, SharedState>,
    namespace: Namespace,
    table_name: String,
    target_timestamp: String,
    entry_ids: Option<Vec<String>>,  // si fourni, rollback seulement ces entries
) -> Result<RollbackSqlResponse, String>

/// Génère le SQL de rollback pour UNE seule entry.
#[tauri::command]
pub async fn generate_entry_rollback_sql(
    state: State<'_, SharedState>,
    entry_id: String,
) -> Result<RollbackSqlResponse, String>

// --- Config ---

/// Retourne la config du time-travel.
#[tauri::command]
pub async fn get_time_travel_config(
    state: State<'_, SharedState>,
) -> Result<TimeTravelConfigResponse, String>

/// Met à jour la config.
#[tauri::command]
pub async fn update_time_travel_config(
    state: State<'_, SharedState>,
    config: TimeTravelConfig,
) -> Result<TimeTravelConfigResponse, String>

// --- Maintenance ---

/// Purge le changelog d'une table.
#[tauri::command]
pub async fn clear_table_changelog(
    state: State<'_, SharedState>,
    namespace: Namespace,
    table_name: String,
) -> Result<GenericResponse, String>

/// Purge tout le changelog.
#[tauri::command]
pub async fn clear_all_changelog(
    state: State<'_, SharedState>,
) -> Result<GenericResponse, String>

/// Export du changelog filtré.
#[tauri::command]
pub async fn export_changelog(
    state: State<'_, SharedState>,
    filter: ChangelogFilter,
) -> Result<ExportResponse, String>
```

### 3.2 Response Types

```rust
#[derive(Serialize)]
pub struct TimelineResponse {
    pub success: bool,
    pub events: Vec<TimelineEvent>,
    pub total_count: usize,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct RowHistoryResponse {
    pub success: bool,
    pub entries: Vec<ChangelogEntry>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct TemporalDiffResponse {
    pub success: bool,
    pub diff: Option<TemporalDiff>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct RowStateResponse {
    pub success: bool,
    pub state: Option<HashMap<String, serde_json::Value>>,
    pub exists: bool,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct RollbackSqlResponse {
    pub success: bool,
    pub sql: Option<String>,
    pub statements_count: usize,
    pub warnings: Vec<String>,
    pub error: Option<String>,
}

#[derive(Serialize)]
pub struct TimeTravelConfigResponse {
    pub success: bool,
    pub config: TimeTravelConfig,
    pub error: Option<String>,
}
```

### 3.3 Registration dans `lib.rs`

Ajouter dans `generate_handler![]` :
```rust
commands::time_travel::get_table_timeline,
commands::time_travel::get_row_history,
commands::time_travel::compute_temporal_diff,
commands::time_travel::get_row_state_at,
commands::time_travel::generate_rollback_sql,
commands::time_travel::generate_entry_rollback_sql,
commands::time_travel::get_time_travel_config,
commands::time_travel::update_time_travel_config,
commands::time_travel::clear_table_changelog,
commands::time_travel::clear_all_changelog,
commands::time_travel::export_changelog,
```

### 3.4 Frontend bindings (`src/lib/tauri.ts`)

Ajouter les wrappers TypeScript pour chaque commande, avec types miroir des structs Rust.

### 3.5 Fichiers créés/modifiés

| Fichier | Action | Lignes estimées |
|---------|--------|-----------------|
| `src-tauri/src/commands/time_travel.rs` | Créer | ~400 |
| `src-tauri/src/commands/mod.rs` | Modifier | +1 ligne |
| `src-tauri/src/lib.rs` | Modifier | +12 lignes |
| `src/lib/tauri.ts` | Modifier | +120 lignes |

### 3.6 Checklist de vérification

- [ ] Toutes les commandes retournent `Result<T, String>`
- [ ] Le pro gating est appliqué : `#[cfg(not(feature = "pro"))]` stubs
- [ ] Les timestamps sont validés (parsing ISO 8601)
- [ ] Les limites par défaut sont raisonnables (100 events, 50 diff rows)
- [ ] Les commandes sont enregistrées dans `generate_handler!`
- [ ] Les bindings TypeScript sont typés et correspondent aux types Rust

---

## Phase 4 — Frontend : Timeline visuelle

> **Objectif** : Créer l'interface principale de la feature — une timeline interactive des mutations par table.

### 4.1 Nouveau type de tab

**Fichier** : `src/lib/tabs.ts`

```typescript
export type TabType =
  | 'query' | 'table' | 'database' | 'diff'
  | 'federation' | 'snapshots' | 'notebook'
  | 'time-travel';  // NOUVEAU

export interface OpenTab {
  // ... existant ...
  // Time-Travel-specific
  timeTravelNamespace?: Namespace;
  timeTravelTableName?: string;
}

export function createTimeTravelTab(
  namespace: Namespace,
  tableName: string,
): OpenTab {
  return {
    id: generateTabId(),
    type: 'time-travel',
    title: `History: ${tableName}`,
    namespace,
    timeTravelNamespace: namespace,
    timeTravelTableName: tableName,
  };
}
```

### 4.2 Composants

```
src/components/TimeTravel/
├── TimeTravelViewer.tsx        # Composant principal (orchestrateur)
├── TimelineChart.tsx           # Timeline visuelle (chart area/bar)
├── TimelineEventList.tsx       # Liste des événements (table scrollable)
├── TimelineFilters.tsx         # Filtres (date range, operation, session)
├── RowHistoryPanel.tsx         # Panel slide-over : historique d'une row
├── RowHistoryEntry.tsx         # Un entry dans l'historique d'une row
├── TimeTravelToolbar.tsx       # Toolbar (export, clear, settings)
└── hooks/
    ├── useTimeline.ts          # Hook : fetch + state timeline
    ├── useRowHistory.ts        # Hook : fetch historique row
    └── useTemporalDiff.ts      # Hook : fetch diff temporel
```

### 4.3 `TimeTravelViewer.tsx` — Layout principal

```
┌─────────────────────────────────────────────────────────────┐
│ [Toolbar] Export CSV | Export JSON | Clear | Settings    🔒Pro│
├─────────────────────────────────────────────────────────────┤
│ [Filters] Date range: [____] to [____]  Op: [All ▼]        │
│           Session: [All ▼]  Connection: [All ▼]             │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  [TimelineChart]                                             │
│  ┌──────────────────────────────────────────────────────┐   │
│  │  ▊▊    ▊       ▊▊▊▊     ▊   ▊▊         ▊▊▊         │   │
│  │  ──────────────────────────────────────────────────  │   │
│  │  Apr 1    Apr 3     Apr 5      Apr 8     Apr 10     │   │
│  └──────────────────────────────────────────────────────┘   │
│                                                              │
│  [Compare] T1: [●] Apr 5 14:00   T2: [●] Apr 10 09:00      │
│            [Compare these points]                            │
│                                                              │
├─────────────────────────────────────────────────────────────┤
│  [TimelineEventList]                                         │
│  ┌──────┬──────────┬──────┬──────┬───────────┬───────────┐  │
│  │ Time │ Operation│ Rows │ Conn │ PK        │ Actions   │  │
│  ├──────┼──────────┼──────┼──────┼───────────┼───────────┤  │
│  │14:32 │ UPDATE   │  1   │ Prod │ id=42     │ 🔍 ↩️ 📋  │  │
│  │14:30 │ INSERT   │  1   │ Prod │ id=99     │ 🔍 ↩️ 📋  │  │
│  │13:45 │ DELETE   │  1   │ Dev  │ id=7      │ 🔍 ↩️ 📋  │  │
│  └──────┴──────────┴──────┴──────┴───────────┴───────────┘  │
│                          [Load more]                         │
└─────────────────────────────────────────────────────────────┘

Actions: 🔍 = Row history | ↩️ = Rollback SQL | 📋 = Copy entry
```

### 4.4 `TimelineChart.tsx`

- Utiliser un simple chart SVG custom (pas de dépendance chart lourde)
- Axe X : temps (auto-scaled selon la plage de données)
- Axe Y : nombre d'opérations
- Barres empilées par couleur : INSERT (vert), UPDATE (orange), DELETE (rouge)
- Interaction : cliquer sur une barre filtre la liste en dessous
- Points T1/T2 sélectionnables par clic pour le diff temporel
- Brush/zoom sur la plage temporelle

### 4.5 `TimelineEventList.tsx`

- Table virtualisée (réutiliser le pattern de `DiffResultsGrid` avec `@tanstack/react-virtual`)
- Colonnes : Timestamp, Operation (badge coloré), Rows affected, Connection, Primary Key (condensé), Actions
- Tri par timestamp DESC par défaut
- Pagination infinie (load more)
- Cliquer sur une row ouvre le `RowHistoryPanel`

### 4.6 `RowHistoryPanel.tsx`

- Panel slide-over (depuis la droite, comme les sheets dans shadcn)
- Affiche l'historique complet d'une row spécifique
- Chaque entry montre :
  - Timestamp + operation badge
  - Before/After en side-by-side avec highlight des changements (réutiliser les couleurs du Diff)
  - Bouton "Rollback to this point"
  - Bouton "Copy SQL"

### 4.7 Intégration dans le tab system

**`src/AppLayout.tsx`** — Ajouter le rendu conditionnel :

```typescript
if (activeTab?.type === 'time-travel') {
  return (
    <LicenseGate feature="data_time_travel">
      <TimeTravelViewer
        sessionId={sessionId}
        namespace={activeTab.timeTravelNamespace}
        tableName={activeTab.timeTravelTableName}
        connectionName={activeConnection?.name}
      />
    </LicenseGate>
  );
}
```

**`src/components/Tabs/TabBar.tsx`** — Ajouter l'icône :

```typescript
case 'time-travel':
  return <History size={14} />;  // lucide-react
```

### 4.8 Point d'entrée : Context Menu

Ajouter un item dans `TableContextMenu.tsx` :

```typescript
<ContextMenuItem onClick={() => onOpenTimeTravel?.(collection)}>
  <History className="mr-2 h-4 w-4" />
  {t('timeTravel.viewHistory')}
</ContextMenuItem>
```

### 4.9 Fichiers créés/modifiés

| Fichier | Action | Lignes estimées |
|---------|--------|-----------------|
| `src/components/TimeTravel/TimeTravelViewer.tsx` | Créer | ~250 |
| `src/components/TimeTravel/TimelineChart.tsx` | Créer | ~300 |
| `src/components/TimeTravel/TimelineEventList.tsx` | Créer | ~200 |
| `src/components/TimeTravel/TimelineFilters.tsx` | Créer | ~150 |
| `src/components/TimeTravel/RowHistoryPanel.tsx` | Créer | ~250 |
| `src/components/TimeTravel/RowHistoryEntry.tsx` | Créer | ~120 |
| `src/components/TimeTravel/TimeTravelToolbar.tsx` | Créer | ~100 |
| `src/components/TimeTravel/hooks/useTimeline.ts` | Créer | ~80 |
| `src/components/TimeTravel/hooks/useRowHistory.ts` | Créer | ~60 |
| `src/components/TimeTravel/hooks/useTemporalDiff.ts` | Créer | ~60 |
| `src/lib/tabs.ts` | Modifier | +20 |
| `src/AppLayout.tsx` | Modifier | +15 |
| `src/components/Tabs/TabBar.tsx` | Modifier | +3 |
| `src/components/Tree/TableContextMenu.tsx` | Modifier | +10 |

### 4.10 Checklist de vérification

- [ ] Le tab `time-travel` s'ouvre depuis le context menu d'une table
- [ ] La timeline se charge et affiche les événements récents
- [ ] Les filtres par date/operation/session fonctionnent
- [ ] Le chart est interactif (clic filtre la liste)
- [ ] Le RowHistoryPanel s'ouvre avec le before/after correctement affiché
- [ ] La virtualisation fonctionne pour > 1000 événements
- [ ] Le tab est gated derrière LicenseGate
- [ ] L'icône History apparaît dans la TabBar

---

## Phase 5 — Frontend : Diff temporel

> **Objectif** : Permettre de comparer l'état d'une table entre deux points dans le temps.

### 5.1 Sélection des points T1/T2

Dans le `TimeTravelViewer`, ajouter un mode "Compare" :
- Deux date-time pickers pour T1 et T2
- OU : cliquer sur deux points dans le TimelineChart
- OU : sélectionner deux events dans la liste
- Bouton "Compare these points" lance le diff

### 5.2 `TemporalDiffViewer.tsx`

Réutiliser au maximum les composants Diff existants :

```typescript
// src/components/TimeTravel/TemporalDiffViewer.tsx

import { DiffResultsGrid } from '@/components/Diff/DiffResultsGrid';
import { DiffStatsBar } from '@/components/Diff/DiffStatsBar';

// Convertir TemporalDiff (backend) → DiffResult (format existant)
function temporalDiffToDiffResult(temporal: TemporalDiff): DiffResult {
  return {
    columns: temporal.columns.map(c => ({ name: c, data_type: 'text', nullable: true })),
    rows: temporal.rows.map(row => ({
      status: row.status,
      leftCells: /* state_at_t1 */,
      rightCells: /* state_at_t2 */,
      rowKey: JSON.stringify(row.primary_key),
    })),
    stats: {
      unchanged: 0,
      added: temporal.stats.added,
      removed: temporal.stats.removed,
      modified: temporal.stats.modified,
      total: temporal.stats.total_changes,
    },
  };
}
```

### 5.3 Layout du diff

```
┌─────────────────────────────────────────────────────────────┐
│ Comparing: users @ Apr 5 14:00 → Apr 10 09:00              │
├─────────────────────────────────────────────────────────────┤
│ [DiffStatsBar] Added: 3 | Modified: 7 | Removed: 1 | [All]│
├─────────────────────────────────────────────────────────────┤
│ [DiffResultsGrid]                                            │
│  ┌─────────────────────────┬─────────────────────────┐      │
│  │  State at T1            │  State at T2            │      │
│  ├─────────────────────────┼─────────────────────────┤      │
│  │  id=42 name="Alice"     │  id=42 name="Bob"  ← hl│      │
│  │  id=7  status="active"  │  (deleted)         ← red│      │
│  │  (not yet)              │  id=99 name="Eve"  ← grn│      │
│  └─────────────────────────┴─────────────────────────┘      │
├─────────────────────────────────────────────────────────────┤
│ [Actions] Generate rollback SQL to T1 | Export diff          │
└─────────────────────────────────────────────────────────────┘
```

### 5.4 Fichiers créés

| Fichier | Lignes estimées |
|---------|-----------------|
| `src/components/TimeTravel/TemporalDiffViewer.tsx` | ~200 |
| `src/components/TimeTravel/TemporalDiffToolbar.tsx` | ~80 |

### 5.5 Checklist de vérification

- [ ] Le diff s'affiche correctement avec les composants Diff existants
- [ ] Les colonnes modifiées sont highlight en jaune
- [ ] Les rows ajoutées en vert, supprimées en rouge
- [ ] Le filtre par status (added/modified/removed) fonctionne
- [ ] L'export CSV/JSON du diff fonctionne
- [ ] Le bouton "Generate rollback SQL" est disponible

---

## Phase 6 — Rollback SQL Generator

> **Objectif** : Générer du SQL pour restaurer l'état à un point précédent dans le temps.

### 6.1 Module Rust (`src-tauri/src/time_travel/rollback.rs`)

```rust
// SPDX-License-Identifier: BUSL-1.1

/// Génère les statements SQL pour rollback un ensemble de changements.
///
/// Logique d'inversion :
///   INSERT → DELETE WHERE pk = values
///   UPDATE → UPDATE SET columns = before_values WHERE pk = values
///   DELETE → INSERT INTO table (columns) VALUES (before_values)
///
/// Les statements sont générés en ordre INVERSE chronologique
/// (le plus récent d'abord) pour respecter les dépendances FK.
pub fn generate_rollback_statements(
    entries: &[ChangelogEntry],
    driver_id: &str,
) -> RollbackResult {
    // Pour chaque entry, en ordre inverse :
    //   match entry.operation {
    //     Insert => "DELETE FROM {table} WHERE {pk_clause}",
    //     Update => "UPDATE {table} SET {before_columns} WHERE {pk_clause}",
    //     Delete => "INSERT INTO {table} ({columns}) VALUES ({before_values})",
    //   }
}
```

### 6.2 Détails de la génération SQL

**Gestion des dialectes** — Utiliser `sqlparser` pour le quoting correct :

| Driver | Quoting | Placeholder |
|--------|---------|-------------|
| PostgreSQL | `"column"` | `$1` ou literal |
| MySQL | `` `column` `` | `?` ou literal |
| SQLite | `"column"` | `?` ou literal |
| SQL Server | `[column]` | `@p1` ou literal |

**Choix** : Générer des statements avec des valeurs littérales (pas de placeholders), car le SQL sera exécuté manuellement par l'utilisateur dans l'éditeur de requêtes. Cela permet la review avant exécution.

**Gestion des types** :
- Strings : échappées avec `'` (doubler les `'` internes)
- Numbers : littéral
- NULL : `NULL`
- Booleans : `TRUE`/`FALSE` (PG) ou `1`/`0` (MySQL)
- Dates : `'2026-04-11T14:30:00Z'` (format ISO)
- JSON : `'{"key": "value"}'::jsonb` (PG) ou `JSON '...'` (MySQL)
- Bytes/Binary : exclure avec un warning

### 6.3 Warnings du rollback

Le générateur produit des warnings dans ces cas :
- Row sans before-image (capture échouée) → skip avec warning
- Type binaire non supporté → skip le champ avec warning
- FK potentiellement violée → warning informatif
- Table modifiée depuis (DDL ALTER détecté) → warning sur les colonnes manquantes/renommées

### 6.4 Preview frontend

**`RollbackPreview.tsx`** — Dialog modale :

```
┌─────────────────────────────────────────────────────┐
│ Rollback SQL Preview                          [X]   │
├─────────────────────────────────────────────────────┤
│ Target: users @ Apr 5 14:00                         │
│ Statements: 11 (3 DELETE, 7 UPDATE, 1 INSERT)       │
│                                                      │
│ ⚠️ 2 warnings:                                      │
│   - Row id=15: no before-image available (skipped)  │
│   - Column "avatar" is BYTEA: excluded from restore │
│                                                      │
│ ┌─────────────────────────────────────────────────┐ │
│ │ -- Rollback generated by QoreDB Time-Travel     │ │
│ │ -- Target: 2026-04-05T14:00:00Z                 │ │
│ │ -- Table: public.users                          │ │
│ │ -- ⚠️ Review carefully before executing!        │ │
│ │                                                  │ │
│ │ BEGIN;                                           │ │
│ │                                                  │ │
│ │ -- Undo INSERT (id=99, 2026-04-10 09:00)        │ │
│ │ DELETE FROM "public"."users"                     │ │
│ │   WHERE "id" = 99;                              │ │
│ │                                                  │ │
│ │ -- Undo UPDATE (id=42, 2026-04-08 15:30)        │ │
│ │ UPDATE "public"."users"                          │ │
│ │   SET "name" = 'Alice', "email" = 'a@b.com'    │ │
│ │   WHERE "id" = 42;                              │ │
│ │                                                  │ │
│ │ COMMIT;                                          │ │
│ └─────────────────────────────────────────────────┘ │
│                                                      │
│ [Copy to clipboard] [Open in Query Tab] [Cancel]    │
└─────────────────────────────────────────────────────┘
```

### 6.5 Actions disponibles

1. **Copy to clipboard** : Copie le SQL
2. **Open in Query Tab** : Ouvre un nouveau tab query avec le SQL pré-rempli (réutilise `createQueryTab(sql, namespace)`)
3. **Execute directly** : NON disponible — on force la review manuelle par sécurité

### 6.6 Fichiers créés/modifiés

| Fichier | Action | Lignes estimées |
|---------|--------|-----------------|
| `src-tauri/src/time_travel/rollback.rs` | Créer | ~350 |
| `src/components/TimeTravel/RollbackPreview.tsx` | Créer | ~200 |

### 6.7 Checklist de vérification

- [ ] Le SQL généré est syntaxiquement correct pour chaque driver
- [ ] Les valeurs sont correctement échappées (pas d'injection SQL)
- [ ] Les statements sont en ordre inverse chronologique
- [ ] L'inversion est correcte : INSERT→DELETE, UPDATE→restore before, DELETE→INSERT
- [ ] Les warnings sont clairs et informatifs
- [ ] Le SQL est wrappé dans BEGIN/COMMIT (sauf SQLite qui utilise des savepoints)
- [ ] Le bouton "Open in Query Tab" fonctionne

---

## Phase 7 — Filtres avancés (user/session/plage)

> **Objectif** : Permettre le filtrage fin des événements.

### 7.1 Filtres disponibles

| Filtre | Type | Source |
|--------|------|--------|
| **Plage temporelle** | Date range picker | `from_timestamp` / `to_timestamp` |
| **Type d'opération** | Multi-select | INSERT / UPDATE / DELETE |
| **Connexion** | Select | `connection_name` (from changelog entries) |
| **Session** | Select | `session_id` (from changelog entries) |
| **Environnement** | Select | development / staging / production |
| **Primary Key** | Text input | Recherche par valeur de PK |

### 7.2 `TimelineFilters.tsx`

- Date range : deux `<Input type="datetime-local" />` avec composants shadcn
- Operation : `<ToggleGroup>` avec badges colorés (INSERT vert, UPDATE orange, DELETE rouge)
- Connexion/Session/Environment : `<Select>` avec options dynamiques extraites des entries
- PK search : `<Input>` avec debounce 300ms

### 7.3 Persistance des filtres

Les filtres sont locaux au tab (pas persistés entre sessions). Stockés dans le state du hook `useTimeline`.

---

## Phase 8 — Settings, i18n, License gating

### 8.1 Settings

Ajouter une sous-section "Time-Travel" dans la catégorie "Data" des settings.

**Toggle principal** : Le switch on/off est le premier élément, bien visible. Quand désactivé :
- Aucun `SELECT` before-image n'est exécuté (zero overhead)
- Le `ChangelogStore` ignore les appels `record()`
- Le menu "View data history" reste visible mais le tab affiche "Time-Travel is disabled" avec un lien vers les settings

**Warning** : Un texte d'information sous le toggle explique ce qui est stocké :
> "When enabled, QoreDB records the before/after values of every row you edit, update, or delete through the data grid. This data is stored locally on your machine. Adjust retention to match your data policy."

**Settings détaillés** :

```typescript
// Dans settingsConfig.ts, section 'data'
{
  id: 'time-travel',
  label: t('settings.timeTravel.title'),
  settings: [
    // Toggle principal — désactive tout (capture + UI)
    { key: 'timeTravelEnabled', type: 'toggle', default: true,
      label: t('settings.timeTravel.enabled'),
      description: t('settings.timeTravel.enabledDescription') },
    // Warning informatif sur les données stockées
    { type: 'info', text: t('settings.timeTravel.dataWarning') },
    // Rétention automatique
    { key: 'timeTravelRetentionDays', type: 'number', default: 30, min: 0, max: 365,
      label: t('settings.timeTravel.retentionDays'),
      description: t('settings.timeTravel.retentionDaysDescription') },
    // Limite d'entries avant rotation
    { key: 'timeTravelMaxEntries', type: 'number', default: 50000, min: 1000, max: 500000,
      label: t('settings.timeTravel.maxEntries'),
      description: t('settings.timeTravel.maxEntriesDescription') },
    // Restreindre aux envs production
    { key: 'timeTravelProductionOnly', type: 'toggle', default: false,
      label: t('settings.timeTravel.productionOnly'),
      description: t('settings.timeTravel.productionOnlyDescription') },
    // Tables exclues (noms exacts, séparés par virgule)
    { key: 'timeTravelExcludedTables', type: 'text', placeholder: 'migrations,sessions,schema_history',
      label: t('settings.timeTravel.excludedTables'),
      description: t('settings.timeTravel.excludedTablesDescription') },
  ]
}
```

**Flow du toggle** :
1. L'utilisateur toggle off dans les settings
2. Le frontend appelle `update_time_travel_config({ enabled: false, ... })`
3. Le backend persiste dans `time-travel.json`
4. `ChangelogStore.record()` vérifie `config.enabled` en premier — early return si `false`
5. `mutation.rs` vérifie `config.enabled` AVANT le fetch before-image — skip le SELECT si désactivé
6. Le changelog existant n'est PAS supprimé (l'utilisateur peut le purger manuellement via "Clear history")

### 8.2 i18n — Clés à ajouter

Ajouter dans les 9 fichiers de locale (`en.json`, `fr.json`, etc.) :

```json
{
  "timeTravel": {
    "viewHistory": "View data history",
    "title": "Data Time-Travel",
    "timeline": {
      "title": "Timeline",
      "noEvents": "No mutations recorded for this table",
      "loadMore": "Load more events",
      "event": {
        "insert": "Inserted",
        "update": "Updated",
        "delete": "Deleted"
      },
      "rows": "{{count}} row",
      "rows_plural": "{{count}} rows"
    },
    "filters": {
      "dateRange": "Date range",
      "from": "From",
      "to": "To",
      "operation": "Operation",
      "connection": "Connection",
      "session": "Session",
      "environment": "Environment",
      "allOperations": "All operations",
      "allConnections": "All connections",
      "allSessions": "All sessions",
      "searchPk": "Search by primary key..."
    },
    "diff": {
      "title": "Temporal diff",
      "selectPoints": "Select two points to compare",
      "stateAtT1": "State at {{time}}",
      "stateAtT2": "State at {{time}}",
      "compare": "Compare these points",
      "noChanges": "No changes between these points"
    },
    "rowHistory": {
      "title": "Row history",
      "noHistory": "No history for this row",
      "before": "Before",
      "after": "After",
      "changedColumns": "Changed columns"
    },
    "rollback": {
      "title": "Rollback SQL Preview",
      "target": "Target: {{table}} @ {{time}}",
      "statements": "{{count}} statement",
      "statements_plural": "{{count}} statements",
      "warnings": "{{count}} warning",
      "warnings_plural": "{{count}} warnings",
      "reviewWarning": "Review carefully before executing!",
      "copyToClipboard": "Copy to clipboard",
      "openInQueryTab": "Open in Query Tab",
      "noBeforeImage": "Row {{pk}}: no before-image available (skipped)",
      "binaryExcluded": "Column \"{{column}}\" is binary: excluded from restore",
      "generateRollback": "Generate rollback SQL",
      "rollbackToPoint": "Rollback to this point"
    },
    "toolbar": {
      "export": "Export changelog",
      "clear": "Clear history",
      "clearConfirm": "Are you sure? This will permanently delete all recorded changes for this table.",
      "settings": "Time-Travel settings"
    },
    "settings": {
      "title": "Time-Travel",
      "enabled": "Enable change tracking",
      "enabledDescription": "Record before/after state for mutations made through QoreDB's data grid.",
      "dataWarning": "When enabled, QoreDB records the before/after values of every row you edit, update, or delete through the data grid. This data is stored locally on your machine. Adjust retention to match your data policy.",
      "disabled": "Time-Travel is disabled",
      "disabledDescription": "Enable it in Settings > Data > Time-Travel to start tracking changes.",
      "retentionDays": "Retention period (days)",
      "retentionDaysDescription": "How long to keep change history. Set 0 for unlimited.",
      "maxEntries": "Maximum entries",
      "maxEntriesDescription": "Maximum number of change records before rotation.",
      "productionOnly": "Production only",
      "productionOnlyDescription": "Only track changes in production environments.",
      "excludedTables": "Excluded tables",
      "excludedTablesDescription": "Comma-separated table names to exclude from tracking (exact match)."
    },
    "empty": {
      "title": "No change history",
      "description": "Changes made through QoreDB's data grid will appear here. Start editing data to see the timeline.",
      "hint": "Only mutations via QoreDB's UI are tracked (INSERT, UPDATE, DELETE through the data grid)."
    }
  }
}
```

### 8.3 License gating

**Frontend** :

1. Ajouter `'data_time_travel'` dans `ProFeature` (`src/lib/license.ts`)
2. Ajouter le mapping : `data_time_travel: 'pro'` dans `FEATURE_REQUIRED_TIER`
3. Wrapper le composant dans `<LicenseGate feature="data_time_travel">`
4. Ajouter le label dans les locales : `license.features.data_time_travel`

**Backend** :

1. Gater les commandes avec `#[cfg(feature = "pro")]`
2. Les stubs non-pro retournent `"Data Time-Travel requires a Pro license."`
3. Le `ChangelogStore` ne capture rien si la feature pro n'est pas compilée

### 8.4 Fichiers modifiés

| Fichier | Changement |
|---------|------------|
| `src/lib/license.ts` | +2 lignes (ProFeature + tier mapping) |
| `src/components/Settings/settingsConfig.ts` | +15 lignes |
| `src/locales/en.json` | +80 clés |
| `src/locales/fr.json` | +80 clés (traduction FR) |
| `src/locales/es.json` | +80 clés |
| `src/locales/de.json` | +80 clés |
| `src/locales/pt-BR.json` | +80 clés |
| `src/locales/zh-CN.json` | +80 clés |
| `src/locales/ja.json` | +80 clés |
| `src/locales/ko.json` | +80 clés |
| `src/locales/ru.json` | +80 clés |

---

## Phase 9 — Performance & limites

### 9.1 Coût du before-image fetch

Le fetch `SELECT * WHERE pk = ?` avant chaque UPDATE/DELETE ajoute une query. Mitigations :

| Stratégie | Impact |
|-----------|--------|
| **Best-effort** | Si le fetch échoue (timeout, row locked), on log sans before-image plutôt que de bloquer |
| **Timeout court** | 2s timeout sur le before-image fetch |
| **Skip si disabled** | Si `time_travel.enabled = false` dans les settings, aucun fetch, zero overhead |
| **Import CSV exempt** | L'import CSV ne passe pas par `insert_row` — aucun impact (cf. ADR-2) |
| **Async logging** | L'écriture dans le changelog est fire-and-forget (tokio::spawn) |
| **Check avant fetch** | `mutation.rs` vérifie `config.enabled` + `excluded_tables` AVANT le SELECT, pas après |

### 9.2 Taille du changelog

| Scénario | 100 mutations/jour | 1000/jour | 10000/jour |
|----------|-------------------|-----------|------------|
| Taille entry moyenne | ~500 bytes | ~500 bytes | ~500 bytes |
| Par jour | 50 KB | 500 KB | 5 MB |
| Rétention 30j | 1.5 MB | 15 MB | 150 MB |

Avec le défaut `max_entries: 50000` et `retention_days: 30`, la taille restera sous contrôle.

### 9.3 Index en mémoire

L'index `HashMap<String, Vec<usize>>` (table → entry indices) est reconstruit au démarrage en scannant le cache in-memory. Pour 5000 entries, c'est instantané (<1ms).

Pour les queries sur le fichier complet (hors cache), on scanne le JSONL séquentiellement avec filtre early-exit. C'est acceptable pour un outil desktop.

### 9.4 Limites explicites

| Paramètre | Valeur par défaut | Configurable |
|-----------|-------------------|-------------|
| Cache in-memory | 5000 entries | Non (hardcodé) |
| Max entries fichier | 50 000 | Oui (settings) |
| Max file size | 500 MB | Oui (settings) |
| Retention | 30 jours | Oui (settings) |
| Before-image timeout | 2 secondes | Non |
| Diff max rows | 10 000 | Oui (param commande) |
| Timeline page size | 100 events | Oui (param commande) |

---

## Phase 10 — Tests

### 10.1 Tests Rust (unitaires)

| Test | Fichier | Description |
|------|---------|-------------|
| `test_changelog_entry_creation` | `time_travel/types.rs` | Création et sérialisation |
| `test_changed_columns_detection` | `time_travel/capture.rs` | Détection des colonnes modifiées |
| `test_store_record_and_retrieve` | `time_travel/store.rs` | Écriture + lecture |
| `test_store_filter_by_table` | `time_travel/store.rs` | Filtre par table |
| `test_store_filter_by_timestamp` | `time_travel/store.rs` | Filtre par plage temporelle |
| `test_store_rotation` | `time_travel/store.rs` | Rotation quand max atteint |
| `test_store_retention_purge` | `time_travel/store.rs` | Purge des entries expirées |
| `test_timeline_aggregation` | `time_travel/store.rs` | Agrégation en TimelineEvents |
| `test_temporal_diff` | `time_travel/store.rs` | Diff entre deux timestamps |
| `test_row_state_reconstruction` | `time_travel/store.rs` | Reconstruction d'état à T |
| `test_rollback_insert_generates_delete` | `time_travel/rollback.rs` | INSERT → DELETE |
| `test_rollback_update_restores_before` | `time_travel/rollback.rs` | UPDATE → UPDATE before |
| `test_rollback_delete_generates_insert` | `time_travel/rollback.rs` | DELETE → INSERT |
| `test_rollback_order_is_reverse_chrono` | `time_travel/rollback.rs` | Ordre inverse |
| `test_rollback_sql_escaping` | `time_travel/rollback.rs` | Échappement correct |
| `test_rollback_dialect_quoting` | `time_travel/rollback.rs` | Quoting par dialecte |
| `test_rollback_warnings` | `time_travel/rollback.rs` | Warnings appropriés |
| `test_config_persistence` | `time_travel/store.rs` | Save/load config |

### 10.2 Tests d'intégration

| Test | Description |
|------|-------------|
| `test_mutation_captures_changelog` | Vérifie que insert_row/update_row/delete_row créent des ChangelogEntry |
| `test_update_captures_before_after` | Vérifie les before/after images sur une vraie DB |
| `test_full_timeline_flow` | Insert → Update → Delete → vérifie la timeline |
| `test_rollback_sql_execution` | Génère le rollback SQL et vérifie qu'il restaure l'état |

### 10.3 Tests Frontend (si test framework existant)

| Test | Description |
|------|-------------|
| `TimeTravelViewer renders empty state` | État vide avec message d'aide |
| `TimelineChart renders events` | Chart avec des données mockées |
| `TemporalDiff conversion` | Conversion TemporalDiff → DiffResult |
| `RollbackPreview shows SQL` | Affichage correct du SQL et warnings |

---

## Phase 11 — Documentation

### 11.1 Fichiers à mettre à jour

| Fichier | Changement |
|---------|------------|
| `doc/todo/v3.md` | Cocher les items implémentés |
| `doc/rules/FEATURES.md` | Ajouter la section Data Time-Travel |
| `doc/FEATURES.csv` | Ajouter la ligne |
| `README.md` | Mentionner la feature dans les highlights |

### 11.2 Documentation interne

Créer `doc/internals/DATA_TIME_TRAVEL.md` avec :
- Architecture du ChangelogStore
- Format de stockage (JSONL schema)
- Flow de capture (séquence insert/update/delete)
- Limitations connues (scope QoreDB only, pas de SQL brut row-level)
- Configuration et tuning

---

## Decisions d'architecture

### ADR-1 : Pas de `excluded_columns` en V1

**Decision** : Pas de filtrage de colonnes sensibles par pattern glob.

**Contexte** : On a envisagé un mécanisme `excluded_columns: ["*password*", "*token*"]` pour éviter de stocker des données sensibles dans le changelog.

**Pourquoi non** :
- Les colonnes sensibles ne suivent pas de convention de nommage universelle (`ssn`, `salary`, `dob`, `card_number`...). Un glob donne une **fausse impression de sécurité**.
- Les faux positifs (`password_reset_count`, `token_count`) frustrent l'utilisateur.
- Le fichier changelog vit dans `~/.local/share/com.qoredb.app/` — **même périmètre de sécurité** que le vault Argon2 et les credentials de connexion. Quiconque a accès au JSONL a déjà accès à la DB elle-même.

**Ce qu'on fait à la place** :
- Toggle on/off global dans les settings (Phase 8)
- Rétention configurable (30j défaut) — les données sont purgées automatiquement
- `excluded_tables` pour exclure des tables entières (scope clair, pas d'ambiguïté)
- Warning explicite dans l'UI settings : "Le changelog contient les valeurs réelles des données modifiées via le DataGrid. La rétention est configurable."
- Si un client enterprise demande le filtrage par colonne, on l'ajoutera en V2 avec un vrai mécanisme de tagging côté schema (pas du glob fragile)

### ADR-2 : Pas de `batch_threshold` — ciblage par flow

**Decision** : Pas de seuil automatique qui coupe la capture au-delà de N rows.

**Contexte** : On a envisagé un `batch_threshold: 50` au-delà duquel la capture before/after serait désactivée pour les opérations en masse.

**Pourquoi non** :
- Un seuil crée un **cliff UX** : 49 rows = tracking complet, 51 = rien. Imprévisible.
- Les mutations DataGrid passent par `insert_row`/`update_row`/`delete_row` **une row à la fois** (N appels séquentiels). L'overhead est N SELECTs, mais l'utilisateur attend déjà N mutations séquentielles.

**Ce qu'on fait à la place** :
- **DataGrid** : toujours capturer. Le +60% par mutation unitaire est absorbé car l'utilisateur est déjà dans un flow interactif row-by-row.
- **Import CSV** (`commands/import.rs`) : **skip le time-travel**. L'import a son propre bulk path, ne passe pas par `insert_row`, et l'utilisateur a le fichier CSV comme "état source". Un import de 10K rows ne génère pas 10K SELECTs.
- **Queries SQL brutes** (`execute_query`) : déjà prévu — pas de capture row-level, seulement l'audit trail existant.

---

## Risques et mitigations

| Risque | Impact | Probabilité | Mitigation |
|--------|--------|-------------|------------|
| **Before-image fetch timeout** | Pas de before data pour l'entry | Moyenne | Best-effort, timeout 2s, entry créée sans before |
| **Table sans PK** | Impossible d'identifier la row | Haute | Skip le time-travel pour les tables sans PK, warning dans les settings |
| **Changelog trop gros** | Espace disque, lenteur | Faible | Rotation auto, retention, max_file_size |
| **Schema change entre capture et rollback** | SQL de rollback invalide | Moyenne | Warning si colonnes manquantes, skip les colonnes inexistantes |
| **Concurrency sur le fichier JSONL** | Corruption | Très faible | RwLock + append-only + atomic rotation |
| **Données sensibles dans le changelog** | Fuite locale | Moyenne | Même périmètre sécu que le vault. Toggle on/off + rétention 30j + warning UI (voir ADR-1) |
| **Performance sur mutations fréquentes** | Ralentissement | Faible | Fire-and-forget async write, skip import CSV (voir ADR-2) |

---

## Fichiers créés/modifiés (récapitulatif)

### Fichiers créés (Rust backend) — 15 fichiers

| Fichier | SPDX | Lignes |
|---------|------|--------|
| `src-tauri/src/time_travel/mod.rs` | BUSL-1.1 | ~10 |
| `src-tauri/src/time_travel/types.rs` | BUSL-1.1 | ~180 |
| `src-tauri/src/time_travel/store.rs` | BUSL-1.1 | ~500 |
| `src-tauri/src/time_travel/capture.rs` | BUSL-1.1 | ~150 |
| `src-tauri/src/time_travel/rollback.rs` | BUSL-1.1 | ~350 |
| `src-tauri/src/commands/time_travel.rs` | BUSL-1.1 | ~400 |

### Fichiers créés (Frontend) — 12 fichiers

| Fichier | SPDX | Lignes |
|---------|------|--------|
| `src/components/TimeTravel/TimeTravelViewer.tsx` | BUSL-1.1 | ~250 |
| `src/components/TimeTravel/TimelineChart.tsx` | BUSL-1.1 | ~300 |
| `src/components/TimeTravel/TimelineEventList.tsx` | BUSL-1.1 | ~200 |
| `src/components/TimeTravel/TimelineFilters.tsx` | BUSL-1.1 | ~150 |
| `src/components/TimeTravel/RowHistoryPanel.tsx` | BUSL-1.1 | ~250 |
| `src/components/TimeTravel/RowHistoryEntry.tsx` | BUSL-1.1 | ~120 |
| `src/components/TimeTravel/TimeTravelToolbar.tsx` | BUSL-1.1 | ~100 |
| `src/components/TimeTravel/TemporalDiffViewer.tsx` | BUSL-1.1 | ~200 |
| `src/components/TimeTravel/TemporalDiffToolbar.tsx` | BUSL-1.1 | ~80 |
| `src/components/TimeTravel/RollbackPreview.tsx` | BUSL-1.1 | ~200 |
| `src/components/TimeTravel/hooks/useTimeline.ts` | BUSL-1.1 | ~80 |
| `src/components/TimeTravel/hooks/useRowHistory.ts` | BUSL-1.1 | ~60 |
| `src/components/TimeTravel/hooks/useTemporalDiff.ts` | BUSL-1.1 | ~60 |

### Fichiers modifiés

| Fichier | Changement |
|---------|------------|
| `src-tauri/src/lib.rs` | +ChangelogStore dans AppState, +commands registration |
| `src-tauri/src/commands/mod.rs` | +`pub mod time_travel;` |
| `src-tauri/src/commands/mutation.rs` | +capture before/after à chaque mutation |
| `src/lib/tabs.ts` | +type `'time-travel'`, +factory function, +OpenTab fields |
| `src/lib/tauri.ts` | +bindings TypeScript pour les commandes time-travel |
| `src/lib/license.ts` | +`'data_time_travel'` dans ProFeature |
| `src/AppLayout.tsx` | +rendu conditionnel du tab time-travel |
| `src/components/Tabs/TabBar.tsx` | +icône History |
| `src/components/Tree/TableContextMenu.tsx` | +item "View data history" |
| `src/components/Settings/settingsConfig.ts` | +section time-travel |
| `src/locales/*.json` (×9) | +clés timeTravel.* |

### Documentation

| Fichier | Action |
|---------|--------|
| `doc/internals/DATA_TIME_TRAVEL.md` | Créer |
| `doc/todo/v3.md` | Cocher les items |
| `doc/rules/FEATURES.md` | Ajouter section |

---

## Ordre d'exécution recommandé

```
Phase 1 (Changelog Store)     ████████░░░░░░░░░░░░░░░  Backend foundation
Phase 2 (Capture mutations)   ░░░░████████░░░░░░░░░░░  Core data flow
Phase 3 (Commandes Tauri)     ░░░░░░░░████████░░░░░░░  API layer
Phase 8 (License + i18n)      ░░░░░░░░░░░░████░░░░░░░  Infrastructure
Phase 4 (Timeline UI)         ░░░░░░░░░░░░░░████████░  Main UI
Phase 5 (Diff temporel)       ░░░░░░░░░░░░░░░░░░████░  Diff UI
Phase 6 (Rollback generator)  ░░░░░░░░░░░░░░░░░░░░██░  Rollback
Phase 7 (Filtres avancés)     ░░░░░░░░░░░░░░░░░░░░░█░  Polish
Phase 9 (Performance)         ░░░░░░░░░░░░░░░░░░░░░█░  Optimization
Phase 10 (Tests)              ░░░░░░░░░░░░████████████  Continu
Phase 11 (Documentation)      ░░░░░░░░░░░░░░░░░░░░░██  Final
```

**Estimation totale** : ~4200 lignes de code (Rust + TypeScript)
