# Database Notebooks — Spec complète

> **Statut** : Draft v1
> **Feature** : Killer Feature #6
> **Effort estimé** : 4-6 semaines (1 dev frontend + backend)

---

## 1. Vision

Un **document exécutable** qui mélange cellules SQL/NoSQL, Markdown et visualisations, connecté à une base de données live. Le notebook est le chaînon manquant entre le query editor jetable et la documentation formelle.

**Analogie** : Jupyter Notebook, mais natif, local, zéro config, pensé pour les bases de données.

**Positionnement** : Aucun client DB desktop ne propose ça. DataGrip a des "consoles" (du texte plat). DBeaver a des "SQL scripts" (séquentiel, pas documenté). TablePlus n'a rien. Les Jupyter notebooks SQL existent mais nécessitent Python + un kernel + de la config.

---

## 2. Cas d'usage concrets

### Investigation d'incident (le cas le plus fréquent)
> "Vendredi soir, le paiement du client #4521 a échoué. Je retrace tout dans un notebook."

```
[markdown] ## Incident: Paiement échoué - Client #4521
[markdown] Signalé le 2026-02-14 à 23:12. Erreur Stripe timeout.
[sql]      SELECT * FROM payments WHERE customer_id = 4521 ORDER BY created_at DESC LIMIT 5;
           → résultat inline (3 lignes, la dernière en status='failed')
[markdown] Le paiement #8832 a échoué. Vérifions les logs associés :
[sql]      SELECT * FROM payment_logs WHERE payment_id = 8832 ORDER BY ts;
           → résultat inline (timeline des events)
[markdown] **Root cause** : timeout Stripe après 30s, pas de retry configuré.
[markdown] **Action** : ajout retry avec backoff exponentiel. PR #234.
```

Le notebook est ensuite **partageable** avec l'équipe et **ré-exécutable** pour vérifier que le fix fonctionne.

### Onboarding développeur
> "Le nouveau dev doit comprendre notre schéma de facturation."

Le notebook guide à travers les tables clés, montre des exemples de données réelles, et documente les cas limites — le tout exécutable et toujours à jour.

### Reporting / audit récurrent
> "Chaque lundi, je vérifie les métriques de la semaine."

Un notebook avec des queries paramétrées (`$week_start`) qu'on ré-exécute en un clic.

### Documentation vivante de queries complexes
> "Cette query de 40 lignes calcule le MRR. Personne ne comprend comment."

Le notebook découpe la query en étapes avec des explications entre chaque cellule.

---

## 3. Modèle de données

### 3.1 Format fichier : `.qnb` (QoreDB Notebook)

```typescript
interface QoreNotebook {
  version: 1;
  metadata: NotebookMetadata;
  cells: NotebookCell[];
  variables: Record<string, NotebookVariable>;  // paramètres globaux
}

interface NotebookMetadata {
  id: string;                    // uuid
  title: string;
  description?: string;
  createdAt: string;             // ISO 8601
  updatedAt: string;
  author?: string;
  tags?: string[];
  connectionHint?: {             // suggestion de connexion (non obligatoire)
    driver: DriverType;
    database?: string;
    label?: string;              // nom de la connexion sauvegardée
  };
}

interface NotebookCell {
  id: string;                    // uuid, stable (pour références inter-cellules)
  type: 'sql' | 'mongo' | 'markdown' | 'chart';
  source: string;                // contenu brut de la cellule
  // Résultat (optionnel, sérialisé au save pour "snapshot" des résultats)
  lastResult?: CellResult | null;
  // Métadonnées d'exécution
  executionState?: 'idle' | 'running' | 'success' | 'error';
  executionCount?: number;       // combien de fois exécutée
  executedAt?: string;           // dernière exécution
  executionTimeMs?: number;
  // Config optionnelle par cellule
  config?: CellConfig;
}

interface CellConfig {
  namespace?: Namespace;          // override le namespace du notebook
  maxRows?: number;               // limite d'affichage (défaut: 500)
  collapsed?: boolean;            // résultat replié
  pinned?: boolean;               // cellule épinglée (toujours visible)
  label?: string;                 // nom optionnel (pour référence: $cell.label)
  hideSource?: boolean;           // masquer le code en mode "présentation"
}

interface CellResult {
  type: 'table' | 'document' | 'message' | 'error';
  // Pour type='table'
  columns?: ColumnInfo[];
  rows?: Row[];
  totalRows?: number;
  affectedRows?: number;
  // Pour type='document' (MongoDB)
  documents?: object[];
  // Pour type='error'
  error?: string;
  // Pour type='message' (ex: "3 rows deleted")
  message?: string;
}

interface NotebookVariable {
  name: string;                   // ex: "customer_id"
  type: 'text' | 'number' | 'date' | 'select';
  defaultValue?: string;
  description?: string;
  // Pour type='select': valeurs possibles
  options?: string[];
  // Valeur actuelle (non persistée dans le fichier, runtime only)
  currentValue?: string;
}

interface ChartConfig {
  type: 'bar' | 'line' | 'pie' | 'scatter';
  sourceCell: string;             // id de la cellule source
  xAxis: string;                  // nom de colonne
  yAxis: string | string[];       // nom(s) de colonne(s)
  title?: string;
}
```

### 3.2 Format fichier sur disque

Le `.qnb` est un fichier JSON (pas binaire) pour être :
- lisible dans un éditeur de texte
- diffable avec Git
- mergeable (chaque cellule a un id stable)

Taille typique : 5-50 Ko sans les résultats, 50-500 Ko avec snapshots.

### 3.3 Intégration au système de tabs

```typescript
// Extension du TabType existant
type TabType = 'query' | 'table' | 'database' | 'diff' | 'notebook';

// Extension de OpenTab
interface OpenTab {
  // ... champs existants ...
  // Nouveaux champs pour les notebooks
  notebookPath?: string;          // chemin du fichier .qnb
  notebookUnsaved?: boolean;      // modifications non sauvegardées
}
```

Fonction factory à ajouter dans `tabs.ts` :

```typescript
function createNotebookTab(title: string, path?: string): OpenTab {
  return {
    id: generateId(),
    type: 'notebook',
    title: title || 'Untitled Notebook',
    notebookPath: path,
  };
}
```

---

## 4. Architecture technique

### 4.1 Vue d'ensemble

```
┌──────────────────────────────────────────────────────┐
│  NotebookTab (nouveau composant top-level)           │
│  ┌────────────────────────────────────────────────┐  │
│  │  NotebookToolbar                               │  │
│  │  [▶ Run All] [↻ Clear] [💾 Save] [⚙ Vars]    │  │
│  ├────────────────────────────────────────────────┤  │
│  │  VariableBar (si variables définies)           │  │
│  │  [$customer_id: 4521] [$date_from: 2026-01-01] │  │
│  ├────────────────────────────────────────────────┤  │
│  │  CellList (scrollable, virtualisé si >50)      │  │
│  │  ┌──────────────────────────────────────────┐  │  │
│  │  │ NotebookCell [markdown]                  │  │  │
│  │  │ ## Investigation paiement client #4521   │  │  │
│  │  ├──────────────────────────────────────────┤  │  │
│  │  │ NotebookCell [sql]                       │  │  │
│  │  │ ┌─ CodeMirror (SQLEditor réutilisé) ──┐ │  │  │
│  │  │ │ SELECT * FROM payments ...            │ │  │  │
│  │  │ └────────────────────────────────────── ┘ │  │  │
│  │  │ ┌─ CellResult (DataGrid réutilisé) ───┐ │  │  │
│  │  │ │ id | amount | status | created_at    │ │  │  │
│  │  │ │ ───────────────────────────────────── │ │  │  │
│  │  │ │ 8832 | 49.99 | failed | 2026-02-14  │ │  │  │
│  │  │ └──────────────────────────────────────┘ │  │  │
│  │  ├──────────────────────────────────────────┤  │  │
│  │  │ NotebookCell [markdown]                  │  │  │
│  │  │ Root cause : timeout Stripe ...          │  │  │
│  │  ├──────────────────────────────────────────┤  │  │
│  │  │       [+ Add Cell]  (sql | md | chart)   │  │  │
│  │  └──────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
```

### 4.2 Composants frontend (nouveaux)

```
src/components/Notebook/
├── NotebookTab.tsx              # Container principal, gère l'état du notebook
├── NotebookToolbar.tsx          # Barre d'actions (run all, save, export, etc.)
├── NotebookVariableBar.tsx      # Inputs pour les variables paramétrées
├── NotebookCellList.tsx         # Liste des cellules (drag-and-drop pour réordonner)
├── cells/
│   ├── NotebookCell.tsx         # Wrapper générique d'une cellule
│   ├── SqlCell.tsx              # Cellule SQL (réutilise SQLEditor)
│   ├── MongoCell.tsx            # Cellule MongoDB (réutilise MongoEditor)
│   ├── MarkdownCell.tsx         # Cellule Markdown (édition + rendu)
│   └── ChartCell.tsx            # Cellule chart (référence une cellule source)
├── results/
│   ├── CellResultViewer.tsx     # Affichage résultat inline (réutilise DataGrid/DocumentResults)
│   └── CellErrorViewer.tsx      # Affichage erreur inline
├── NotebookExportDialog.tsx     # Export en .md / .html / .pdf
└── NotebookCommandPalette.tsx   # Actions spécifiques notebook dans le command palette
```

### 4.3 Composants réutilisés (ZERO réécriture)

| Composant existant | Usage dans Notebook |
|---|---|
| `SQLEditor` | Éditeur CodeMirror dans `SqlCell` (même autocompletion, même shortcuts) |
| `MongoEditor` | Éditeur CodeMirror dans `MongoCell` |
| `DataGrid` | Affichage résultats tabulaires dans `CellResultViewer` |
| `DocumentResults` | Affichage résultats MongoDB dans `CellResultViewer` |
| `executeQuery()` | Exécution des cellules SQL (même flow, même interceptor) |
| `StreamingExport` | Export des résultats de cellules individuelles |
| Interceptor pipeline | Toutes les queries notebook passent par le safety net |

### 4.4 Backend (minimal, car on réutilise l'existant)

Nouvelles commandes Tauri (dans `src-tauri/src/commands/notebook.rs`) :

```rust
/// Sauvegarder un notebook sur disque
#[tauri::command]
async fn save_notebook(path: String, content: String) -> Result<(), String>;

/// Charger un notebook depuis disque
#[tauri::command]
async fn load_notebook(path: String) -> Result<String, String>;

/// Lister les notebooks dans un répertoire
#[tauri::command]
async fn list_notebooks(dir: String) -> Result<Vec<NotebookEntry>, String>;

/// Exporter un notebook en HTML standalone
#[tauri::command]
async fn export_notebook_html(
    notebook_json: String,
    output_path: String,
) -> Result<(), String>;
```

L'exécution des queries ne change pas — on appelle `executeQuery()` existant, cellule par cellule. Le backend notebook est volontairement minimal.

---

## 5. UX détaillée

### 5.1 Création d'un notebook

**Depuis la command palette** : `Ctrl+K` → "New Notebook" → crée un tab notebook vide.

**Depuis le menu contextuel** : clic droit sur une connexion → "New Notebook" (pré-associe la connexion).

**Depuis un query tab** : `Ctrl+Shift+N` ou bouton "Convert to Notebook" → transforme la query courante en notebook avec une cellule SQL initiale.

### 5.2 Édition des cellules

**Ajouter une cellule** : bouton `+` entre chaque cellule (hover-reveal, discret). Choix : SQL, Markdown, Chart. Raccourci : `Ctrl+Shift+Enter` (nouvelle cellule après la courante).

**Supprimer une cellule** : icône corbeille (hover), ou `Ctrl+Shift+Backspace`. Confirmation uniquement si la cellule contient du contenu.

**Réordonner** : drag handle à gauche de chaque cellule. Raccourci : `Alt+↑` / `Alt+↓`.

**Redimensionner** : les cellules SQL/Mongo ont une hauteur auto-adaptative (min 3 lignes, max 20 lignes, scrollable au-delà). Les résultats ont une hauteur par défaut de 10 lignes, extensible manuellement.

### 5.3 Exécution

**Cellule individuelle** : `Ctrl+Enter` (identique au query editor — muscle memory préservée).

**Run All** : `Ctrl+Shift+Enter` depuis la toolbar. Exécute toutes les cellules dans l'ordre, s'arrête à la première erreur (configurable : continuer malgré les erreurs).

**Run From Here** : clic droit sur une cellule → "Run from here" → exécute cette cellule et toutes les suivantes.

**Indicateurs visuels** :
- Cellule idle : bordure gauche `--q-border`
- Cellule running : bordure gauche `--q-accent` + spinner
- Cellule success : bordure gauche `--q-success` (2s puis fade)
- Cellule error : bordure gauche `--q-error` (persiste)
- Stale (source modifiée depuis la dernière exécution) : bordure gauche `--q-warning` en pointillé

### 5.4 Variables / paramètres

Les variables sont définies dans une barre en haut du notebook. Syntaxe dans les queries : `$nom_variable` ou `{{nom_variable}}`.

```sql
SELECT * FROM orders
WHERE created_at >= '{{date_from}}'
  AND customer_id = {{customer_id}};
```

La barre de variables génère automatiquement des inputs typés :
- `text` → input texte
- `number` → input numérique
- `date` → date picker
- `select` → dropdown

Quand une variable change, les cellules qui l'utilisent sont marquées "stale".

### 5.5 Références inter-cellules (v2 de la feature)

Possibilité de référencer le résultat d'une cellule précédente :

```sql
-- Cellule "users_fr" (label configuré)
SELECT id FROM users WHERE country = 'FR';

-- Cellule suivante, référence la première
SELECT * FROM orders WHERE user_id IN ($users_fr.id);
```

Implémentation : le frontend substitue `$users_fr.id` par la liste de valeurs de la colonne `id` du résultat de la cellule nommée `users_fr`. Pas de magie backend.

### 5.6 Cellules Markdown

- Mode édition : textarea avec preview live (split ou toggle)
- Mode lecture : rendu Markdown complet (headers, bold, code blocks, listes, liens)
- Librairie : `react-markdown` ou rendu custom léger
- Double-clic pour passer en mode édition
- `Escape` pour sortir du mode édition

### 5.7 Cellules Chart (v2 de la feature)

Une cellule chart référence une cellule SQL comme source de données :

```
Type: bar
Source: cell_abc123
X axis: month
Y axis: revenue
```

Charts rendus avec `recharts` (déjà dans les dépendances React typiques) ou une lib légère. Pas d'ambition BI — juste une visualisation rapide inline.

### 5.8 Sauvegarde

**Auto-save** : draft en localStorage toutes les 30s (comme les query drafts actuels).

**Save explicite** : `Ctrl+S` → dialogue de sauvegarde si pas de path, sinon overwrite.

**Emplacement par défaut** : répertoire du projet ou dossier configurable dans les settings.

**Indicateur** : point dans le titre du tab si unsaved (pattern standard).

---

## 6. Export & partage

### 6.1 Format `.qnb` (natif)

Le fichier JSON est le format principal. Commitable dans Git.

Stratégie Git-friendly :
- Les `lastResult` sont optionnels au save (toggle "Include results snapshot")
- Sans résultats : fichier léger, diff propre
- Avec résultats : utile pour la documentation, le partage, les audits

### 6.2 Export Markdown

Génère un `.md` avec :
- Les cellules Markdown telles quelles
- Les cellules SQL dans des code blocks ` ```sql `
- Les résultats en tables Markdown (tronqués à N lignes)
- Pas d'interactivité, mais lisible partout (GitHub, Notion, etc.)

### 6.3 Export HTML standalone

Un fichier `.html` autosuffisant avec :
- Les queries avec syntax highlighting (inline CSS)
- Les résultats en tables HTML stylées
- Le Markdown rendu
- Navigation par ancres
- Dark/light theme toggle

Parfait pour un post-mortem partagé par email ou sur Confluence.

### 6.4 Import

**Depuis un `.sql`** : chaque statement séparé par `;` ou `\n\n` devient une cellule SQL.

**Depuis un `.md`** : les code blocks SQL deviennent des cellules SQL, le reste devient des cellules Markdown.

---

## 7. Keyboard shortcuts

| Action | Shortcut | Contexte |
|---|---|---|
| Exécuter cellule courante | `Ctrl+Enter` | Dans une cellule SQL/Mongo |
| Exécuter tout le notebook | `Ctrl+Shift+A` | Toolbar |
| Nouvelle cellule SQL après | `Ctrl+Shift+Enter` | Partout dans le notebook |
| Nouvelle cellule Markdown après | `Ctrl+Shift+M` | Partout dans le notebook |
| Supprimer cellule | `Ctrl+Shift+Backspace` | Cellule focusée |
| Déplacer cellule vers le haut | `Alt+↑` | Cellule focusée |
| Déplacer cellule vers le bas | `Alt+↓` | Cellule focusée |
| Sauvegarder | `Ctrl+S` | Partout dans le notebook |
| Toggle résultat (plier/déplier) | `Ctrl+Shift+R` | Cellule avec résultat |
| Focus cellule précédente | `Ctrl+↑` | Navigation entre cellules |
| Focus cellule suivante | `Ctrl+↓` | Navigation entre cellules |
| Convertir cellule (cycle type) | `Ctrl+Shift+T` | Cellule focusée |

---

## 8. Plan d'implémentation

### Phase 1 — MVP (2 semaines)

**Objectif** : un notebook fonctionnel, sans fioritures.

Backend :
- [ ] `commands/notebook.rs` : save, load, list (simple I/O fichier)

Frontend :
- [ ] `NotebookTab.tsx` : state management du notebook (cells, execution)
- [ ] `NotebookCell.tsx` : wrapper avec bordure d'état, boutons d'action
- [ ] `SqlCell.tsx` : intègre `SQLEditor` existant, exécution via `executeQuery`
- [ ] `MarkdownCell.tsx` : édition + rendu markdown basique
- [ ] `CellResultViewer.tsx` : intègre `DataGrid` existant en mode compact
- [ ] Nouveau tab type `'notebook'` dans `tabs.ts` et `useTabs.ts`
- [ ] `Ctrl+Enter` pour exécuter, `Ctrl+S` pour sauvegarder
- [ ] Bouton `+` pour ajouter des cellules
- [ ] Drag-and-drop pour réordonner

Pas dans le MVP : variables, charts, Run All, export, MongoCell.

### Phase 2 — Complet (2 semaines)

- [ ] `NotebookToolbar.tsx` : Run All, Clear All, export
- [ ] `NotebookVariableBar.tsx` : variables avec inputs typés
- [ ] Substitution de variables dans les queries
- [ ] `MongoCell.tsx` : support MongoDB
- [ ] Indicateurs visuels d'état (stale, running, success, error)
- [ ] Auto-save en localStorage
- [ ] Import depuis `.sql` et `.md`
- [ ] Export Markdown
- [ ] Intégration command palette ("New Notebook", "Open Notebook")
- [ ] "Convert Query to Notebook" depuis un query tab

### Phase 3 — Power features (2 semaines)

- [ ] Références inter-cellules (`$cell_label.column`)
- [ ] `ChartCell.tsx` : visualisation basique (bar, line, pie)
- [ ] Export HTML standalone
- [ ] Run From Here / Run Selected
- [ ] Résultats : toggle snapshot au save (include/exclude)
- [ ] Outline panel (sidebar avec la liste des cellules pour navigation rapide)
- [ ] Search & Replace dans tout le notebook
- [ ] Duplicate cell
- [ ] Merge cells (2 markdown → 1)

---

## 9. Points d'attention

### Performance
- Les résultats inline doivent utiliser `maxRows` (défaut 500) pour ne pas exploser le DOM
- Si >50 cellules : virtualiser la liste des cellules (react-virtual, déjà dans le projet)
- Les résultats en snapshot sont stockés tronqués (pas 100K lignes en JSON)

### Sécurité
- Les queries notebook passent par l'interceptor exactement comme les queries classiques
- Le mode sandbox est compatible : on peut activer sandbox dans un notebook
- Les notebooks n'exécutent RIEN au chargement (l'utilisateur doit cliquer Run)
- Les variables sont sanitisées côté frontend avant substitution

### UX
- Le notebook NE REMPLACE PAS le query editor — c'est un outil complémentaire
- Un notebook vide avec une seule cellule SQL est visuellement quasi-identique au query editor (pas de surcharge cognitive)
- La transition query → notebook doit être fluide (Ctrl+Shift+N et c'est fait)

### Cohérence avec le design system
- Les cellules utilisent `--q-bg-1` comme fond, `--q-border` comme séparation
- Les indicateurs d'état utilisent les couleurs sémantiques existantes
- La densité des résultats inline est identique au DataGrid classique
- Pas de décoration inutile — le notebook est un outil de travail, pas un canvas créatif

---

## 10. Métriques de succès

- **Adoption** : >30% des utilisateurs actifs créent au moins 1 notebook dans le premier mois
- **Rétention** : les utilisateurs qui créent 3+ notebooks ont un taux de rétention 2x supérieur
- **Partage** : >10% des notebooks sont exportés (signe qu'ils ont de la valeur au-delà de l'auteur)
- **Conversion** : le notebook est dans le top 3 des raisons citées pour choisir QoreDB vs alternatives

---

## 11. Ce qu'on ne fait PAS

- **Pas de collaboration temps réel** (v3+ si le produit va vers le multi-user)
- **Pas de scheduling** (ce n'est pas Airflow — on reste un client DB)
- **Pas de BI** (les charts sont une commodité, pas un système de dashboarding)
- **Pas d'exécution côté serveur** (tout est local, cohérent avec la philosophie QoreDB)
- **Pas de kernel externe** (contrairement à Jupyter, pas besoin de process séparé)
