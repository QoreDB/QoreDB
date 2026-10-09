# v0.1.40 — Bibliothèque et changements de projet, 9 octobre 2026

## Référence et périmètre

Branche `feat/v0-1-40`, référence `e7aa546`. Ce lot traite la persistance de la
bibliothèque de requêtes lors des changements de workspace. Les données des
tests sont synthétiques. Le schéma de compatibilité est vérifié contre
`v0.1.39:src/lib/query/queryLibrary.ts` ; ce test n'est pas une migration complète
d'un profil natif.

## Défauts et comportement attendu

Deux tests échouaient avant correction : une sauvegarde différée n'indiquait pas
son projet d'origine ; une modification dans B annulait le timer de sauvegarde
de A. Le backend écrivait dans le workspace actif au moment de l'appel, ce qui
permettait à la bibliothèque de A d'écraser celle de B après une bascule.

Les timers et écritures en cours sont désormais séparés par projet. Les commandes
Rust exigent l'identité du projet et la vérifient sous le verrou du gestionnaire,
maintenu pendant la lecture ou l'écriture. Une requête périmée est refusée, y
compris après retour au projet par défaut. Aucun chemin arbitraire fourni par
le frontend n'est utilisé pour écrire.

Les quatre transitions du provider React — changer, ouvrir, créer et revenir au
projet par défaut — attendent la sauvegarde avant de changer le backend. Un échec
laisse le projet courant actif et affiche l'erreur de sauvegarde déjà traduite.
Les doubles transitions sont refusées et les écritures locales sont suspendues
pendant la bascule. Les suppressions depuis la bibliothèque gèrent ce refus
sans promesse rejetée non interceptée.

Les modifications arrivant pendant une sauvegarde sont écrites dans l'ordre.
Une lecture disque tardive ne remplace ni un autre projet, ni des modifications
locales plus récentes, ni le résultat d'un rechargement plus récent.

## Récupération et compatibilité

Le cache local porte un indicateur de synchronisation en attente, effacé seulement
après confirmation de l'écriture de la même révision. Il survit au redémarrage du
frontend : le prochain chargement tente d'abord de sauvegarder ces modifications.
Si l'écriture échoue encore, les données disque ne remplacent pas la copie locale.
Cet indicateur ne figure ni dans le fichier partagé ni dans l'export JSON.

Le format de fichier reste en version 1. La fixture v0.1.39 conserve dossiers,
favoris, tags, moteur, base, variables, SQL et valeurs numériques textuelles
exactes après chargement, modification, sauvegarde et réouverture. Le projet par
défaut continue d'utiliser uniquement localStorage.

## Vérifications

- Deux régressions reproduites avant correction.
- Douze tests TypeScript ciblés : identité des sauvegardes, timers indépendants,
  attente avant transition, écritures sérialisées, échec/reprise, cache persistant,
  réponses périmées, compatibilité v0.1.39 et profil par défaut.
- Un nouveau test Rust exécuté en Core et Pro utilise deux vrais workspaces dans
  des répertoires temporaires. Il vérifie le refus des lectures/écritures périmées,
  les octets inchangés des deux fichiers et la réouverture des données de A.
- Dix scénarios Chromium utilisent le vrai provider React et les bindings, avec
  IPC simulé : les quatre transitions en succès puis échec/reprise, les demandes
  concurrentes et une nouvelle session frontend avec une sauvegarde en attente.
- Les trente scénarios Chromium existants des notebooks et de l'ouverture différée
  de la bibliothèque passent, sans erreur navigateur.

La suite TypeScript complète passe : 348 tests dans 40 fichiers. Typecheck,
Biome ciblé, rustfmt ciblé, documentation et contrôle des espaces passent.

Commandes :

```bash
pnpm test:ts
pnpm typecheck
pnpm exec biome check src/lib/query/queryLibrary.ts src/lib/query/queryLibrary.test.ts src/lib/tauri/workspace.ts src/providers/WorkspaceProvider.tsx src/components/Query/QueryLibraryModal.tsx scripts/fixtures/query-library/app.jsx
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --lib commands::workspace_queries::tests
LIBRARY_PATH=/home/raphael/.cache/qoredb-v0-1-40-target/debug/deps cargo test --manifest-path src-tauri/Cargo.toml --target-dir /home/raphael/.cache/qoredb-v0-1-40-target -j 4 -p qoredb --features pro --lib commands::workspace_queries::tests
rustfmt --edition 2024 --check src-tauri/src/commands/workspace_queries.rs
pnpm docs:check
git diff --check
```

Après lancement de Vite sur le port 1430, exécuter
`scripts/test-query-library-ui.mjs` et `scripts/test-notebook-ui.mjs` avec Node.
Les variables `QOREDB_PLAYWRIGHT_MODULE` et `QOREDB_CHROMIUM_EXECUTABLE` permettent
d'utiliser les installations locales de Playwright et Chromium. Le parcours
natif Tauri n'est pas exécuté ; les tests Rust ciblés ne sont pas les suites
Core/Pro complètes. La résistance à une interruption physique d'écriture et les
modifications simultanées par plusieurs instances ne sont pas qualifiées ici.
