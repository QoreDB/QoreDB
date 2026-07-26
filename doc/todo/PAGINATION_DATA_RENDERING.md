# Plan produit — Pagination et rendu des données

> Statut au 25 juillet 2026
>
> - Verticale 1 — chargement sans comptage bloquant : livrée, correctifs appliqués, dette de contrat résorbée
> - Verticale 2 — total honnête et opérations annulables : livrée
> - Verticale 3 — coût du tri et de la recherche : livrée
> - Verticale 4 — pagination stable par curseur : livrée, sauf l'édition sous curseur (9.6) et le forçage de stratégie (9.7)
> - Verticale 5 — mémoire et rendu progressif : à concevoir
> - Verticale 6 — cohérence et sécurité : transverse
>
> Le découpage a été révisé après audit du code livré. Correspondance avec la
> version précédente : l'ancienne verticale 2 (curseur) devient la 4, l'ancienne
> 3 (mémoire) devient la 5, l'ancienne 4 (cohérence) devient la 6. Les
> verticales 2 et 3 sont nouvelles : elles couvrent deux coûts que le plan
> initial ne traitait pas alors qu'ils dominent le ressenti utilisateur.

## 1. Vision produit

L'objectif n'est pas de proposer le plus grand nombre d'options possible, mais
de rendre l'exploration des données rapide, prévisible et cohérente sur tous les
moteurs supportés.

Trois principes :

1. fournir le meilleur compromis par défaut ;
2. ne jamais bloquer ou ralentir l'application de manière inattendue ;
3. laisser un choix explicite à l'utilisateur uniquement lorsqu'il comprend la
   conséquence de ce choix et peut agir utilement.

Le produit doit éviter :

- un `COUNT(*)` implicite avant l'affichage de la première page ;
- une interface qui prétend connaître un total lorsqu'il ne s'agit que d'une
  estimation ou d'une borne inférieure ;
- une opération lourde qu'on ne peut pas arrêter ;
- des pages profondes de plus en plus lentes sans explication ;
- des doublons ou des lignes manquantes lors de mutations concurrentes ;
- une consommation mémoire qui augmente sans limite pendant le scroll ;
- des comportements différents et non documentés selon le driver ;
- la disparition des données déjà affichées lorsqu'une opération secondaire
  échoue.

## 2. Non-objectifs

Décisions prises une fois pour toutes, à ne pas rouvrir à chaque verticale.

| Non-objectif                                                                     | Raison                                                                                                         |
| -------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------- |
| Sauter à la page N arbitraire en pagination par curseur                          | Incompatible avec le keyset ; le besoin réel est « aller à une valeur », pas « aller à un rang »               |
| Garantir une cohérence transactionnelle par défaut                               | Coût de connexion et de verrous disproportionné pour de la navigation ; réservé aux workflows qui le demandent |
| Afficher un total exact systématiquement                                         | C'est le coût qu'on cherche justement à supprimer                                                              |
| Rendre `OFFSET` performant sur les pages profondes                               | Impossible côté client ; la réponse est le curseur, pas l'optimisation                                         |
| Unifier les garanties d'ordre entre moteurs relationnels et `SCAN` Redis         | Techniquement impossible ; la réponse est une capacité déclarée, pas une abstraction qui ment                  |
| Charger l'intégralité d'une table en mémoire pour un tri ou une recherche client | Ne passe pas l'échelle ; le tri et la recherche restent serveur                                                |

## 3. Inventaire de l'existant

Établi par lecture du code au 24 juillet 2026. Toute décision ci-dessous s'y
réfère plutôt qu'à des suppositions.

### 3.1 Où le produit affiche un nombre de lignes

| Emplacement                | Source                                                         | Nature                                                   |
| -------------------------- | -------------------------------------------------------------- | -------------------------------------------------------- |
| `DataGridStatusBar`        | `useInfiniteTableData`                                         | Lignes chargées, plus total exact après action explicite |
| `DocumentResults` (entête) | `infiniteScrollLoadedRows`, ou le total exact une fois calculé | Lignes chargées tant que le total est inconnu            |
| Onglet Info, tous moteurs  | `query_table` en `count_mode: estimated`                       | Total étiqueté, exact ou estimé selon le moteur          |

Cet inventaire décrivait trois chemins divergents, dont deux affichaient une
estimation moteur sans jamais l'indiquer. Ils sont unifiés depuis la
verticale 2.

### 3.2 Chemins d'appel de `query_table`

- Navigateur de tables (`useInfiniteTableData`) : envoie `count_mode: none`.
- Bridge HTTP (`qore-server/src/routes/bridge.rs:176`) : seul appelant hors
  application ; transmet les options du client sans les restreindre.
- La CLI et le serveur MCP n'appellent pas `query_table`.

Conséquence : la « migration progressive des consommateurs » évoquée
initialement se réduit à une seule décision, celle du bridge.

### 3.3 Limites en vigueur

| Limite                                           | Valeur                       | Portée                                                            |
| ------------------------------------------------ | ---------------------------- | ----------------------------------------------------------------- |
| `page_size` (`types.rs:1159`)                    | `clamp(1, 10000)`, défaut 50 | Contrat QoreDB. `fetch_size()` peut donc valoir 10001             |
| `policy.max_result_rows`                         | `None` par défaut            | Aucun plafond de lignes tant que l'utilisateur n'en configure pas |
| `policy.max_query_duration_ms`                   | `None` par défaut            | Idem pour le temps                                                |
| `index.max_result_window`                        | 10000, côté cluster          | Limite Elasticsearch et OpenSearch sur `from + size`              |
| `STREAM_SIZE_THRESHOLD` (`search_compat.rs:604`) | 10000                        | Seuil interne : au-delà, `_search` passe en PIT + `search_after`  |

Les trois valeurs à 10000 sont indépendantes ; leur égalité est une
coïncidence, sauf pour la dernière qui dérive de `max_result_window`.

### 3.4 Capacités déjà présentes et sous-exploitées

- `CancelSupport` est déclaré par chaque driver (`Driver` pour DuckDB et les
  moteurs SQL, `BestEffort` pour MongoDB et les moteurs Search). L'annulation
  d'un comptage ne demande donc pas de nouvelle abstraction.
- `search_compat` implémente déjà PIT + `search_after` avec tie-breaker
  `_shard_doc` pour le streaming (`stream_search`). La pagination par curseur
  Elasticsearch consiste à brancher `query_table` dessus, pas à repartir de zéro.
- Les drivers SQL portent un `transaction_conn` : paginer dans une transaction
  ouverte fournit déjà un snapshot, ce qui correspond au niveau 3 de la
  section 11.2.
- `total_pages` n'est consommé nulle part, ni en Rust ni en TypeScript. La
  pagination numérotée (`DataGridPagination`) est purement client-side, sur les
  lignes déjà chargées. Le champ est mort.

## 4. Décisions structurantes

Chaque décision porte son coût. La colonne « Réponse produit » est ce qui rend
la contrepartie acceptable ; sans elle, la décision n'est pas prise.

| Décision                                                         | Gains                                                                            | Contreparties                                                                      | Réponse produit                                                                                         | Statut                           |
| ---------------------------------------------------------------- | -------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | -------------------------------- |
| Ne plus calculer le total au chargement standard                 | Première page plus rapide, un aller-retour en moins, moins de charge sur la base | Le total exact n'est pas immédiatement connu                                       | Afficher les lignes chargées, puis un ordre de grandeur estimé, et un calcul exact explicite            | Livré, estimation en verticale 2 |
| Sur-lire une ligne pour produire `has_more`                      | Détection fiable de la page suivante sans comptage séparé                        | Une ligne de plus est lue, et le budget de lignes peut être dépassé d'une unité    | Coût borné, invisible, sauf là où le moteur impose une fenêtre : la sur-lecture doit alors être clampée | Livré, clamp inclus              |
| Calculer le total exact à la demande                             | L'utilisateur garde le contrôle ; aucun coût pour ceux qui n'en ont pas besoin   | Le comptage peut rester long sur une grande table                                  | Action non bloquante, annulable, avec timeout par défaut, erreur non destructive                        | Livré, annulation en verticale 2 |
| Afficher une estimation moteur                                   | Un ordre de grandeur immédiat, coût quasi nul, meilleure réponse que le silence  | Un nombre approché peut être pris pour un total                                    | Provenance et fraîcheur affichées, marqueur d'imprécision systématique, jamais un nombre nu             | Verticale 2                      |
| Rendre toute opération lourde annulable                          | L'utilisateur n'est jamais captif d'une action déclenchée par erreur             | Un chemin d'annulation par driver, avec des garanties inégales                     | S'appuyer sur `CancelSupport` existant et déclarer honnêtement le niveau réel                           | Verticale 2                      |
| Traiter le coût du tri et de la recherche                        | C'est le premier coût ressenti, avant la pagination profonde                     | Certaines colonnes deviennent explicitement coûteuses à trier ou chercher          | Capacité déclarée, avertissement avant exécution, proposition d'index                                   | Verticale 3                      |
| Utiliser une pagination par curseur quand un ordre stable existe | Coût des pages profondes quasi constant, meilleure stabilité sous mutations      | Pas de saut arbitraire vers la page N ; implémentation spécifique par moteur       | Choix automatique par capacité, fallback explicite vers `OFFSET`                                        | Verticale 4                      |
| Borner les lignes conservées par l'interface                     | Mémoire stable pendant les longues sessions                                      | Les lignes anciennes doivent être rechargées, et la sélection perd son référentiel | Fenêtre glissante sur curseurs, sélection redéfinie comme prédicat, export relu depuis la source        | Verticale 5                      |
| Limiter et charger progressivement les cellules lourdes          | Moins de mémoire, de sérialisation et de blocage du rendu                        | Le contenu complet nécessite une action supplémentaire                             | Preview claire, taille affichée, chargement complet à la demande                                        | Verticale 5                      |
| Uniformiser le contrat sans masquer les différences moteur       | UX cohérente et API plus simple                                                  | Toutes les garanties ne sont pas possibles partout                                 | Matrice de capacités et fallback honnête, jamais de fausse garantie                                     | Transverse                       |

## 5. Objectifs mesurables

Aucune mesure de référence n'existe aujourd'hui. Le premier jalon de la
verticale 2 est de produire ces mesures ; les seuils ci-dessous sont des cibles
à confirmer, puis à figer comme critère de non-régression.

| Indicateur                              | Cible        | Conditions                                           |
| --------------------------------------- | ------------ | ---------------------------------------------------- |
| Temps jusqu'à la première page          | p95 < 400 ms | Table de 10 M lignes, sans filtre, réseau local      |
| Temps d'une page suivante               | p95 < 250 ms | Même table, scroll continu                           |
| Écart page 1 / page 100                 | facteur < 2  | Avec une stratégie de curseur disponible             |
| Allers-retours par page                 | 1            | Sans recherche active ; 2 aujourd'hui avec recherche |
| Mémoire par onglet                      | < 250 Mo     | 100 000 lignes parcourues, 20 colonnes               |
| Première frappe de recherche            | p95 < 800 ms | Table de 1 M lignes, 20 colonnes                     |
| Taux de fallback curseur vers `OFFSET`  | < 20 %       | Sur un échantillon de schémas réels                  |
| Comptages exacts abandonnés par timeout | < 5 %        | Sinon le timeout est mal calibré                     |

## 6. Verticale 1 — Chargement sans comptage bloquant

### 6.1 Contrat livré

`TableQueryOptions` accepte `count_mode: none | exact`. La réponse expose
`total_rows`, `total_rows_exact`, `total_pages`, `has_more`.

Règles :

- l'absence de `count_mode` conserve le comportement historique `exact` ;
- le navigateur de tables envoie `count_mode: none` ;
- en mode `none`, le driver demande `page_size + 1` lignes ;
- la ligne supplémentaire n'est jamais envoyée à l'interface ;
- `total_rows` est une borne inférieure tant que `total_rows_exact` vaut
  `false` ;
- `has_more` est la source de vérité pour poursuivre ou arrêter le scroll.

### 6.2 Couverture réelle

Seize drivers, huit implémentations : `pg_compat` (PostgreSQL, Supabase, Neon,
TimescaleDB, CockroachDB), `mysql` (MySQL, MariaDB), `duckdb`, `sqlite`,
`motherduck`, `sqlserver`, `mongodb`, `search_compat` (Elasticsearch,
OpenSearch), `clickhouse`, `redis`.

| Famille                | Implémentation count-free                                                                                                     |
| ---------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| PostgreSQL compatible  | `LIMIT page_size + 1`, sans requête `COUNT`                                                                                   |
| MySQL compatible       | `LIMIT page_size + 1`, sans requête `COUNT`                                                                                   |
| Embarqué et analytique | Sur-lecture d'une ligne                                                                                                       |
| SQL Server             | `FETCH NEXT page_size + 1` ; `COUNT_BIG` en mode exact                                                                        |
| Document               | `limit(page_size + 1)` sans `count_documents`                                                                                 |
| Search                 | `size = page_size + 1` et `track_total_hits = false`                                                                          |
| OLAP HTTP              | `LIMIT page_size + 1` sans aller-retour `count()`                                                                             |
| Redis                  | Sur-lecture pour hash, list, set, zset et stream. Le type `string` conserve un total exact figé à 1, il n'y a rien à sur-lire |

Deux appels internes ont été migrés : lecture d'une ligne avant capture
time-travel, échantillonnage des clés étrangères du générateur de données.

### 6.3 Défauts corrigés après audit

| Défaut                                                                                                  | Correctif                                                                                                                                                                                           |
| ------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| La vue document affichait la borne inférieure comme un total (« 101 ligne(s) » pour 100 documents)      | `DocumentResults` affiche les lignes chargées tant que `total_rows_exact` est faux, comme la barre d'état                                                                                           |
| La sur-lecture franchissait `max_result_window`, transformant la dernière page chargeable en erreur 400 | `window_clamped_fetch_size` rogne la ligne excédentaire au bord de la fenêtre ; `has_more` devient alors indécidable et reste vrai, le moteur tranchant à la page suivante. Couvert par trois tests |
| L'implémentation par défaut du trait ignorait `count_mode` et fabriquait une borne fausse               | Retour au comportement historique : pas de sur-lecture, `preview_table` ne connaissant pas l'offset, donc aucun `has_more` à en déduire                                                             |
| L'onglet Info lançait un `COUNT(*)` brut avec les identifiants interpolés                               | Passe par `query_table` en `count_mode: exact` : identifiants échappés par le driver, politique de sécurité appliquée                                                                               |

L'annulation et le timeout par défaut de ce comptage relèvent de la verticale 2.

### 6.4 Contrat resserré

`total_rows` portait deux sémantiques distinguées par un booléen adjacent, que
rien n'obligeait un consommateur à lire — le défaut de `DocumentResults` en
était la démonstration. Le contrat livré rend le total absent plutôt
qu'ambigu :

```text
total_rows: number | null              // null tant que le total n'est pas connu
total_rows_source: exact | estimated   // null si et seulement si total_rows l'est
has_more: boolean
```

`total_pages` est supprimé : il n'était lu ni en Rust ni en TypeScript.

Pas de variante `lower_bound` dans `total_rows_source`, contrairement à ce que
prévoyait le 7.1. La conserver imposait de continuer à émettre la borne
inférieure, c'est-à-dire l'ambiguïté même que ce resserrement supprime. La
borne inférieure devient l'absence de total ; l'interface connaît déjà ses
lignes chargées et n'a jamais eu besoin que le backend les lui recompte.

Conséquence côté interface : le mode scroll infini se déduisait de la présence
du total, ce qui devient faux dès lors que l'absence de total est le cas
normal. Il se déduit désormais du nombre de lignes chargées.

### 6.5 Décision sur le bridge HTTP

Le bridge conserve le droit de demander `count_mode: exact` sur une table
arbitraire : l'accès y est déjà filtré par grant et par connexion, et un total
exact est un besoin d'API légitime. Il hérite en revanche du `QUERY_TIMEOUT_MS`
de 30 s que le bridge applique déjà à `execute_query`. Sans cela,
`max_query_duration_ms` valant `None` par défaut, un `COUNT(*)` sur une grande
table immobilisait une connexion sans borne de temps.

C'est un plafond de transport, pas l'annulation demandée en 7.2 : l'opération
n'est toujours pas interruptible côté application.

### 6.6 Garanties et limites

Garanties :

- aucune régression pour les appelants qui n'envoient pas `count_mode` ;
- aucune ligne de sur-lecture visible côté frontend ;
- résultat de comptage tardif protégé contre les changements de génération ;
- résultat approximatif tardif incapable d'écraser un total exact ;
- erreurs backend assainies avant affichage, données visibles jamais effacées.

Limites :

- les limites de la politique de sécurité portent sur les lignes rendues, pas
  sur les lignes lues : la sur-lecture dépasse d'une ligne le plafond clampé, et
  `fetch_size()` peut atteindre 10001 alors que `page_size` est plafonné à
  10000 ;
- le chargement des pages utilise encore `OFFSET` pour la majorité des
  moteurs ;
- un total exact est une photographie au moment du comptage, pas un verrou ;
- Elasticsearch et OpenSearch conservent leurs limites de fenêtre profonde tant
  que `search_after` n'est pas branché sur `query_table`. Au bord de la fenêtre,
  `has_more` n'est plus déductible ; la différence est que la limite est
  désormais déclarée (`max_offset_window`, cf. 9.3), donc l'interface s'arrête
  en l'annonçant au lieu de heurter l'erreur du moteur ;
- la fenêtre supposée vaut 10000 : un cluster qui l'a relevée n'est jamais bridé
  par QoreDB, mais un cluster qui l'a abaissée verra l'erreur arriver plus tôt ;
- les structures Redis basées sur `SCAN` ne fournissent pas les garanties
  d'ordre d'une table relationnelle, et leur coût reste proportionnel à
  l'offset.

### 6.7 Tests

Tests unitaires sur `from_optional_total` et sur la sérialisation du total
absent, plus un test d'intégration DuckDB en mode `none`.

La couverture par famille passe par `assert_count_free_pages`, qui parcourt
toutes les pages en `count_mode: none` et vérifie qu'une page pleine annonce la
suivante, que la dernière ne l'annonce pas, qu'aucune page ne prétend connaître
un total, que la ligne sur-lue n'atteint jamais l'appelant, et qu'aucune ligne
n'est perdue ni dupliquée. Elle est branchée sur PostgreSQL, MySQL, MongoDB,
ClickHouse, Redis (`LRANGE` exact et `HSCAN` approché) et Elasticsearch.

Restent non couverts par cette voie : SQL Server, faute d'amorce de connexion
dans le harnais d'intégration ; SQLite, DuckDB et MotherDuck, embarqués, dont
DuckDB dispose déjà d'un test unitaire ; OpenSearch, qui partage
l'implémentation `search_compat` déjà couverte par Elasticsearch.

## 7. Verticale 2 — Total honnête et opérations annulables

Livrée dans l'ordre du 13 : instrumentation, puis annulation, puis estimation.

### 7.1 Estimation moteur

Un « 100 lignes chargées » n'informe pas. Un « environ 2,4 millions de lignes »
situe immédiatement l'utilisateur, pour un coût quasi nul : la valeur provient
des métadonnées, pas d'un parcours.

| Moteur                    | Source                                                  | Coût                                    |
| ------------------------- | ------------------------------------------------------- | --------------------------------------- |
| PostgreSQL et compatibles | `pg_class.reltuples`                                    | Lecture de catalogue                    |
| MySQL, MariaDB            | `information_schema.tables.table_rows`                  | Lecture de catalogue                    |
| SQL Server                | `sys.dm_db_partition_stats`                             | Lecture de catalogue                    |
| SQLite                    | Aucune estimation fiable                                | Pas d'estimation, total exact seulement |
| DuckDB, MotherDuck        | `COUNT(*)` reste peu coûteux en colonnaire              | Comptage exact direct                   |
| ClickHouse                | `count()` MergeTree                                     | Compteur de métadonnées                 |
| MongoDB                   | `estimatedDocumentCount()`                              | Métadonnées de collection               |
| Elasticsearch, OpenSearch | `_count`, ou `hits.total` avec `track_total_hits` borné | Une requête légère                      |
| Redis                     | `HLEN`, `LLEN`, `SCARD`, `ZCARD`, `XLEN`                | O(1), déjà exact                        |

Règles d'affichage :

- toujours préfixer d'un marqueur d'imprécision, jamais un nombre nu ;
- indiquer la fraîcheur lorsque le moteur l'expose (`last_analyze` sur
  PostgreSQL) ;
- une estimation ne remplace jamais un total exact déjà obtenu ;
- une estimation absente ou nulle n'affiche rien, elle n'affiche pas zéro ;
- l'estimation ne bloque jamais la première page, elle arrive après.

Livré. `count_mode: estimated` signifie « donne-moi un total bon marché »,
pas « donne-moi un nombre approximatif ». La distinction porte tout le reste :

- les moteurs dont le total bon marché est déjà exact — DuckDB, MotherDuck,
  ClickHouse, Redis, Search — répondent ce total avec
  `total_rows_source: exact`. Dégrader un nombre juste en estimation serait un
  mensonge par excès de prudence ;
- les moteurs à statistiques de catalogue — PostgreSQL, MySQL, SQL Server,
  MongoDB — répondent `estimated` ;
- SQLite n'a pas de source fiable et ne répond aucun total, plutôt qu'un
  chiffre inventé.

Garde-fou important : une statistique de catalogue décrit la table entière. Elle
n'est donc demandée que lorsque ni filtre ni recherche ne restreint la vue
(`estimate_matches_scope`). Sans cela, une table de 2,4 M lignes filtrée sur
trois résultats aurait affiché « ~2 400 000 ».

La fraîcheur voyage avec le nombre : `total_rows_as_of` porte
`GREATEST(last_analyze, last_autoanalyze)` sur PostgreSQL, le seul moteur qui
l'expose à coût nul. Les autres laissent le champ vide plutôt que d'inventer une
date.

L'onglet Info passe par ce chemin. Les trois sources qu'il utilisait — un
`COUNT(*)` exact sur PostgreSQL, `information_schema.tables.table_rows` en SQL
interpolé sur MySQL, `schema.row_count_estimate` sur MongoDB — deviennent un
seul appel `count_mode: estimated`. Ouvrir l'onglet ne déclenche donc plus de
comptage non borné, la valeur porte son marqueur `~` et son infobulle, et une
interpolation d'identifiants de moins subsiste (cf. 11.1).

### 7.2 Annulation

Livrée. Le constat de départ : le comptage partait sur une connexion du pool
sans identifiant de requête, alors que `cancel` cherche sa cible dans
`active_queries`, une table que seul `execute` remplissait. Il n'y avait donc
rien à annuler, pas même en best effort.

`TableQueryOptions` gagne `query_id`. Il n'est jamais sérialisé : la clé du
cache de requêtes dérive des options sérialisées, et un identifiant unique par
appel y aurait transformé chaque lecture en défaut de cache.

Chaque driver enregistre le comptage dans le registre qu'il utilise déjà :

| Driver                    | Mécanisme                                        | Portée réelle    |
| ------------------------- | ------------------------------------------------ | ---------------- |
| PostgreSQL et compatibles | connexion épinglée, `pg_backend_pid`             | annulation vraie |
| MySQL, MariaDB            | connexion épinglée, `CONNECTION_ID()`            | annulation vraie |
| SQL Server                | `@@SPID` sur la connexion du comptage            | annulation vraie |
| ClickHouse                | `query_id` serveur, `KILL QUERY`                 | annulation vraie |
| DuckDB, MotherDuck        | `with_query_conn`, handle d'interruption         | annulation vraie |
| Elasticsearch, OpenSearch | `X-Opaque-Id` sur la requête `_search`           | best effort      |
| MongoDB                   | `AbortHandle` : la requête cesse d'être attendue | best effort      |
| SQLite                    | aucun mécanisme                                  | non annulable    |

L'interface ne prétend pas mieux que la réalité : le bouton porte « Annuler (au
mieux) » et une explication en infobulle lorsque le driver déclare
`CancelSupport::BestEffort`, et disparaît lorsqu'il déclare `None`.

Garde-fou temporel : `EXACT_COUNT_TIMEOUT_MS` (120 s) borne le comptage quand la
politique ne fixe aucune durée. Généreux à dessein — le seuil de la section 5
demande moins de 5 % d'abandons — mais fini.

L'onglet Info n'a plus de comptage à annuler : il lit désormais une estimation
(cf. 7.1). L'annulation reste donc cantonnée à l'action explicite de la grille,
qui est la seule à déclencher un parcours.

### 7.3 Instrumentation locale

Livrée. Les mesures de la section 5 n'existaient pas faute de point de collecte.
QoreDB est une application de bureau : rien ne sort de la machine.

La collecte est côté interface (`src/lib/diagnostics/paginationMetrics.ts`) et
non côté moteur, parce que la section 5 mesure ce que l'utilisateur ressent —
temps jusqu'à la première page, aller-retour compris — et non le temps serveur.
Un compteur Rust n'aurait vu ni le transport ni l'attribution par onglet.

Par onglet : pages chargées, lignes chargées, première page, p50 et p95 des
pages, première recherche, comptages exacts et annulés, erreurs. Les durées sont
conservées en anneau borné (200 échantillons, 32 onglets), ce qui donne des
percentiles réels au lieu d'une moyenne trompeuse.

Contrainte de confidentialité tenue par construction : un onglet est identifié
par un ordinal opaque (« #3 »), jamais par le nom de la table. Il n'y avait pas
d'identifiant d'onglet à disposition du hook, et en inventer un anonyme coûtait
moins cher que de faire descendre le vrai — qu'il aurait de toute façon fallu
taire.

Panneau accessible depuis la barre d'état, à côté du journal d'erreurs. Rien
n'est persisté ; « Copier le rapport » produit un JSON destiné à un rapport de
bug.

## 8. Verticale 3 — Coût du tri et de la recherche

Sur un client SQL, la pagination profonde n'est pas le premier coût ressenti. Un
tri sur colonne non indexée et une recherche multi-colonnes le sont, et aucun
des deux n'est traité aujourd'hui.

### 8.1 Recherche

État initial, identique sur PostgreSQL, SQLite, DuckDB et SQL Server :

- une lecture de catalogue (`information_schema.columns`, `PRAGMA table_info`)
  était émise à chaque appel, donc à chaque page du scroll infini et à chaque
  frappe débouncée ;
- le prédicat est un `OR` de `LIKE '%terme%'` sur toutes les colonnes non
  binaires, avec un paramètre par colonne ;
- le motif commence par un joker : aucun index B-tree n'est utilisable ;
- sur une table large, cela produit des dizaines de prédicats et un parcours
  complet.

Livré : `search_columns` dans le contrat. Quand l'appelant fournit le périmètre,
le driver ne lit plus le catalogue.

C'est un écart assumé par rapport à la formulation « mettre en cache le schéma
de colonnes par table et par session ». Sa justification était « il est déjà
chargé ailleurs par `describe_table` » — or l'endroit où il est déjà chargé,
c'est l'interface. Le lui faire descendre supprime l'aller-retour au lieu de
l'amortir, et évite un cache qu'il aurait fallu invalider sur DDL, faute de quoi
une colonne supprimée aurait fait échouer la recherche. Le chemin catalogue
subsiste en repli pour les appelants sans périmètre, le bridge HTTP en
particulier ; un périmètre vide y retombe aussi, plutôt que de signifier
« ne cherche dans rien » et de ne rien renvoyer.

Le périmètre par défaut est celui des colonnes textuelles
(`src/lib/query/searchScope.ts`), avec repli sur les colonnes non binaires quand
la table n'en a aucune. Caster chaque colonne numérique, date et booléenne en
texte multipliait les prédicats sans jamais répondre à ce que l'utilisateur
cherche.

Le périmètre est visible et modifiable depuis la barre de recherche
(`SearchScopeControl`), avec le mode de comparaison et, le cas échéant, la
raison pour laquelle la recherche ne peut pas utiliser d'index.

`search_mode` complète le contrat : `contains` (comportement historique) et
`starts_with`. En mode ancré, les drivers SQL abandonnent le cast **et** le
repli insensible à la casse : l'un comme l'autre suffit à rendre l'expression
inéligible à un index sur la colonne. Le prix est que le mode ancré n'a de sens
que sur une colonne textuelle, ce que l'interface sait puisqu'elle a les types.

MongoDB n'échantillonne plus de document quand le périmètre est fourni, et le
mode ancré y devient `^terme`. L'échantillonnage subsistait de toute façon d'un
défaut plus profond : il déduisait les champs textuels du premier document, donc
d'un seul document dans une collection hétérogène.

Elasticsearch et OpenSearch appliquent enfin une recherche : `query_table` y
émettait un `match_all` inconditionnel, la recherche était simplement ignorée.
C'est un `multi_match` — `best_fields`, ou `phrase_prefix` en mode ancré — sur
le périmètre demandé ou sur `*`, donc les analyseurs de l'index sont utilisés
plutôt qu'émulés.

La proposition d'index est livrée, cf. 8.2.

### 8.2 Tri

`ORDER BY colonne_non_indexée LIMIT 100 OFFSET 100000` retrie l'ensemble à
chaque page. Le keyset de la verticale 4 n'y change rien : il exige justement
une clé indexée.

Livré. La capacité de tri est dérivée des index déjà exposés par
`describe_table`, sans aucun aller-retour supplémentaire
(`src/lib/query/indexCost.ts`).

Seule la **colonne de tête** de chaque index compte : un B-tree sur `(a, b)`
n'accélère pas un tri sur `b` seul. Les index de hachage sont écartés, ils ne
répondent qu'à l'égalité. L'indicateur `isIndexed` déjà présent dans la grille
ne convenait donc pas : il est vrai pour toute colonne apparaissant dans un
index, quelle que soit sa position.

L'entête de colonne marque un tri coûteux avant qu'il ne soit déclenché, avec
la raison en infobulle. Le marquage ne s'affiche qu'en mode serveur : un tri
client porte sur les lignes déjà chargées et ne coûte rien qui mérite un
avertissement.

Le dernier point était déjà tenu : `manualSorting` est actif dès que le mode
serveur l'est, donc la grille ne retrie jamais en silence un sous-ensemble
chargé.

La création de l'index est proposée depuis le menu contextuel de l'entête, là
où le tri est déjà coûteux — pas dans l'infobulle, qui disparaît au clic.

L'action **n'exécute rien** : elle ouvre un onglet de requête pré-rempli avec le
`CREATE INDEX`. Créer un index depuis un écran de consultation serait une
mutation déclenchée depuis une surface de lecture, sur une table potentiellement
en production ; l'onglet de requête est la surface prévue pour ça et porte déjà
ses garde-fous. L'utilisateur voit l'instruction exacte avant de la lancer.

Le SQL vient du générateur Core `buildCreateIndexSQL`, extrait de
`src/lib/ddl/createTable.ts`, qui connaît déjà les dialectes, les capacités
d'index par driver et les index partiels. Le `CREATE INDEX` de
`indexSuggestions.ts` (Premium) est un doublon appauvri : ce qui relève du
Premium dans « Index Suggestions », c'est l'inférence depuis un plan EXPLAIN,
pas la génération de l'instruction.

### 8.3 Critères d'acceptation

- un aller-retour par page en recherche active, contre deux aujourd'hui ;
- un tri sur colonne indexée n'augmente pas le temps de première page de plus de
  20 % ;
- un tri ou une recherche coûteuse est annoncé avant exécution, jamais après ;
- aucune régression sur les filtres par colonne existants.

## 9. Verticale 4 — Pagination stable par curseur

### 9.1 Objectif

Supprimer le coût croissant de `OFFSET` et réduire les doublons ou omissions
lorsque les données changent entre deux chargements.

### 9.2 Contrat

Requête : `cursor`, `keyset_columns`, `page_size`.
Réponse : `next_cursor`, `has_more`, `pagination_strategy`,
`ordering_guarantee`.

`direction` et `previous_cursor` ne sont pas implémentés : le scroll infini est
une chaîne avant seulement, et rien ne les lirait. Les ajouter maintenant
recréerait un champ mort. `supports_backward` reste donc à `false`.

Le curseur ne contient aucun SQL. Il porte les clés d'ordonnancement et les
valeurs de la ligne frontière ; le driver reconstruit le prédicat à partir de
l'ordre qu'il a lui-même décidé, et le curseur ne fournit que des valeurs
liées. Il est borné (4096 octets encodés, 8 clés, 1024 octets par valeur
textuelle) et validé.

La validation la plus importante n'est pas syntaxique : un curseur frappé pour
un autre ordre est **rejeté**, jamais réinterprété. Ses valeurs pointeraient les
bonnes bornes sur les mauvaises colonnes, ce qui se lit comme une perte de
données et non comme une erreur.

`keyset_columns` suit le même principe que `search_columns` : l'appelant tient
déjà le schéma, il fournit la clé unique. Le driver n'a donc aucune lecture de
catalogue à faire, y compris sur la première page.

### 9.3 Capacité déclarée

La sélection automatique de stratégie exige que la capacité soit déclarée
quelque part de stable, sinon chaque driver improvisera. Elle rejoint les
capacités driver existantes :

```text
PaginationCapability {
  keyset: bool,
  requires_unique_key: bool,
  supports_backward: bool,
  snapshot: none | pit | transaction,
  max_offset_window: Option<u64>,
}
```

`max_offset_window` rend explicite la limite Elasticsearch de la section 3.3, et
permet à l'appelant de clamper la sur-lecture plutôt que de heurter le mur.

État : la structure est posée (`PaginationCapability`, agrégée dans
`DriverCapabilities` comme les autres capacités) et exposée au frontend. Seuls
les champs qui ont un consommateur sont renseignés aujourd'hui — le reste garde
le défaut conservateur `keyset: false`, plutôt que de déclarer une promesse que
rien ne tient encore.

Le champ renseigné est `max_offset_window`, à 10000 pour Elasticsearch et
OpenSearch. Il corrige le défaut listé en 6.6 : la limite de fenêtre profonde
n'était visible nulle part, et la seule façon de la découvrir était l'erreur du
moteur à la page suivante. Le scroll infini s'arrête maintenant au bord de la
fenêtre et le dit — « le moteur ne sert plus de pages à cette profondeur » —
au lieu d'échouer.

### 9.4 Stratégie par driver

| Drivers                                              | Stratégie préférée                                                                                   | Fallback                              |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------- |
| PostgreSQL, Supabase, Neon, TimescaleDB, CockroachDB | **Livré.** Keyset sur clé primaire, avec tie-breaker stable                                          | `OFFSET` signalé comme dégradé        |
| MySQL, MariaDB                                       | **Livré.** Keyset sur la clé fournie                                                                | `OFFSET`                              |
| SQLite, DuckDB, MotherDuck                           | **Livré.** Keyset lorsque le schéma fournit une clé stable                                          | `OFFSET`                              |
| SQL Server                                           | **Livré.** Keyset avec `TOP` et ordre unique                                                        | `OFFSET/FETCH`                        |
| MongoDB                                              | **Livré.** `_id`, ou couple `(valeur de tri, _id)`                                                  | `skip`                                |
| Elasticsearch, OpenSearch                            | **Livré.** `search_after` sur une clé unique déclarée ; sans PIT, cf. ci-dessous                     | `from/size` dans la fenêtre autorisée |
| ClickHouse                                           | **Livré.** Keyset sur la clé fournie                                                                | `OFFSET` avec avertissement de coût   |
| Redis list                                           | Index natif                                                                                          | Pagination actuelle                   |
| Redis zset                                           | Couple `(score, member)`                                                                             | Index                                 |
| Redis stream                                         | Identifiant de stream                                                                                | Pagination actuelle                   |
| Redis hash et set                                    | Curseur `HSCAN` ou `SSCAN`, sans promettre un ordre stable                                           | Scan depuis le début si nécessaire    |

Redis garde le défaut `keyset: false`. Ses structures paginent déjà nativement
par index (`LRANGE`) ou par curseur de scan ; il n'y a pas de clé unique sur
laquelle poser un keyset, et prétendre le contraire aurait été une déclaration
fausse. Les stratégies par type listées plus haut restent à écrire, mais elles
relèvent d'un autre modèle que celui de cette verticale.

Elasticsearch et OpenSearch méritent une note. `search_after` exige un ordre
total, et sans lecteur PIT le moteur n'offre aucun tie-breaker inter-shards :
`_shard_doc` en demande un, et `_doc` n'ordonne qu'à l'intérieur d'un shard.
Le keyset y repose donc entièrement sur une clé unique déclarée par l'appelant.
Quand elle existe, `from` disparaît au profit de `search_after`, ce qui lève
aussi la limite de fenêtre profonde sur ce chemin.

Un piège corrigé après un essai en conditions réelles, qui vaut d'être écrit :
le schéma d'une table arrive **après** sa première page. L'interface partait donc
en `OFFSET`, puis envoyait la clé unique à partir de la page 2 — et le driver,
voyant un keyset sans curseur, supprimait l'`OFFSET` et resservait la première
page. La table entière se lisait en boucle.

Deux garde-fous, l'un vaut pour tout appelant :

- côté contrat, `keyset_applies()` : une page au-delà de la première **sans**
  curseur n'est pas un parcours par curseur, c'est de la pagination par offset,
  et elle est servie comme telle. Sans cela le bridge HTTP aurait le même bug ;
- côté interface, la clé de keyset est figée pour toute la durée du parcours, et
  son arrivée tardive ne provoque qu'un seul rechargement, tant qu'une seule
  page est chargée.

Trois décisions d'implémentation qui ne se voient pas dans le tableau :

- la clé retenue est la **clé primaire uniquement**, pas n'importe quel index
  unique. Un index unique peut porter sur des colonnes nullables, et une
  comparaison sur `NULL` est `NULL` : la ligne sort du prédicat et disparaît
  silencieusement de la pagination. La clé primaire est unique _et_ non nulle
  par définition ;
- le prédicat est écrit en forme développée
  (`k1 > v1 OR (k1 = v1 AND k2 > v2)`) plutôt qu'avec un constructeur de ligne
  `(k1, k2) > (v1, v2)`. Ce dernier est plus lisible et plus indexable, mais il
  ne sait pas mélanger `ASC` et `DESC`, que le §9.8 demande de couvrir ;
- la construction du prédicat, de l'ordre et de la frappe du curseur vit dans un
  seul `KeysetPlan` (`qore-core/src/cursor.rs`), paramétré par une fonction de
  quoting et une fonction de rendu de paramètre. C'est la partie facile à rater
  de façon subtile, et une erreur subtile ici saute des lignes au lieu
  d'échouer. Les drivers qui inlinent leurs littéraux — SQL Server, ClickHouse —
  passent leur propre formateur, ceux qui lient passent `?` ou `$n`.

MongoDB a demandé un traitement à part : `query_table` y projette une unique
colonne `document` contenant tout le JSON, donc la frappe générique du curseur
n'y aurait jamais trouvé les champs clés et le keyset serait resté
silencieusement inactif. La borne est lue depuis le document BSON.

### 9.5 Schémas sans clé stable

C'est le cas fréquent, pas l'exception : vues, vues matérialisées sans index,
tables sans clé primaire, résultats de requête ad hoc. Le comportement par
défaut y est `OFFSET`, avec `ordering_guarantee: none`.

Livré : la barre d'état affiche « ordre instable » avec, en infobulle, ce que ça
coûte concrètement — une ligne peut apparaître deux fois ou être sautée si les
données changent pendant le défilement. C'est la formulation qui compte : dire
« pas de clé unique » n'apprend rien à qui n'a pas le modèle en tête.

Une table partitionnée ou une clé composite nullable relève du même traitement
tant que l'unicité n'est pas démontrée. Côté interface, seule la clé primaire
est proposée comme clé de keyset, jamais un index unique quelconque : un index
unique peut porter sur des colonnes nullables.

### 9.6 Interaction avec l'édition

Une ligne modifiée localement peut changer de position dans l'ordre keyset et
donc réapparaître ou disparaître de la fenêtre.

Constat après lecture du code : le produit ne connaît aujourd'hui qu'une seule
réponse à une édition. `useInlineEdit` écrit côté serveur puis appelle
`onRowsUpdated`, que `TableBrowser` branche sur `reload` — le scroll entier est
jeté et refetché depuis la page 1. Il n'y a aucune mise à jour optimiste : la
grille dépend de ce rechargement pour afficher la nouvelle valeur.

Cette base est **correcte** sous keyset : repartir de zéro ne peut ni dupliquer
ni sauter de ligne. Elle est en revanche brutale — dix pages parcourues sont
perdues à chaque cellule modifiée — et elle rend les trois règles ci-dessous
inapplicables telles quelles, puisqu'elles supposent toutes que les lignes
chargées survivent à l'édition :

- une ligne éditée reste ancrée à sa position d'origine jusqu'au prochain
  rechargement explicite ;
- une ligne qui sort du prédicat de filtre après édition est signalée, pas
  masquée silencieusement ;
- une ligne insérée localement reste visible jusqu'au rechargement, même si le
  curseur l'aurait placée ailleurs.

Les trois demandent la même primitive absente : muter en place le jeu de lignes
chargées, en localisant la ligne par sa clé primaire. Cela implique de changer
la signature de `onRowsUpdated` pour qu'elle porte la clé et les valeurs
modifiées, de la propager depuis `useInlineEdit`, et — pour la deuxième règle —
d'évaluer les filtres de colonne côté client afin de marquer la ligne qui en
sort. La recherche serveur, elle, ne peut pas être réévaluée localement : la
règle ne peut donc porter que sur les filtres de colonne, et doit le dire.

Non livré. C'est le seul point de cette verticale que je laisse ouvert, et
délibérément : la réécriture porte sur le chemin d'édition, qu'aucun test ne
couvre ici, et une erreur y afficherait des valeurs périmées sans rien signaler.
La base actuelle étant correcte, le risque de la refonte dépasse le gain tant
qu'elle n'est pas vérifiable.

### 9.7 Décision de setting

Il ne faut pas demander à l'utilisateur de choisir « cursor » ou « offset » dans
le parcours normal. QoreDB sélectionne automatiquement la meilleure stratégie
sûre selon le schéma et le driver.

Un réglage avancé « Forcer la stratégie de pagination » ne devient pertinent que
pour diagnostiquer un schéma atypique, contourner temporairement un bug moteur,
comparer des performances, ou préserver un comportement historique dans une
intégration. Il reste par connexion ou par table, jamais une préférence
générale.

Prérequis vérifié, et il n'est pas rempli : `SavedConnection` est une structure
fixe du coffre, sans espace de réglages libre. Ajouter un champ persisté pour un
réglage de diagnostic supposerait de toucher la sérialisation du coffre et sa
migration — disproportionné pour un usage que le paragraphe ci-dessus qualifie
lui-même d'exceptionnel.

Ce que la verticale apporte à la place couvre les deux premiers usages cités :
la stratégie est désormais **observable**, puisque `pagination_strategy` et
`ordering_guarantee` reviennent dans chaque réponse et que l'ordre instable est
affiché. Diagnostiquer un schéma atypique ne demande plus de forcer quoi que ce
soit. Le forçage proprement dit reste non livré.

### 9.8 Critères d'acceptation

| Critère                                                | État                                                                                      |
| ------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| Ordre déterministe, ou indication explicite du contraire | Tenu. `ordering_guarantee` remonte et s'affiche                                          |
| Aucune concaténation de valeur de curseur dans le SQL   | Tenu. Les drivers qui lient passent des paramètres ; SQL Server et ClickHouse inlinent par leur formateur d'échappement, comme pour toute autre valeur |
| Filtres et tris identiques entre les pages             | Tenu par construction : un curseur frappé pour un autre ordre est rejeté                   |
| Clés simples, composites, directions mixtes            | Testé sur PostgreSQL contre une base réelle                                               |
| Clés nullables                                         | Écarté par conception : seule la clé primaire sert de clé de keyset, cf. 9.5               |
| Insertions et suppressions entre deux pages            | Testé sur PostgreSQL : une ligne insérée derrière la borne ne réapparaît pas               |
| Fallback contrôlé sans clé unique                      | Testé : `pagination_strategy: offset`, `ordering_guarantee: none`, aucun curseur émis      |
| Pas de régression sur l'édition, la sélection, l'export | Non vérifié — aucun test ne couvre ces chemins                                            |

Les tests d'intégration (`postgres_keyset_pagination`, `mysql_keyset_pagination`)
parcourent toutes les pages par curseur et vérifient qu'aucune ligne n'est
rendue deux fois ni sautée. `postgres_keyset_pagination` a été exécuté contre
PostgreSQL ; le test MySQL est écrit mais n'a pas pu tourner, le port 3306 étant
déjà occupé sur la machine de développement.

Les six autres implémentations — SQL Server, SQLite, DuckDB, MotherDuck,
MongoDB, ClickHouse, Elasticsearch — compilent et suivent le même `KeysetPlan`
testé unitairement, mais leur SQL n'a pas été exécuté.

## 10. Verticale 5 — Mémoire et rendu progressif

### 10.1 Budget mémoire par onglet

Le scroll infini ne doit pas conserver indéfiniment toutes les lignes. La forme
initialement prévue — fenêtre glissante autour de la zone visible, éviction des
anciens chunks, rechargement silencieux à la remontée à partir des curseurs de
la verticale 4 — a été écartée après mesure. Ce qui est livré est un budget
d'octets par onglet, sans éviction.

Ce que disent les mesures. Une ligne pèse environ 2 Ko sur une table de vingt
colonnes courtes et 5,4 Ko sur une table large à texte long ; les 250 Mo de la
définition de terminé correspondent donc à 46 000 à 120 000 lignes, soit 460 à
1 200 pages. Or le chargement est strictement séquentiel et déclenché par la
proximité du bas : atteindre ce volume suppose de faire défiler plusieurs
millions de pixels sans interruption, de l'ordre de vingt minutes. La fenêtre
glissante défendrait donc un scénario que personne n'atteint, au prix du
mécanisme le plus coûteux du chantier — lignes fantômes pour préserver l'espace
d'index, rechargement en arrière, et une reprise complète de la sélection, du
filtre client, de la copie et de l'édition.

Le volume réellement atteignable n'est pas la profondeur, c'est le poids. Une
table dont une colonne porte des documents ou des blobs de plusieurs mégaoctets
sature en quelques pages. C'est ce cas que le budget couvre, et il le couvre
mieux qu'une fenêtre : une fenêtre glissante sur des lignes de 5 Mo ne garderait
que quelques dizaines de lignes visibles et passerait son temps à recharger.

Le mécanisme livré tient en une règle. Chaque page chargée est pesée — en
parcourant les valeurs, jamais en les sérialisant, puisque la page à mesurer est
justement celle qu'il ne faut pas recopier — et le cumul est comparé au budget de
l'onglet. Au-delà, le défilement s'arrête et l'onglet le dit, en renvoyant vers
le filtre ou l'export, qui sont les vraies réponses à « je veux voir la ligne
200 000 ». Le plafond porte sur la charge utile et vaut 128 Mo, en dessous des
250 Mo visés : la grille ajoute un proxy par ligne et le moteur de rendu un nœud
par cellule visible.

Le réglage prévu en 12 n'est pas exposé. Un défaut sûr suffit tant que personne
ne l'a atteint pour de mauvaises raisons ; l'ajouter maintenant serait une
commande sans conséquence observable.

### 10.2 Sélection et export sous fenêtre bornée

Sujet le plus risqué de cette verticale, tranché avant d'écrire le premier
mécanisme d'éviction. Quatre décisions, dans l'ordre où elles se contraignent.

**La sélection reste un ensemble de lignes concrètes.** L'alternative — une
sélection-prédicat portant les filtres et le tri courants — a été écartée. Les
deux actions de masse existantes, la suppression et l'édition groupée,
construisent leur clause `WHERE` à partir de la clé primaire de chaque ligne
retenue. Les faire porter un prédicat transformerait la navigation en
`DELETE ... WHERE <filtres>` sur une table potentiellement de production :
ce n'est pas une variante de la sélection, c'est une autre fonctionnalité, avec
son propre coût de sûreté. Elle n'est pas nécessaire ici.

**Une ligne sélectionnée n'est jamais évincée.** C'est l'invariant qui rend la
décision précédente tenable : puisque la sélection désigne des lignes, elle ne
peut pas rétrécir en silence parce que l'utilisateur a continué à faire défiler.
La même protection couvre la cellule en cours d'édition et, en mode bac à sable,
les lignes dont la modification n'est pas encore appliquée. Conséquence assumée :
une sélection très large fixe la mémoire qu'elle occupe. Le budget borne le
défilement, pas ce que l'utilisateur a explicitement désigné.

**L'éviction exige une identité de ligne stable.** La grille identifie une ligne
par sa clé primaire, et retombe sur son index de position lorsqu'il n'y en a pas.
Sous cette dernière forme, évincer décalerait la sélection sur d'autres lignes.
Une table sans clé primaire ne fait donc pas d'éviction du tout : elle est déjà
celle qui n'a pas d'ordre stable, mieux vaut une seule limite à expliquer.

**L'export « tout » relit la source, jamais la grille.** Le chemin en flux le
faisait déjà. Ce qui restait à corriger : l'action d'export du menu applicatif
écrivait les lignes présentes en mémoire, donc silencieusement moins que la
table dès qu'elle est plus grande que ce qui a été parcouru. Sans sélection,
elle ouvre désormais l'export en flux. Avec une sélection, elle exporte
exactement cette sélection. Le presse-papiers, lui, reste ce qu'il est — une
opération sur ce qui est affiché — mais son menu annonce sa portée et son
nombre de lignes au lieu de les laisser deviner.

### 10.3 Taille de chunk adaptative

Écartée, et pas seulement par prudence : le contrat s'y oppose.

Le profil utilisateur à trois crans (Économe, Équilibré, Rapide) l'était déjà —
une seconde commande sur un algorithme adaptatif, dont personne ne peut prédire
ce que « Rapide » signifie. Reste l'adaptation automatique, dont le seul entrant
réellement utile est le poids des lignes : une table à cinq mégaoctets par ligne
ne devrait pas en demander cent d'un coup. Mais faire varier la taille de page
en cours de parcours suppose que l'appelant ne raisonne plus en numéro de page,
puisque l'offset s'en déduit. Sous curseur, la taille peut varier librement ;
sous offset, la faire varier décale ou répète des lignes — exactement la classe
de régression silencieuse corrigée en 9.4.

Restreindre l'adaptation aux parcours par curseur serait possible, mais elle y
ferait double emploi avec le budget de 10.1, qui protège déjà le cas des lignes
lourdes et le fait de façon compréhensible : le chargement s'arrête et le dit,
au lieu de ralentir sans que rien ne l'explique.

La taille de page brute reste un réglage avancé de diagnostic.

### 10.4 Cellules lourdes

Une cellule affichait sa valeur entière. Le formateur de la grille rendait tout
le texte, ou tout le document sérialisé, dans un nœud du DOM large de quelques
dizaines de pixels : pour une colonne `text` de plusieurs mégaoctets, chaque
ligne visible coûtait sa valeur complète en mémoire de rendu, à chaque passage.
La troncature CSS masquait le trop-plein sans jamais l'éviter.

Le rendu passe désormais par un aperçu borné, distinct du formateur exact. La
séparation est le point important : la copie, l'export et les filtres continuent
de lire le formateur exact, si bien qu'aucun chemin sortant ne peut expédier un
extrait à la place de la valeur. Seul l'affichage est coupé, et il le dit —
élargir une colonne révèle la suite d'une troncature CSS mais rien d'un extrait,
donc les deux ne doivent pas se ressembler. La valeur entière reste à un clic,
dans la ligne.

Le binaire avait déjà son traitement : taille lisible à la place du base64, et
visionneuse à la demande.

Ce qui n'est pas fait, et pourquoi. Le rendu direct de HTML ou de SVG non fiable
n'a rien demandé : aucun `dangerouslySetInnerHTML` n'existe dans l'interface, et
React échappe par défaut. Déplacer le formatage hors du thread de rendu
supposerait un worker pour un coût désormais borné par l'aperçu — la dépense ne
se justifie plus. Limiter copie et export selon la politique de sécurité relève
de 11.1 et reste ouvert : il n'existe aujourd'hui **aucune limite d'octets**
dans la politique, seulement une limite de lignes.

### 10.5 Précision des types

Éviter les pertes silencieuses entre moteurs, Rust et JavaScript. L'audit a
trouvé trois pertes réelles, chacune invisible : la valeur affichée est
plausible, rien ne signale qu'elle a été altérée.

**Les décimaux passaient par un double à la lecture.** `NUMERIC` et `DECIMAL`
étaient convertis en `f64` dès que la conversion aboutissait, et ne repassaient
en texte qu'en cas de débordement. Un `numeric(40,10)` arrivait donc arrondi à
une quinzaine de chiffres significatifs, sans que rien en aval puisse distinguer
la valeur arrondie de la valeur stockée. Côté MySQL, la conversion allait plus
loin : `to_f64().unwrap_or(0.0)` remplaçait par **zéro** un décimal non
convertible. La représentation est désormais décidée par le contrat, dans
`Value::from_decimal_text` : la valeur reste un nombre quand un double la porte
exactement — la colonne ordinaire garde donc son type — et voyage en texte
lorsqu'un double l'altérerait. La comparaison porte sur les chiffres et non sur
les valeurs, sans quoi on comparerait un double avec lui-même ; l'échelle rendue
par le moteur (`1.50` pour `1.5`) ne compte pas comme une perte.

**Les horodatages sans fuseau étaient tronqués à la seconde.** Le format
`%Y-%m-%d %H:%M:%S` supprimait les fractions de seconde d'un `timestamp(6)` ou
d'un `time(6)`, sur PostgreSQL comme sur MySQL, dans les chemins typés comme
dans les chemins de repli. Le format porte maintenant `%.f`, qui n'ajoute rien
lorsque la fraction est nulle : les colonnes à la seconde s'affichent comme
avant.

**L'édition d'une cellule numérique repassait par un double.** `parseInputValue`
appelait `Number()` sur tout type numérique, y compris `bigint` et `numeric` :
éditer une cellule était l'opération qui perdait un chiffre, cette fois en
écriture. La coercition est maintenant conditionnée à sa réversibilité, avec le
même critère de chiffres que côté Rust ; ce qui n'y survit pas part sous la
forme saisie.

Reste ouvert : un entier hors plage sûre JavaScript arrive déjà arrondi par
`JSON.parse`, avant que le moindre code de l'application le voie. Les
identifiants de type Snowflake, autour de 1,4 × 10¹⁸, sont exactement dans ce
cas. Le corriger suppose de choisir une représentation de rechange sur le fil,
et `Value` est sérialisé en `untagged` : une chaîne de chiffres y devient
indistinguable d'une colonne texte qui contient des chiffres, ce qui casserait
la liaison des paramètres au retour — `WHERE code = '12345'` sur une colonne
texte et `WHERE id = 12345` sur une colonne entière ont la même forme sur le
fil. C'est un changement de protocole, pas un correctif local, et il est décrit
ici pour être traité comme tel.

## 11. Verticale 6 — Cohérence et sécurité

Transverse : chaque point s'applique au fur et à mesure des verticales, pas en
fin de chantier.

### 11.1 Sécurité

- appliquer timeout, limite de lignes et limite d'octets à tous les modes, en
  tenant compte du fait que les plafonds de la politique valent `None` par
  défaut ;
- faire porter les limites sur les lignes lues, pas seulement sur les lignes
  rendues ;
- borner la longueur et la complexité des curseurs ;
- signer ou authentifier les curseurs sur le bridge HTTP
  (`qore-server/src/routes/bridge.rs`), seul chemin où ils franchissent une
  frontière de confiance ; à l'intérieur de l'application de bureau, ce n'est
  pas nécessaire ;
- décider explicitement si le bridge peut demander `count_mode: exact` sur une
  table arbitraire, ou s'il hérite d'un plafond ;
- paramétrer toutes les valeurs et valider tous les identifiants, y compris dans
  l'onglet Info où les noms de schéma et de table sont aujourd'hui interpolés ;
- limiter les previews de blobs et documents imbriqués ;
- ne jamais journaliser de données de cellule ni de contenu de curseur ;
- conserver les erreurs secondaires non destructives pour l'interface.

### 11.2 Niveaux de cohérence

1. best effort : navigation rapide sans snapshot ;
2. ordre stable : keyset avec tie-breaker unique ;
3. snapshot cohérent : PIT côté Search, ou transaction ouverte côté SQL, ce que
   les drivers savent déjà faire via `transaction_conn`.

Le niveau 1 est le défaut du navigateur. Le niveau 2 s'active automatiquement
dès que la capacité le permet. Le niveau 3 n'est jamais implicite : il immobilise
une connexion et doit être demandé par le workflow qui en a besoin.

## 12. Politique de settings

| Option potentielle                       | Décision                                                                             |
| ---------------------------------------- | ------------------------------------------------------------------------------------ |
| Calculer toujours le total exact         | Ne pas exposer ; conserver l'action à la demande                                     |
| Afficher une estimation moteur           | Retenu, verticale 2, avec provenance et fraîcheur affichées                          |
| Périmètre de la recherche (colonnes)     | Retenu, verticale 3, visible dans la barre de recherche plutôt que dans les réglages |
| Taille de page brute                     | Réglage avancé de diagnostic uniquement                                              |
| Profil Économe, Équilibré, Rapide        | Écarté, cf. 10.3                                                                     |
| Forcer cursor ou offset                  | Diagnostic avancé, par connexion ou table                                            |
| Budget mémoire par onglet                | Retenu, réglages avancés, défaut sûr                                                 |
| Taille maximale de preview d'une cellule | Retenu, avec plafond imposé par la politique de sécurité                             |
| Niveau de cohérence ou snapshot          | Exposé uniquement dans les workflows qui en ont besoin                               |

Une option n'est ajoutée que si plusieurs implémentations sont viables, qu'aucune
ne domine clairement, que l'utilisateur en comprend la conséquence, que le choix
s'explique avec un vocabulaire produit, et qu'il ne permet pas de contourner une
limite de sécurité administrateur.

## 13. Ordre de livraison

Séquencé par valeur rendue et par dépendance, en lots livrables indépendamment.

1. Correctifs de la verticale 1 — appliqués, cf. section 6.3.
2. Contrat resserré — appliqué, cf. sections 6.4 et 6.5.
3. Tests par famille de drivers — appliqués, cf. section 6.7.
4. Verticale 2 — appliquée dans cet ordre : instrumentation, annulation,
   estimation moteur, unification de l'onglet Info.
5. Verticale 3 — livrée, cf. 8.1 et 8.2.
6. Verticale 4 — livrée, cf. 9.2 à 9.8. Restent l'édition sous curseur (9.6) et
   le forçage de stratégie (9.7), tous deux documentés avec leur raison.
7. Verticale 5 — décisions sélection et export d'abord, puis fenêtre bornée,
   chunk adaptatif, rendu progressif des cellules, types sans perte.

Les points de la verticale 6 s'appliquent au sein de chaque lot.

## 14. Définition de terminé

Le chantier est terminé lorsque :

- l'ouverture d'une grande table ne dépend d'aucun comptage, et affiche un ordre
  de grandeur en moins de 400 ms p95 ;
- aucun nombre affiché n'est ambigu : exact, estimé ou absent, jamais un total
  implicite ;
- toute opération de plus d'une seconde est annulable ;
- une recherche ou un tri coûteux est annoncé avant exécution ;
- les pages profondes restent dans un facteur 2 de la première page lorsqu'une
  stratégie stable existe, et les fallbacks sont visibles et explicables ;
- la mémoire d'un onglet reste sous 250 Mo pendant un scroll prolongé ;
- la sélection et l'export gardent un sens sous fenêtre bornée ;
- aucune valeur n'est silencieusement altérée par le rendu ;
- un échec secondaire ne détruit jamais le travail ou le contexte visible ;
- chaque famille de drivers dispose de tests sur ses garanties réelles ;
- les réglages exposés correspondent à des décisions compréhensibles, pas à des
  détails d'implémentation.
