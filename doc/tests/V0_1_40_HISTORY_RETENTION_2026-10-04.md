# v0.1.40 — Historique conservé hors cache, 4 octobre 2026

Référence : `b151d2063c5bf116afff04f59f50b5a3f9b2ffaf`, avec les lots locaux
édition en place, captures, clés générées et sûreté transactionnelle conservés.
Ce complément concerne le journal desktop Pro et ses commandes IPC.

## Défauts reproduits

Cinq nouveaux tests échouaient avant correction :

- Un journal de 5 005 événements ne fournissait que les 5 000 présents en cache.
  Les compteurs, pages anciennes, recherches par identifiant, états et diffs
  ignoraient les événements évincés.
- La purge d'une entrée expirée en cache réécrivait uniquement ce cache,
  supprimant aussi les entrées encore valides qui en étaient sorties.
- Une entrée expirée uniquement sur disque échappait à cette purge.
- La rotation du fichier conservait en mémoire des événements déjà supprimés.
- Un événement ajouté tardivement avec un horodatage ancien prenait la place
  du plus récent dans les lectures annoncées comme chronologiques.

## Correction

Le [store](../../src-tauri/src/time_travel/store.rs) utilise le cache seulement
lorsqu'il représente tout le journal chargé avec succès. Sinon, il parcourt le
JSONL conservé. La pagination sélectionne des horodatages et positions avant
de copier les images de la page ; un grand offset ne conserve pas les images
ignorées. Le cache reste limité à 5 000 entrées.

Timeline et compteur partagent le même verrou et les mêmes filtres. L'ordre
est l'horodatage décroissant, puis l'ordre d'ajout décroissant en cas d'égalité.
Le diff conserve la première image avant et la dernière image après de chaque
clé, même si les événements traversent la limite du cache ou arrivent hors
ordre. Les images incomplètes restent signalées.

Les règles courantes de masquage et l'identité de connexion s'appliquent aussi
aux entrées lues sur disque. Le rollback sélectionné retrouve une ancienne
entrée par son UUID. Le rollback global refuse plus de 10 000 changements avant
de charger leurs images : il ne génère plus un script partiel silencieux. Sa
borne temporelle exclut les événements exactement à l'instant cible.

Rotation, suppression ciblée et purge parcourent le journal complet et écrivent
un fichier temporaire synchronisé, remplacé par renommage. Le cache et les
compteurs changent après réussite. Les lignes anciennes non interprétables sont
préservées par la purge et la suppression ciblée ; la rotation par nombre de
lignes peut les retirer avec le préfixe ancien. Une suppression globale échouée
ne vide plus le cache et ne renvoie plus de succès.

Les erreurs de lecture remontent aux commandes au lieu de produire un historique
partiel réussi. Les [commandes](../../src-tauri/src/commands/time_travel.rs)
déportent les lectures, générations SQL et suppressions sur le pool bloquant.
Les contrôles de licence, connexion et confirmation de suppression restent dans
leur chemin existant. Les réponses IPC ne changent pas de forme.

## Vérification

Treize tests ajoutés couvrent les cinq défauts initiaux, l'isolation et le
masquage hors cache, le diff entre événements anciens et récents, une erreur de
lecture après un premier enregistrement valide, les échecs de réécriture, les
lignes anciennes, la rétention illimitée, le seuil de rollback, une capture
concurrente avec suppression ciblée et la capacité configurée par défaut.

```bash
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --features pro --lib
cargo test --manifest-path src-tauri/Cargo.toml -p qoredb --lib
rustfmt --edition 2024 --check src-tauri/src/time_travel/store.rs src-tauri/src/time_travel/capture.rs src-tauri/src/commands/time_travel.rs
pnpm docs:check
git diff --check
```

Les suites passent avec 568 tests Pro et 242 tests Core, sans test ignoré.
La sélection Time Travel comprend 57 tests. Formatage ciblé, `docs:check` et
contrôle du diff passent également. Les fixtures utilisent des fichiers temporaires et des valeurs
synthétiques. Les tests existants de capture SQLite restent dans la suite Pro.
La matrice de bases distantes et le parcours Tauri natif n'ont pas été relancés.

Une exécution isolée du test `retained_history_default_capacity_keeps_cache_bounded`
sur Linux x86_64, binaire de test debug Rust 1.94.0, donne :

| Mesure | Valeur |
| --- | --- |
| Journal synthétique | 50 000 événements, 26 727 780 octets |
| Chargement initial du cache | 174 ms |
| Page de 50 événements à l'offset 49 950, avec compteur complet | 312 ms |
| Diff de 50 000 clés, statistiques complètes et sortie limitée à 50 lignes | 970 ms |
| RSS maximal du processus de test | 118 176 Kio |

Le processus est lancé directement depuis Python et mesuré avec `resource`, sans
inclure Cargo ni la compilation dans le RSS. Il s'agit d'une seule observation
sur fichiers synthétiques et cache système non contrôlé, pas d'une médiane/p95,
d'un budget de release ou d'une mesure WebView. Les journaux locaux sont dans
`.perf/v0140-history-*.log` et ne sont pas requis pour exécuter les tests.

## Limites

Les lectures hors cache restent des parcours séquentiels du fichier ; une page
effectue deux passages. Le verrou sérialise aussi les captures concurrentes avec
ces lectures. La mémoire du diff dépend du nombre de clés et de la taille des
images agrégées ; limiter les lignes retournées ne borne pas cette agrégation.

L'export respecte toujours la limite demandée ; l'interface demande 10 000
événements. Les lignes JSON non reconnues restent exclues des lectures, sans
certification de leur contenu. Les entrées déjà retirées par rétention ne sont
pas récupérables et les anciennes images ne sont pas certifiées rétroactivement.

La fonction de purge par durée est corrigée mais n'est toujours pas planifiée
par l'application ; `max_file_size_mb` n'est pas encore appliqué. La rotation
existante par `max_entries` reste active lors des captures. Ces politiques,
l'isolation par workspace, les très grandes images et la recette native restent
à qualifier. Aucune source frontend ne change dans ce complément ; la mesure
précédente du bundle n'a pas été relancée.
