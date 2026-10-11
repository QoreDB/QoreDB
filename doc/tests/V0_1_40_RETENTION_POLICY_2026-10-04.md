# v0.1.40 — Rétention applicative, 4 octobre 2026

Référence : `b151d2063c5bf116afff04f59f50b5a3f9b2ffaf`, avec les lots locaux
[historique hors cache](V0_1_40_HISTORY_RETENTION_2026-10-04.md) et
[origine workspace](V0_1_40_WORKSPACE_HISTORY_2026-10-04.md) conservés.
Ce complément clôt leurs points ouverts sur le déclenchement de la purge par
durée et l'application de la limite de taille. Il ne clôt pas la qualification
de Time Travel ou de la release.

## Défauts reproduits et critères

Quatre tests échouaient avant correction : la taille maximale n'était pas
appliquée, réduire la durée ou le nombre maximal ne nettoyait pas l'historique
existant, et un échec de sauvegarde des réglages modifiait quand même l'état
en mémoire sans remonter d'erreur. La purge par durée existait mais n'était
appelée par aucun parcours applicatif.

Le comportement attendu est l'application automatique des trois limites, même
sans nouvelle capture, avec préservation du journal et du cache si sa
réécriture échoue. L'interface doit attendre un enregistrement explicite :
saisir « 70 » ne doit pas appliquer une rétention intermédiaire de sept jours.
Un réglage non chargé ne peut pas être remplacé par les valeurs par défaut,
et un échec doit permettre une nouvelle tentative avec la saisie conservée.

Une régression navigateur supplémentaire reproduit la saisie des exclusions :
« sessions, audit_log » devenait « sessionsaudit_log », car chaque frappe
supprimait immédiatement le séparateur. Le champ conserve maintenant son texte
pendant la saisie et transmet les deux noms séparés à l'enregistrement.

## Mise en œuvre

Le [module de rétention](../../src-tauri/src/time_travel/retention.rs) parcourt
le journal complet sous le verrou commun aux écritures et aux purges. Il retire
les événements expirés, puis les plus anciennes positions restantes si le
nombre ou le volume dépasse sa limite. Le calcul garde des indices et des
longueurs, sans accumuler les images des lignes. Une réécriture n'est effectuée
que si des entrées sont retirées ; sa publication reste atomique et le cache
est remplacé après le renommage réussi.

La tâche de maintenance démarre avec le desktop, effectue un premier passage
immédiat puis un passage toutes les quinze minutes. Les lectures et
réécritures utilisent le pool bloquant, sans retenir l'état global Tauri.
Les passages ne se chevauchent pas. Une erreur est journalisée et le passage
suivant réessaie ; la tâche ne conserve qu'une référence faible entre deux
passages. Elle fonctionne aussi lorsque la capture est désactivée ou que
l'application est compilée en Core.

L'enregistrement des réglages applique immédiatement la politique sauvegardée.
Après chaque capture, les dépassements de nombre et de taille déclenchent aussi
le nettoyage. Les politiques restent globales au journal local, tous workspaces
confondus : l'isolation des lectures et des suppressions manuelles ne crée pas
de budgets distincts.

Les conventions suivantes sont explicites :

- `retention_days = 0` conserve les événements sans limite d'âge.
- `max_file_size_mb = 0` désactive la limite de volume ; une unité vaut
  1 048 576 octets. Le panneau conserve ce réglage existant sans ajouter de champ.
- Une limite de nombre dépassée ramène la cible à 75 % de cette limite, comme
  la rotation précédente. La même marge s'applique à une limite de taille
  dépassée. `max_entries = 0` ne conserve aucune entrée.
- Un événement récent trop volumineux peut vider le journal : conserver des
  états plus anciens en omettant uniquement cet événement ferait passer un état
  périmé pour le dernier état connu. Les captures suivantes restent possibles.
- Les lignes non interprétables sont conservées par la purge d'âge, mais
  comptent dans les budgets de nombre et de taille et peuvent être retirées
  avec le préfixe du journal.

Une configuration présente mais illisible suspend les purges automatiques
jusqu'à un enregistrement valide. Un fichier absent utilise les valeurs par
défaut existantes. Le [store](../../src-tauri/src/time_travel/store.rs) écrit les
réglages par fichier temporaire, synchronisation et renommage avant de changer
la configuration en mémoire. Un échec de cette écriture ne change pas la
politique active. Les réglages et le journal sont deux fichiers : si les
réglages sont sauvegardés mais que la purge échoue, l'IPC retourne une erreur
et la politique sauvegardée reste disponible pour une nouvelle tentative.

La [carte de réglages](../../src/components/Settings/sections/DataSection.tsx)
conserve un brouillon et un bouton Enregistrer. Elle désactive ses contrôles
pendant le chargement et l'enregistrement, reprend la configuration retournée
par le backend en cas de succès, et affiche une erreur traduite avec possibilité
de réessayer. Les champs numériques passent par la validation du formulaire.
Les champs non affichés, notamment les colonnes sensibles, restent dans la
configuration envoyée. Les contrôles de licence backend sont conservés.

Pour respecter le budget de chargement initial, les dialogues IA d'explication
de schéma et de réécriture passent aussi en chargement à l'ouverture dans
[AppLayout](../../src/AppLayout.tsx) et
[QueryPanel](../../src/components/Query/QueryPanel.tsx). Leurs conditions
d'ouverture, paramètres et actions restent sur les mêmes chemins.

## Vérifications

Dix tests Rust ont été ajoutés, dont les quatre régressions initialement en
échec. Ils couvrent taille UTF-8, limites combinées, événement trop volumineux,
configuration illisible, erreurs de sauvegarde/réécriture, passage sans travail,
valeurs illimitées/extrêmes, nettoyage sans capture et reprise périodique après
erreur. Les tests existants de purge hors cache, de lignes anciennes et de
sérialisation avec les nouvelles captures passent aussi.

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
pnpm typecheck
pnpm test:ts src/lib/tauri/time-travel.test.ts
pnpm exec biome check src/AppLayout.tsx src/components/Query/QueryPanel.tsx src/components/Settings/sections/DataSection.tsx
pnpm exec vite --host 127.0.0.1 --port 1430 --strictPort
# Dans un autre terminal, pendant que Vite tourne :
node scripts/test-time-travel-settings-ui.mjs
# Après arrêt du serveur de test :
pnpm perf:bundle --baseline .perf/v0140-retention-before.json --output .perf/v0140-retention-after.json
pnpm docs:check
git diff --check
```

Les suites passent avec 586 tests Pro, 259 Core et quatre tests TypeScript,
sans test ignoré. Le typecheck et le formatage Rust ciblé passent. Biome ne
remonte aucune erreur ; trois avertissements existants restent présents sur
`refreshStats` dans la carte de cache voisine et deux dépendances `driver`
dans AppLayout. Cargo signale des artefacts de
compilation incrémentale corrompus dans ses dépendances locales ; les
compilations et les suites aboutissent malgré ces avertissements.

Les sept groupes du [parcours navigateur](../../scripts/test-time-travel-settings-ui.mjs)
passent : chargement, saisie de plusieurs exclusions, enregistrement explicite,
deux formes de refus, validation numérique, échec initial/réessai et fonction indisponible. Aucun
appel IPC de modification n'est émis pendant la saisie et aucune erreur de
page n'est observée. Le test emploie Playwright 1.62.1 et Chromium
153.0.8010.12 avec les variables documentées dans le
[guide de tests](../development/TESTING.md).

Un contrôle Chromium local complémentaire expose les objets `lazy` des modules
AppLayout et QueryPanel dans une fixture, sans monter toute l'application.
Les fournisseurs de licence/préférences sont simulés avec une IA non configurée.
Les deux modules de dialogue ne sont pas demandés avant leur ouverture ; chacun
s'affiche ensuite et se ferme avec Échap, sans erreur de page ni appel IPC.
Ce contrôle vérifie le chargement et la fermeture, pas une génération IA ni
l'application d'une requête réécrite dans l'éditeur.

La comparaison Vite avec la référence prise avant cette modification de
l'interface respecte les huit budgets :

| Mesure | Avant | Après | Écart |
| --- | ---: | ---: | ---: |
| JS initial brut | 2 439 897 | 2 435 378 | −4 519 |
| JS initial gzip | 708 359 | 707 591 | −768 |
| JS total brut | 4 790 860 | 4 794 036 | +3 176 |
| JS total gzip | 1 419 350 | 1 421 800 | +2 450 |
| CSS initial et total brut | 111 943 | 111 943 | 0 |
| CSS initial et total gzip | 17 491 | 17 491 | 0 |

Les valeurs sont en octets. La hausse de JS total reste inférieure à 0,173 %.
Le panneau de réglages est déjà chargé à la demande ; les deux dialogues IA
sortent de la fermeture statique initiale du manifeste. Ces mesures de poids
ne constituent pas une mesure de latence au démarrage.
Rapports et logs locaux : `.perf/v0140-retention-*`.

## Limites

Les essais de rétention utilisent des répertoires temporaires et des données
synthétiques. Aucun historique réel de l'utilisateur n'a été purgé pendant
cette vérification. Le parcours Chromium simule IPC et licence ; il ne valide
pas la fenêtre native, le transport Tauri ou le comportement sur Windows/macOS.
La tâche périodique est testée avec un intervalle court, sans recette native
de quinze minutes.

Le journal et les réglages ne forment pas une transaction unique. Une purge
empêchée par une erreur disque peut laisser le fichier au-dessus de sa limite
jusqu'à une tentative réussie. Le verrou protège les opérations du même store,
pas les éditions externes ou plusieurs processus écrivant dans le même fichier.
La purge se base sur la date des événements pour l'âge et leur ordre d'écriture
pour les budgets ; elle ne restaure pas les historiques déjà supprimés.

Le coût CPU et la latence des passages sur un gros journal, la mémoire native
et les autres critères de la release restent à mesurer. Le contrôle du bundle
ne couvre pas ces mesures. Les autres moteurs et la migration de masquage des
anciens fichiers ne sont pas qualifiés par ce lot.
