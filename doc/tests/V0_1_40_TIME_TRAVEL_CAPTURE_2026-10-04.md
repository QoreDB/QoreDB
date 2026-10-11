# v0.1.40 — Fidélité des captures Time Travel, 4 octobre 2026

Référence : `b151d2063c5bf116afff04f59f50b5a3f9b2ffaf`, avec le lot local
[édition en place](V0_1_40_INLINE_EDIT_2026-10-04.md) déjà présent et conservé.
Ce lot poursuit la phase 1 du [plan](../todo/V0_1_40.md), sans clôturer la recette
native ni la qualification de release.

Le [complément suivant](V0_1_40_GENERATED_KEYS_2026-10-04.md) traite les clés
générées SQLite/PostgreSQL et résorbe le dépassement frontend consigné ici.
Les résultats ci-dessous restent ceux de ce lot avant ce complément.

## Défauts et correction

Une régression SQLite a d'abord échoué dans l'ancien `fetch_row_by_pk` : un
sélecteur correspondant à deux lignes produisait l'image de la première.
Les captures UPDATE reconstruisaient aussi leur état final depuis la saisie,
les INSERT utilisaient toutes les valeurs saisies comme clé, et les lots Sandbox
reprenaient les anciennes valeurs fournies par l'interface.

Le [service de capture](../../src-tauri/crates/qore-service/src/mutation/capture.rs)
vérifie les colonnes de clé primaire dans le schéma et relit directement le
driver, sans cache ni comptage. Une lecture demande au plus deux lignes pour
refuser un résultat ambigu. Les lectures de schéma et d'image ont chacune un
plafond de deux secondes. Aucune lecture de récupération ne réémet une écriture.

Les mutations desktop utilisent ce service avant et après l'écriture. Le chemin
Sandbox l'appelle autour de chaque mutation : deux modifications successives
d'une ligne gardent leurs images intermédiaires réelles. Les images ne sont
publiées dans le journal qu'après confirmation du commit, ou pour les seuls
succès d'un lot explicitement non transactionnel. Un rollback ou un commit
incertain supprime les images en attente.

Une clé absente, incomplète, générée mais non retournée, modifiée explicitement
ou introuvable après un trigger n'est pas devinée. L'événement reste visible sans
clé exploitable. Une capture manquante ne reprend jamais `old_values` ou
`new_values` de l'interface. Les écritures unitaires ayant affecté zéro ligne ne
produisent pas de capture. Un résultat multi-lignes ou sans compteur confirmé
ne produit pas d'image présentée comme celle d'une seule ligne.

Le rollback refuse une clé vide et un UPDATE sans image finale, avec un message
traduit dans les neuf locales. Une écriture non identifiable empêche aussi de
présenter une image antérieure comme l'état courant de la ligne concernée.
Une nouvelle image vérifiable permet de reprendre cette reconstruction.

Les images internes ne sont pas sérialisées dans `ApplyBatchResult`. Le stockage
applique toujours les règles de masquage courantes et la liste de colonnes
sensibles avant écriture sur disque. Les helpers et tests de capture partagés
restent sous BUSL-1.1 ; les contrôles de mutation restent dans le service commun.

Les images retenues sont plafonnées à 8 Mio de données JSON par capture et au
total par lot. Le comptage ne construit pas une seconde copie JSON en mémoire.
Au-delà, les événements confirmés restent sans image exploitable. Ce plafond
ne borne ni la mémoire transitoire du driver lisant une grosse ligne ni le RSS.

## Scénarios

| Scénario | Preuve | Résultat |
| --- | --- | --- |
| Sélecteur non unique et clé composite incomplète | Régression échouant avant correction ; SQLite réel | Réussi |
| INSERT explicite, valeurs par défaut et triggers INSERT/UPDATE | Images relues dans un fichier SQLite temporaire | Réussi |
| Clé canonique après coercition et entier `9007199254740993` | SQLite réel ; clé composite et clé texte convertie en entier | Réussi |
| Clé générée, clé déplacée et DELETE recréé par trigger | Événements incomplets sans cible de rollback inventée | Réussi |
| Zéro ligne touchée et relecture impossible | SQLite réel ; suppression de table entre lectures | Réussi |
| Plusieurs écritures sur une même ligne dans un lot | Images intermédiaires vérifiées malgré des `old_values` périmés | Réussi |
| Rollback, perte de réponse au commit et échec non transactionnel | SQLite et injection de panne ; seules les captures confirmées sont conservées | Réussi |
| Capture facultative et garde lecture seule | Service partagé et SQLite | Réussi |
| Gros enregistrements et budget global du lot | Fixtures SQLite de plusieurs Mio ; événements confirmés conservés | Réussi |
| Confidentialité IPC et journal, puis exécution du rollback | Images absentes de la réponse sérialisée ; secret synthétique masqué sur disque ; restauration SQLite | Réussi |
| État historique après une capture non identifiable | Test du store, puis nouvelle image vérifiée | Réussi |
| Message de refus en français et anglais | Composant réel dans Chromium ; résultat de rollback simulé, sans IPC | Réussi |
| IPC et rendu Tauri ; moteurs hors SQLite | Aucun pont natif connecté et aucun service externe utilisé | Non exécuté |

## Commandes

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qore-service --no-default-features --features driver-sqlite --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
pnpm test:ts src/lib/tauri/time-travel.test.ts
pnpm typecheck
pnpm exec biome check src/components/TimeTravel/RollbackDialog.tsx src/locales/*.json
pnpm docs:check
git diff --check
```

Le formatage Rust est vérifié sur les fichiers modifiés uniquement. Biome garde
un avertissement préexistant sur la clé d'index de la liste des avertissements.

Résultats des suites : 229 tests desktop Core, 555 desktop Pro et 4 tests
TypeScript du transport Time Travel, sans test ignoré dans ces sélections.
TypeScript et les contrôles de documentation passent. La suite service SQLite
comprend 190 tests, dont 13 nouveaux scénarios de capture et de lots.

Le contrôle visuel utilise Playwright 1.62.1 et Chromium 153.0.8010.12, sous
Linux, avec le composant `RollbackDialog` et les traductions réels. Le résultat
de rollback est simulé ; aucune connexion ni donnée utilisateur n'est utilisée.
Les captures locales sont `.perf/v0140-capture-fr.png` et
`.perf/v0140-capture-en.png`. La fixture ponctuelle `.perf/check-capture-ui.mjs`
est ignorée ; ce contrôle ne constitue pas une nouvelle suite navigateur
versionnée ni une preuve de rendu WebView Tauri.

## Poids frontend

```bash
pnpm perf:bundle --output .perf/v0140-capture-before.json
pnpm perf:bundle --baseline .perf/v0140-capture-before.json --output .perf/v0140-capture-after.json
```

| Mesure | Avant ce lot | Après | Écart |
| --- | ---: | ---: | ---: |
| JS initial brut | 2 450 665 | 2 450 781 | +116 |
| JS initial gzip | 709 711 | 709 751 | +40 |
| JS total brut | 4 785 913 | 4 787 170 | +1 257 |
| JS total gzip | 1 415 449 | 1 415 889 | +440 |
| CSS initial/total brut | 111 943 | 111 943 | 0 |
| CSS initial/total gzip | 17 491 | 17 491 | 0 |

La comparaison immédiate échoue sur le budget de zéro croissance initiale ;
le message traduit ajoute 116 octets bruts / 40 gzip au JS initial. Aucune
exception n'est considérée comme approuvée. Une comparaison distincte avec la
référence déjà établie avant le lot d'édition en place
(`.perf/v0140-inline-before.json`) passe : −8 969 octets initiaux bruts / −460
gzip, et moins de 2 % de croissance totale. Les métadonnées sont comparables ;
cette seconde comparaison ne remplace pas le résultat du présent lot.

## Limites restantes

Les identifiants générés absents du résultat des drivers restent indisponibles :
leur restitution fiable nécessite un contrat de mutation adapté, pas une
recherche sur les seules valeurs saisies. Les clés NULL, flottantes, binaires ou
composées de valeurs structurées ne sont pas admises par cette capture prudente.

Les lectures avant/après ne constituent pas une isolation de snapshot. Les
écritures externes ou concurrentes dans la même session, les effets de triggers
sur d'autres lignes et les triggers différés au commit restent à qualifier.
Le SQL de rollback reste soumis à revue : colonnes générées, contraintes et
effets de triggers lors de sa réexécution dépendent du moteur.

Les anciennes images déjà persistées ne sont ni réparées ni certifiées par ce
lot ; leurs clés et valeurs potentiellement issues de la saisie restent une
limite historique. Les anciens fichiers en clair ne sont pas purgés.
L'isolation par workspace, la couverture hors des 5 000 entrées du cache,
les autres moteurs et plateformes, les performances natives et la recette
Tauri restent ouverts. Les adaptations HTTP/CLI/MCP ne demandent pas de capture
et conservent l'entrée `apply_batch` sans lecture d'historique supplémentaire.
