# UX/UI Audit — QoreDB

Audit professionnel de l'expérience utilisateur. Propositions classées par impact.

---

## CRITIQUE — Accessibilité & Safety

- [x] **1. Focus rings invisibles** — Focus ring `var(--q-accent)` ajouté sur `button.tsx`, `TabBar.tsx` tab buttons, `ConnectionItem.tsx`. `cursor-not-allowed` ajouté sur disabled.
- [x] **2. Pas d'Error Boundary** — Composant `ErrorBoundary` créé (`ui/error-boundary.tsx`), wrappant `AppContent` dans `AppLayout.tsx`. Fallback avec icône, message d'erreur, bouton "Reload panel".
- [x] **3. ARIA manquants** — `aria-controls` + `id` sur tab buttons, `aria-expanded` sur `ConnectionItem`, `role="menu"` + `role="menuitem"` sur context menu, navigation clavier (ArrowUp/Down/Escape), focus trap, `<hr>` sémantique.

## HIGH — Feedback & confiance utilisateur

- [x] **4. Exécution de query invisible** — Timer live avec spinner `Loader2` animé dans le bouton Run : `2.3s...`. Se met à jour toutes les 100ms. Tooltip `Run (⌘+Enter)` ajouté. (`QueryPanelToolbar.tsx`)
- [x] **5. Messages d'erreur plus riches** — Durée auto-adaptée (8s pour erreurs longues), extraction élargie à 200 chars. (`notify.tsx`)
- [x] **6. Pas de validation inline formulaires** — `input.tsx` supporte maintenant `aria-invalid` : bordure rouge + focus ring rouge automatiques via `aria-[invalid=true]:border-[var(--q-error)]`.
- [x] **7. Empty states pauvres** — `ResultsTable.tsx` : icône `SearchX` + message + hint. `Sidebar.tsx` : icône `Database` + message + bouton CTA "New connection". i18n EN+FR.

## MEDIUM-HIGH — Polish pro

- [x] **8. Headers de table sticky en scroll horizontal** — `headerRef` + `onScroll` synchronise le scroll horizontal du header avec le body. (`ResultsTable.tsx`)
- [x] **9. Recherche dans le sidebar** — Champ de recherche avec icône `Search`, filtrage case-insensitive sur le nom de connexion, apparaît à partir de 4+ connexions. i18n EN+FR. (`Sidebar.tsx`)
- [x] **10. Onglets résultats confus** — Les tabs affichent maintenant les 30 premiers caractères de la query + temps d'exécution : `SELECT * FROM users... (42ms)`. (`QueryPanelResults.tsx`)
- [x] **11. Tooltips raccourcis clavier** — Tooltip sur bouton Run (`⌘+Enter`), bouton "+" new tab (`⌘+T`), bouton close tab. (`QueryPanelToolbar.tsx`, `TabBar.tsx`)
- [x] **12. Boutons disabled cursor-not-allowed** — Ajouté globalement dans `button.tsx` base class.

## MEDIUM — Expérience pro

- [x] **13. Skeleton loaders** — Skeleton animé (3 lignes pulse) affiché pendant la connexion dans le sidebar. (`Sidebar.tsx`)
- [x] **14. Tab overflow gestionnaire** — Bouton `ChevronsUpDown` apparaît à 6+ tabs, ouvre un dropdown listbox avec tous les onglets + icônes + indicateur pin. (`TabBar.tsx`)
- [ ] **15. Navigation clavier dans les résultats** — Pas de flèches dans les cellules, pas de copie `Ctrl+C`. Ajouter focus cellule + navigation + copie. (`ResultsTable.tsx`, `DataGrid.tsx`)
- [ ] **16. Breadcrumbs dans query editor** — L'utilisateur perd le contexte base/schéma. Breadcrumb visible : `PostgreSQL > mydb > public`. (`QueryPanel.tsx`)
- [x] **17. Context menus discoverables** — Déjà implémenté : `ConnectionMenu` (bouton `...`) visible au hover, `ConnectionContextMenu` (clic droit). (`ConnectionItem.tsx`)
- [ ] **18. Tokens de design non-enforcés** — `text-white` hardcodé, pixels magiques. Mapper tokens dans tailwind config, lint les hex. (`button.tsx`, `WelcomeScreen.tsx`)
- [x] **19. Cheatsheet raccourcis clavier** — `?` ouvre un overlay avec tous les raccourcis (General, Tabs, Query Editor). Fermeture via `?` ou `Esc`. i18n EN+FR. (`KeyboardCheatsheet.tsx`, `ShortcutProvider.tsx`)
- [x] **20. Confirmation post-connexion** — Déjà implémenté : auto-connexion après save dans `ConnectionModal.tsx`.
