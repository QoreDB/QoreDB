│ Visual Data Diff - Plan d'implementation │
│ │
│ Resume │
│ │
│ Reimplementation complete de la fonctionnalite "Visual Data Diff" pour comparer deux sources de donnees cote a cote avec mise en evidence visuelle des differences (style Git diff). │
│ │
│ Conclusion backend : Aucune nouvelle commande backend necessaire. Les commandes existantes (executeQuery, describeTable, listCollections) suffisent. Tout se fait cote frontend. │
│ │
│ --- │
│ Problemes de l'implementation actuelle │
│ │
│ 1. Pas de selection de tables - Juste deux champs texte pour requetes SQL │
│ 2. Interface confuse - Pas de workflow clair │
│ 3. Pas de bouton "Comparer" explicite │
│ 4. Pas de configuration des colonnes cles pour le matching │
│ 5. UI incomplete - Pas de toolbar, pas d'export │
│ │
│ --- │
│ Architecture cible │
│ │
│ DataDiffViewer (main) │
│ ├── DiffToolbar (swap, export, refresh) │
│ ├── DiffSourcePanel x2 (left/right) │
│ │ ├── Tabs: Table | Query │
│ │ ├── DiffTablePicker (dropdown searchable) │
│ │ └── QueryInput (textarea SQL) │
│ ├── DiffConfigPanel │
│ │ ├── Key columns selector │
│ │ └── Compare button │
│ ├── DiffStatsBar (stats + filtres) │
│ └── DiffResultsGrid (virtualise) │
│ │
│ --- │
│ Fichiers a creer │
│ ┌─────────────────────────────────────────────┬────────────────────────────────────────────┐ │
│ │ Fichier │ Description │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/DiffSourcePanel.tsx │ Panel de selection source (table ou query) │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/DiffTablePicker.tsx │ Dropdown searchable pour tables │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/DiffConfigPanel.tsx │ Config colonnes cles + bouton Compare │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/DiffStatsBar.tsx │ Stats + filtres │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/DiffResultsGrid.tsx │ Grille virtualisee des resultats │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/DiffToolbar.tsx │ Toolbar (swap, export) │ │
│ ├─────────────────────────────────────────────┼────────────────────────────────────────────┤ │
│ │ src/components/Diff/hooks/useDiffSources.ts │ Hook gestion sources et execution │ │
│ └─────────────────────────────────────────────┴────────────────────────────────────────────┘ │
│ Fichiers a modifier │
│ ┌──────────────────────────────────────────┬──────────────────────────────────────────┐ │
│ │ Fichier │ Modification │ │
│ ├──────────────────────────────────────────┼──────────────────────────────────────────┤ │
│ │ src/components/Diff/DataDiffViewer.tsx │ Rewrite complet avec nouveaux composants │ │
│ ├──────────────────────────────────────────┼──────────────────────────────────────────┤ │
│ │ src/components/Tree/TableContextMenu.tsx │ Ajouter "Compare with..." │ │
│ ├──────────────────────────────────────────┼──────────────────────────────────────────┤ │
│ │ src/lib/diffUtils.ts │ Ajouter export CSV/JSON │ │
│ ├──────────────────────────────────────────┼──────────────────────────────────────────┤ │
│ │ src/locales/en.json │ Nouvelles traductions diff │ │
│ ├──────────────────────────────────────────┼──────────────────────────────────────────┤ │
│ │ src/locales/fr.json │ Nouvelles traductions diff │ │
│ └──────────────────────────────────────────┴──────────────────────────────────────────┘ │
│ --- │
│ Phases d'implementation │
│ │
│ Phase 1 : Composants de selection (2 fichiers) │
│ │
│ 1. DiffTablePicker.tsx - Dropdown avec recherche │
│ - Utilise listCollections pour charger les tables │
│ - Pattern Command (comme GlobalSearch) │
│ - Props: sessionId, namespace, onSelect │
│ 2. DiffSourcePanel.tsx - Panel complet │
│ - Tabs: "Table" | "Query" │
│ - Integre DiffTablePicker ou textarea SQL │
│ - Bouton Execute par source │
│ - Affiche row count apres execution │
│ │
│ Phase 2 : Configuration et stats (2 fichiers) │
│ │
│ 3. DiffConfigPanel.tsx - Configuration du diff │
│ - Multi-select pour colonnes cles │
│ - Auto-detect PK checkbox (utilise describeTable) │
│ - Gros bouton "Compare" avec loader │
│ 4. DiffStatsBar.tsx - Barre de stats │
│ - Compteurs: +added -removed ~modified =unchanged │
│ - Couleurs: green/red/yellow/muted │
│ - Dropdown filtre + toggle "Show unchanged" │
│ │
│ Phase 3 : Grille et toolbar (2 fichiers) │
│ │
│ 5. DiffResultsGrid.tsx - Grille virtualisee │
│ - Utilise @tanstack/react-virtual (pattern de DataGrid.tsx) │
│ - Row colors: bg-green-500/10, bg-red-500/10, bg-yellow-500/10 │
│ - Cellules modifiees: old strikethrough + new en vert │
│ 6. DiffToolbar.tsx - Actions │
│ - Swap sources │
│ - Export CSV/JSON │
│ - Refresh │
│ │
│ Phase 4 : Hook et assemblage (2 fichiers) │
│ │
│ 7. hooks/useDiffSources.ts - Logique metier │
│ interface UseDiffSourcesReturn { │
│ leftSource, rightSource, │
│ leftResult, rightResult, │
│ leftLoading, rightLoading, │
│ setLeftSource, setRightSource, │
│ executeLeft, executeRight, │
│ swap │
│ } │
│ 8. DataDiffViewer.tsx - Rewrite complet │
│ - Compose tous les sous-composants │
│ - Utilise useDiffSources │
│ - Appelle compareResults de diffUtils │
│ │
│ Phase 5 : Integration (3 fichiers) │
│ │
│ 9. TableContextMenu.tsx - Menu contextuel │
│ <ContextMenuItem onClick={() => onCompareWith?.(collection)}> │
│ <GitCompare size={14} className="mr-2" /> │
│ {t('diff.compareTable')} │
│ </ContextMenuItem> │
│ 10. diffUtils.ts - Export │
│ - exportDiffAsCSV(diffResult): string │
│ - exportDiffAsJSON(diffResult): string │
│ 11. Traductions - Completer en.json et fr.json │
│ │
│ --- │
│ Patterns a reutiliser │
│ ┌──────────────────────┬──────────────────────┬─────────────────┐ │
│ │ Pattern │ Source │ Usage │ │
│ ├──────────────────────┼──────────────────────┼─────────────────┤ │
│ │ Virtualisation │ DataGrid.tsx │ DiffResultsGrid │ │
│ ├──────────────────────┼──────────────────────┼─────────────────┤ │
│ │ Couleurs changements │ ChangeItem.tsx │ Stats et rows │ │
│ ├──────────────────────┼──────────────────────┼─────────────────┤ │
│ │ Dropdown searchable │ GlobalSearch.tsx │ DiffTablePicker │ │
│ ├──────────────────────┼──────────────────────┼─────────────────┤ │
│ │ Context menu │ TableContextMenu.tsx │ Integration │ │
│ └──────────────────────┴──────────────────────┴─────────────────┘ │
│ Couleurs (de ChangeItem.tsx) : │
│ - Insert: text-success, bg-success/10 │
│ - Update: text-warning, bg-warning/10 │
│ - Delete: text-error, bg-error/15 │
│ │
│ --- │
│ Flux utilisateur │
│ │
│ Cas 1: Compare deux tables │
│ │
│ 1. Ouvrir Data Diff (palette Ctrl+K) │
│ 2. Panel gauche: Tab "Table" → select table A │
│ 3. Panel droit: Tab "Table" → select table B │
│ 4. [Optionnel] Configurer colonnes cles │
│ 5. Clic "Compare" │
│ 6. Voir resultats, filtrer, exporter │
│ │
│ Cas 2: Compare depuis menu contextuel │
│ │
│ 1. Clic droit sur table → "Compare with..." │
│ 2. Ouvre DiffTab avec source gauche pre-remplie │
│ 3. Selectionner source droite │
│ 4. Clic "Compare" │
│ │
│ --- │
│ Verification │
│ │
│ 1. Build : pnpm build sans erreurs │
│ 2. Lint : pnpm lint sans erreurs dans Diff/ │
│ 3. Test manuel : │
│ - Ouvrir via palette de commandes │
│ - Selectionner table dans dropdown │
│ - Executer requete personnalisee │
│ - Configurer colonnes cles │
│ - Clic Compare → voir resultats │
│ - Filtrer par type (added/removed/modified) │
│ - Export CSV et JSON │
│ - Swap sources │
│ - Clic droit table → "Compare with..." │
│ │
│ --- │
│ Estimations │
│ ┌─────────┬─────────────────────────┐ │
│ │ Phase │ Complexite │ │
│ ├─────────┼─────────────────────────┤ │
│ │ Phase 1 │ Moyenne │ │
│ ├─────────┼─────────────────────────┤ │
│ │ Phase 2 │ Moyenne │ │
│ ├─────────┼─────────────────────────┤ │
│ │ Phase 3 │ Elevee (virtualisation) │ │
│ ├─────────┼─────────────────────────┤ │
│ │ Phase 4 │ Moyenne │ │
│ ├─────────┼─────────────────────────┤ │
│ │ Phase 5 │ Faible │ │
│ └─────────┴─────────────────────────┘
