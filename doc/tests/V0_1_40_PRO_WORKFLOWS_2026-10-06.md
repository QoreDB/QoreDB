# v0.1.40 — Parcours Pro, 6 octobre 2026

Référence Git : `79a3664`, arbre initial propre, branche `feat/v0-1-40`.
Les fixtures sont synthétiques. Ce rapport couvre des correctifs ciblés du
[plan v0.1.40](../todo/V0_1_40.md), sans qualifier la release ni les parcours natifs.

## Variables et références des notebooks

Dix des dix-sept premiers tests échouaient avant correction : valeurs contenant
un autre placeholder rescannées, grands entiers et décimaux arrondis, saisie
numérique vide transformée en zéro, hexadécimal accepté et confusion entre
variable `$users` et référence `$users.id`.

La substitution parcourt désormais la source une seule fois. Les nombres suivent
une grammaire décimale avec exposant facultatif et conservent leurs chiffres ;
ils ne transitent plus par un double JavaScript. Les valeurs non valides restent
non résolues, conformément au contrat existant. Les variables absentes et les
échappements `$$` restent inchangés. Les propriétés héritées de l'objet des
variables ne constituent pas des définitions.

L'exécution du notebook résout variables et références entre cellules depuis
les seuls tokens de la source originale. Une valeur contenant `$users.id` ou
`{{autre}}` ne déclenche aucune substitution supplémentaire. Les requêtes
sauvegardées utilisent le même correctif de variables. Le dialogue de saisie des
variables se charge à son ouverture pour respecter le budget frontend.

Validation : 20 tests ciblés passent, vérification TypeScript réussie, Biome
réussi sur les fichiers modifiés. Les tests portent sur le SQL produit ; ils ne
prouvent pas le stockage numérique sur chaque moteur. La recette Tauri,
l'invalidation des résultats intercellules périmés et les règles d'échappement
spécifiques à chaque dialecte restent à qualifier.

```bash
pnpm test:ts src/lib/notebook/notebookVariables.test.ts src/lib/notebook/notebookInterCellRef.test.ts
pnpm typecheck
pnpm exec biome check src/lib/notebook/notebookVariables.ts src/lib/notebook/notebookVariables.test.ts src/lib/notebook/notebookInterCellRef.ts src/lib/notebook/notebookInterCellRef.test.ts src/hooks/useNotebook.ts src/components/Query/QueryLibraryModal.tsx
```

## Fidélité de Visual Diff

Six des neuf premiers tests reproduisaient une perte de lignes portant une même
clé, des différences dues au seul ordre des propriétés JSON et un retour chariot
non protégé dans le CSV. Le diff conserve maintenant chaque occurrence,
apparie d'abord les lignes identiques, puis les autres dans l'ordre des sources.
Les identités de rendu restent distinctes. Les objets JSON sont comparés avec
des propriétés ordonnées ; l'ordre des tableaux et les types primitifs restent
significatifs. Les grands nombres transportés en chaînes restent exacts.

Six tests supplémentaires reproduisaient l'absence d'un statut incomplet.
L'UI et le JSON signalent maintenant clés ambiguës ou absentes, colonnes non
comparées, masquage et troncature. Le CSV ajoute `_comparison_incomplete` à
chaque ligne ; un CSV vide ne contient que son en-tête, le JSON conserve les
raisons même sans ligne. Les colonnes de statut existantes sont préservées.
La troncature des requêtes est conservée depuis la réponse et celle d'une table
est évaluée avec la limite utilisée lors de sa lecture. Changer le sélecteur de
limite ne rend plus une page existante exhaustive. Les filtres sans ligne
affichent leur propre état vide ; un résultat incomplet n'annonce pas une
égalité globale. Les nouvelles chaînes sont traduites dans les neuf locales.

Les tests couvrent aussi clés composites, colonnes réordonnées, NULL, colonnes
homonymes et un groupe de 25 000 doublons. L'appariement des clés dupliquées est
déterministe, mais ne prouve pas une identité de ligne : l'avertissement reste
présent. La comparaison des seules colonnes communes et le repli par position
sont conservés et signalés comme incomplets.

## Chargement et sessions de Visual Diff

Sept scénarios Chromium échouaient avant correction : succès et échec tardifs
réintroduits après modification de la requête, actualisation comparant les
anciens résultats, deux connexions ouvertes pour une même source, changement
de limite utilisant encore l'ancienne valeur, capture tardive réintroduite et
boucle de lecture après échec d'une capture asynchrone.

Les lectures de table, requête et capture partagent maintenant leur contrôle
de génération. Changement de source, inversion, remise à zéro, fermeture et
changement de workspace invalident les réponses en attente. L'actualisation
compare les réponses qu'elle vient de recevoir, sans temporisation ni closure
sur les anciens résultats. Une lecture en cours ou en échec invalide le diff
précédent ; changer les colonnes clés l'invalide aussi. Une capture indisponible
présente son erreur jusqu'à une reprise explicite.

La promesse de connexion est enregistrée avant son attente et partagée entre
les deux côtés, avec une origine de workspace. Le dernier propriétaire ferme
la session ; fermer la vue ferme aussi les sessions dont la connexion finit
plus tard. Un échec de lecture des namespaces libère la session acquise et
permet une reprise explicite. Changer de workspace efface les sources actives,
leurs résultats et leur cache de schéma.
Les erreurs de source disposent d'un bouton Réessayer qui relance la connexion
ou la lecture concernée ; aucune reconnexion répétitive n'est déclenchée.

Treize scénarios du hook réel passent dans Chromium, avec IPC simulé. Ils
couvrent les sept régressions, troncature, échec d'actualisation, changement de
clé, reprise après échec des namespaces, connexion tardive après fermeture et
workspace. Ces contrôles ne prouvent pas l'ouverture/fermeture native d'un
driver ni le comportement d'un serveur distant.

```bash
node scripts/test-diff-sources-ui.mjs
node scripts/test-pro-workflows-ui.mjs
pnpm test:ts src/lib/diffUtils.test.ts
pnpm typecheck
```

Le script d'interface vérifie aussi les vrais composants de grille et de
bibliothèque : distinction des états vides, dialogue chargé seulement à son
ouverture et SQL exact soumis depuis les champs réels. Les messages français
ont été inspectés à 600 px de largeur en thème sombre dans
`.perf/pro-workflows-ui.png` (artefact local ignoré).

## Clavier du dialogue de variables

Le parcours Chromium reproduisait un focus perdu après Escape. Le dialogue
mémorise maintenant son déclencheur et lui rend le focus s'il est encore
présent. Les actions de bibliothèque deviennent visibles au focus clavier.
Le formulaire accepte Enter pour valider ; Annuler ne le soumet pas. Les champs
numériques acceptent les décimaux avec `step="any"` et conservent leur texte.

Le navigateur vérifie ouverture par Enter, focus initial, Tab/Shift+Tab, Escape
avec focus restauré, nouvelle ouverture puis validation par Enter. Les deux
saisies `9007199254740993` et `0.12345678901234567890` apparaissent sans arrondi
dans le SQL reçu par le callback. Cette preuve concerne Chromium avec stockage
et licence simulés ; le parcours macOS/WebView et la recette Tauri restent ouverts.

## Bilan des vérifications

| Vérification | Résultat |
| --- | --- |
| `pnpm test:ts` | 295 tests réussis dans 37 fichiers ; 37 tests ajoutés |
| `pnpm typecheck` | Réussi |
| `node scripts/test-diff-sources-ui.mjs` | 13 scénarios Chromium réussis, IPC simulé |
| `node scripts/test-pro-workflows-ui.mjs` | 7 groupes de contrôles réussis, licence et stockage simulés |
| Biome sur les 26 fichiers frontend/fixtures concernés | Aucune erreur ; 12 avertissements déjà présents dans les composants Diff (clés par index, assertions non nulles et callback `forEach`) |
| `pnpm docs:check` et `git diff --check` | Réussis |
| Rust, bases réelles et recette Tauri | Non exécutés dans ce lot frontend |

Les scripts navigateur utilisent Chrome for Testing 153.0.8010.12 sous Linux
x86_64 et Playwright déjà installé, avec
`QOREDB_PLAYWRIGHT_MODULE` pointant vers son module local et
`QOREDB_CHROMIUM_EXECUTABLE` vers Chromium. Aucune dépendance de production ou
de développement n'est ajoutée. Les commandes reproduisibles et les variantes
d'environnement sont décrites dans le [guide de tests](../development/TESTING.md).

## Poids du frontend

Référence : HEAD `79a3664` avant modifications ; ce n'est pas une mesure de la
release publiée v0.1.39. Référence et candidate utilisent Linux x86_64,
Node.js 26.10.0, pnpm 11.3.0 et Vite 8.1.0, avec les mêmes lockfile,
configuration et empreintes d'environnement. La comparaison valide ces
métadonnées. La candidate inclut les quatre lots de ce rapport.

```bash
pnpm perf:bundle --output .perf/v0-1-40-2026-10-06-before.json
pnpm perf:bundle --baseline .perf/v0-1-40-2026-10-06-before.json --output .perf/v0-1-40-2026-10-06-final.json
```

| Mesure, octets | Référence | Candidate | Écart | Budget |
| --- | ---: | ---: | ---: | --- |
| JS initial brut | 2 435 378 | 2 434 659 | −719 | Respecté |
| JS initial gzip | 707 591 | 707 575 | −16 | Respecté |
| CSS initial brut | 111 943 | 111 943 | 0 | Respecté |
| CSS initial gzip | 17 491 | 17 491 | 0 | Respecté |
| JS total brut | 4 794 036 | 4 803 787 | +9 751 (+0,203 %) | Respecté, maximum +2 % |
| JS total gzip | 1 421 800 | 1 425 572 | +3 772 (+0,265 %) | Respecté, maximum +2 % |
| CSS total brut | 111 943 | 111 943 | 0 | Respecté |
| CSS total gzip | 17 491 | 17 491 | 0 | Respecté |

Le premier essai sans chargement différé du dialogue de variables dépassait
le budget initial ; le lot final le respecte. Le navigateur vérifie que ce
module n'est téléchargé ni au montage de la fixture ni à l'ouverture de la
bibliothèque, mais à l'ouverture de sa saisie de variables.

Les rapports JSON et journaux restent locaux sous `.perf/`. La mesure porte
sur la fermeture statique des imports et le gzip niveau 9 par fichier, pas sur
la mémoire, le CPU, la latence native ou la taille installée. Ces références
restent ouvertes dans le plan de release.
