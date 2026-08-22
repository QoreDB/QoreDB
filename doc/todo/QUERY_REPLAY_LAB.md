# Query Replay Lab

Statut : livré en v0.1.38 (verticales 1 à 3).
Tier : Pro (`BUSL-1.1`), clé de licence `query_replay`.

La fonctionnalité répond à une question qu'aucun client desktop ne traite : « ma migration
a-t-elle cassé quelque chose ? ». On enregistre un jeu de requêtes pendant une session de
travail, on le rejoue après la migration ou sur un autre environnement, et on lit un rapport
des écarts.

## 1. Ce qui est livré

**Enregistrer.** Un recorder observe ce que l'intercepteur voit déjà, au point où une requête
se termine. `execute_query` sert aussi les modales DDL, les statistiques du navigateur, le
générateur de données et les sources de diff : l'appelant déclare donc explicitement si son
exécution a sa place dans un enregistrement (`recordable`), et ce qui n'est pas marqué reste
dehors. Seuls l'éditeur et les notebooks le marquent.

**Rejouer.** Le jeu est ré-exécuté séquentiellement sur la connexion active, par le même
chemin de preflight et d'exécution qu'une requête utilisateur, marqué `QuerySource::Replay`.
Audit, règles de sûreté et rate limiting s'appliquent sans chemin parallèle.

**Rapporter.** Chaque entrée reçoit un verdict : identique, cassée, écart de nombre de
lignes, écart de contenu, plus lente, ignorée. Depuis une ligne dont les lignes ont été
capturées, le diff baseline ↔ rejeu s'ouvre dans `DataDiffViewer`.

## 2. Décisions structurantes

### Le partageable et le local

Ce qui est séparé, ce n'est pas les données des métadonnées, c'est le partageable du local.

| Artefact              | Emplacement                             | Contenu                                    | Git       |
| --------------------- | --------------------------------------- | ------------------------------------------ | --------- |
| Définition du jeu     | `.qoredb/replays/<slug>.qreplay.json`   | Requêtes et attentes (durée, lignes, digest) | Versionné |
| Captures de résultats | `data_dir/replays/<run_id>/`            | Lignes de la baseline et de chaque rejeu   | Jamais    |

Le jeu est le « test » : partageable avec l'équipe, sans une seule **ligne de résultat** — un
test le vérifie en relisant le fichier produit. Les captures sont la « preuve » : elles
vivent dans le répertoire applicatif, comme les snapshots, et ne traversent jamais le dépôt.
Précédent dans le produit : les baselines de schéma sont déjà git-ignorées pour la même
raison.

**Ce qui est versionné en revanche, c'est le texte des requêtes, tel quel.** Une requête
rejouable ne peut pas être expurgée : un littéral qu'elle contient — `WHERE api_key = '…'`,
un `AUTH`, une charge Redis — part dans le dépôt avec elle. C'est le même compromis que la
query library, déjà versionnée avec du SQL brut, et l'interface le dit avant de démarrer un
enregistrement. Deux tests fixent la frontière : l'un vérifie qu'aucune ligne de résultat
n'atteint le fichier, l'autre qu'un littéral de requête y est bien conservé — pour que le
premier ne se lise pas comme une garantie plus large qu'elle ne l'est.

Le digest est un SHA-256 déterministe des lignes. Sur un résultat court à faible entropie, il
reste attaquable par dictionnaire ; il ne vaut pas anonymisation.

### Le rejeu est lié à sa connexion et à son workspace

L'enregistrement retient le `session_id` de départ : une requête exécutée sur une autre
connexion, dans un autre onglet, est comptée et écartée. Sans cela, un enregistrement démarré
sur une base de développement capturerait les valeurs d'une requête de production sous la
politique du développement.

Les captures vivent sous `data_dir/replays/<project_id>/`. Deux workspaces peuvent porter un
jeu de même nom ; ni l'un ni l'autre ne doit voir — ou supprimer — les rejeux de l'autre.

### Les résultats sont capturés

Le scénario central est temporel : on enregistre avant la migration, on rejoue après. L'état
« avant » n'est plus reproductible une fois la migration appliquée ; ne pas l'avoir gardé
signifie ne jamais pouvoir montrer l'écart, seulement l'annoncer. Un rapport qui dit « 34
requêtes ont un contenu différent » sans montrer quoi est inexploitable, d'autant qu'une part
de ces écarts est du bruit attendu (`updated_at`, `now()`, séquences).

Les captures réutilisent le format `Snapshot` (`meta` + `rows`), donc `to_query_result()`
réalimente `DataDiffViewer` sans conversion.

### Garde-fous de la capture

- borne par requête : 1 000 lignes, même défaut que le Visual Data Diff, réglable ;
- budget d'octets par rejeu (64 Mio par défaut), appliqué **avant** l'écriture : une entrée qui
  n'y tient pas n'est pas écrite du tout, plutôt que de dépasser de sa taille entière. Le
  rapport le dit ; les entrées continuent d'être enregistrées, seules leurs lignes manquent ;
- rétention : les N derniers rejeux, purge automatique des plus anciens — la baseline est
  toujours conservée, sans quoi les rejeux suivants n'auraient plus rien à comparer ;
- production : capture de valeurs désactivée par défaut à l'enregistrement, activable
  explicitement ; un rejeu contre la production n'écrit jamais de lignes ;
- mode « métadonnées seules » disponible, avec sa conséquence annoncée dans l'interface.

### Colonnes ignorées par jeu

Sans cela, toute table portant un `updated_at` sort en écart à chaque rejeu et le rapport
devient du bruit. La liste est définie sur le jeu, donc versionnée avec lui, et la
comparaison est insensible à la casse.

Modifier la liste après coup **recalcule les empreintes attendues** depuis la capture de
référence. Sans ce recalcul, l'attente resterait celle de l'ancienne liste et une requête
inchangée sortirait en écart de contenu — la correction serait pire que le bruit. Quand il n'y
a pas de capture d'où recalculer, l'empreinte est retirée et la comparaison retombe sur les
métadonnées.

### Le rejeu est une lecture par défaut

Les mutations sont exclues du rejeu sauf activation explicite, et le backend refuse le rejeu
de mutations en production sans possibilité de confirmation — même règle que l'agent IA.

**La classification qui fait foi est celle du preflight, jamais celle du fichier.**
`.qreplay.json` est versionné et éditable à la main : un `INSERT` déclaré `"is_mutation":
false` y serait sinon exécuté, en production comprise, puisque le preflight n'y bloque que les
requêtes dangereuses. Le contrôle a donc lieu après le preflight, sur la valeur qu'il calcule
depuis le texte de la requête. Deux tests le vérifient avec un booléen volontairement falsifié.

Un preflight refusé est classé « ignorée », pas « cassée » : c'est QoreDB qui décline, pas la
base qui répond autrement.

### Le digest détecte, la capture explique

Hash SHA-256 des lignes, calculé après tri canonique — l'ordre n'est pas garanti sans
`ORDER BY` — et après retrait des colonnes ignorées, borné au même nombre de lignes que la
capture. Chaque champ est préfixé de sa longueur, sinon `["ab", "c"]` et `["a", "bc"]`
donneraient le même hash. Les noms de colonnes retenues entrent aussi dans le hash, donc un
renommage se voit. Le digest donne la détection compacte, fonctionne en mode métadonnées
seules, et sert de test rapide avant d'ouvrir le diff. Au-delà de la borne, le rapport dit
que la comparaison est partielle plutôt que de prétendre à l'exhaustivité.

### Seuil de dégradation temporelle

Une requête est « plus lente » seulement si elle l'est relativement **et** absolument :
facteur ×2 et plus de 100 ms d'écart par défaut. Sans les deux conditions, une requête de
4 ms passant à 9 ms sortirait en régression alors que c'est du bruit d'ordonnancement.

## 3. Formats

Définition du jeu — `.qoredb/replays/<slug>.qreplay.json`, cohérent avec `.qoredb/migrations/` :

```json
{
  "version": 1,
  "name": "checkout flow",
  "created_at": "2026-08-21T10:00:00Z",
  "source": { "driver_id": "postgres", "connection_label": "staging", "environment": "staging" },
  "ignored_columns": ["updated_at", "last_seen_at"],
  "entries": [
    {
      "id": "uuid",
      "order": 1,
      "query": "SELECT …",
      "driver_id": "postgres",
      "namespace": null,
      "operation_type": "select",
      "is_mutation": false,
      "expected": {
        "execution_time_ms": 12.4,
        "row_count": 42,
        "success": true,
        "fingerprint": "…",
        "result_digest": "sha256:…"
      }
    }
  ]
}
```

Captures — `data_dir/replays/<run_id>/<entry_id>.json` au format `Snapshot`. Un `run.json`
par rejeu porte l'horodatage, la connexion cible, le mode de capture retenu et la raison d'un
éventuel arrêt de capture. Le rejeu d'enregistrement est un rejeu comme les autres, marqué
comme baseline.

## 4. Fichiers

Backend (`BUSL-1.1`) : `src-tauri/src/replay/{mod,types,digest,store,capture,recorder,runner,compare}.rs`,
`src-tauri/src/commands/replay.rs`, `src-tauri/tests/replay_e2e.rs`. Le module est derrière
`#[cfg(feature = "pro")]`.

Backend (`Apache-2.0`) : `QuerySource::Replay` dans
`qore-service/src/interceptor/types.rs` — variante additive, aucun `match` exhaustif à
étendre — et le branchement du recorder dans le `on_complete` de `execute_query`.

Front (`BUSL-1.1`) : `src/components/Replay/*`, `src/lib/replay.ts`, `src/hooks/useReplay.ts`,
type d'onglet `replay` gardé par `LicenseGate`, et un indicateur d'enregistrement dans la
status bar qui n'interroge le backend que sous licence.

Quatre chemins mènent à la fonctionnalité : le modal « What's New » à la mise à jour, le
dropdown « + » de la barre d'onglets, la palette (`cmd_open_replay`, `feat_replay`) et le
panneau de découverte Pro de la sidebar. Sans licence, l'onglet affiche l'`UpgradePrompt`.

## 5. Tests

- cargo, digest : stabilité sous permutation de lignes, effet des colonnes ignorées, bornage
  qui reste insensible à l'ordre d'entrée, non-collision des frontières de champs, détection
  d'un renommage de colonne.
- cargo, classification : priorité cassée > lignes > contenu > plus lente, deux échecs
  identiques ne sont pas une régression, un ralentissement faible en absolu est du bruit.
- cargo, stores : aller-retour disque, refus des chemins traversants, purge qui conserve la
  baseline, et surtout : aucune valeur de données n'atteint `.qoredb/`.
- cargo, recorder : ordre et digest des entrées, budget qui arrête la capture en le disant,
  production qui dégrade la capture en métadonnées seules.
- cargo, runner contre `MockDriver` : rejeu identique, colonne renommée qui casse une entrée,
  contenu changé à nombre de lignes égal, mutations ignorées hors activation, mutations
  refusées en production même activées, production qui n'écrit aucune ligne, annulation.
- cargo, A/B : comparaison des deux côtés vivants, diff offert seulement si les deux ont
  capturé, entrée ignorée d'un côté ignorée des deux.
- cargo, bout en bout contre un vrai PostgreSQL (`tests/replay_e2e.rs`, exécuté par
  `build-pro.yml`) : enregistrer, renommer une colonne, rejouer et obtenir « cassée » sur la
  seule requête concernée ; écart de contenu à nombre de lignes égal, silencé par une colonne
  ignorée ; et une valeur présente dans la capture locale absente du jeu sérialisé.

## 6. Mode A/B deux connexions

Le même jeu est rejoué sur deux connexions actives, et les deux résultats sont comparés l'un
à l'autre plutôt qu'à l'enregistrement. Les deux côtés étant vivants, la comparaison tient là
où une référence enregistrée ne suffirait pas — comparer la production à une staging migrée,
par exemple.

La classification est celle du rejeu simple : `compare_sides` reconstruit une attente à partir
du côté gauche et réutilise `classify`. Le diff ne s'ouvre que si les deux côtés ont capturé
leurs lignes, et le backend refuse deux fois la même connexion.

**Les exclusions sont décidées avant toute exécution**, pour les deux côtés à la fois, depuis
le texte des requêtes et l'environnement de chaque connexion. Les décider après coup laisserait
une mutation s'exécuter d'un côté avant d'être étiquetée « ignorée » parce que l'autre l'avait
refusée. Le garde du preflight, dans chaque rejeu, reste le dernier mot.

La comparaison est persistée à côté de son rejeu de droite, donc rouvrir l'onglet la retrouve
telle quelle plutôt que de la présenter comme un rejeu contre l'enregistrement.

Dans l'interface, le sélecteur de comparaison de l'en-tête liste, au même endroit,
l'enregistrement, les rejeux précédents du jeu et les autres connexions ouvertes. Choisir un
rejeu précédent **rebase les attentes sur ce qu'il a observé** : sans cela, le tableau
classerait contre l'enregistrement d'origine pendant que le diff ouvrirait un autre rejeu.

## 7. Reste ouvert

- Version avancée listée dans `doc/todo/v3.md` : analyse sémantique des résultats, scoring de
  régression, intégration CI/CD.
- Un seul enregistrement et un seul rejeu à la fois, globalement — pas par connexion. Le choix
  est délibéré : un compteur d'enregistrement par connexion demanderait de désigner laquelle
  enregistre, pour un scénario que personne n'a demandé.
