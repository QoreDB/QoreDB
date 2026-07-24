# Plan produit — Pagination et rendu des données

> Statut au 24 juillet 2026
>
> - Verticale 1 — chargement sans comptage bloquant : livrée, correctifs appliqués, dette de contrat ouverte
> - Verticale 2 — total honnête et opérations annulables : à implémenter
> - Verticale 3 — coût du tri et de la recherche : à concevoir
> - Verticale 4 — pagination stable par curseur : à concevoir
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

| Non-objectif | Raison |
|---|---|
| Sauter à la page N arbitraire en pagination par curseur | Incompatible avec le keyset ; le besoin réel est « aller à une valeur », pas « aller à un rang » |
| Garantir une cohérence transactionnelle par défaut | Coût de connexion et de verrous disproportionné pour de la navigation ; réservé aux workflows qui le demandent |
| Afficher un total exact systématiquement | C'est le coût qu'on cherche justement à supprimer |
| Rendre `OFFSET` performant sur les pages profondes | Impossible côté client ; la réponse est le curseur, pas l'optimisation |
| Unifier les garanties d'ordre entre moteurs relationnels et `SCAN` Redis | Techniquement impossible ; la réponse est une capacité déclarée, pas une abstraction qui ment |
| Charger l'intégralité d'une table en mémoire pour un tri ou une recherche client | Ne passe pas l'échelle ; le tri et la recherche restent serveur |

## 3. Inventaire de l'existant

Établi par lecture du code au 24 juillet 2026. Toute décision ci-dessous s'y
réfère plutôt qu'à des suppositions.

### 3.1 Où le produit affiche un nombre de lignes

| Emplacement | Source | Nature |
|---|---|---|
| `DataGridStatusBar` | `useInfiniteTableData` | Lignes chargées, plus total exact après action explicite |
| `DocumentResults` (entête) | `infiniteScrollLoadedRows`, ou le total exact une fois calculé | Lignes chargées tant que le total est inconnu |
| Onglet Info, PostgreSQL | `query_table` en `count_mode: exact` | Total exact, non borné, pas encore annulable |
| Onglet Info, MySQL et MariaDB | `information_schema.tables.table_rows` | Estimation moteur, affichée sans mention d'imprécision |
| Onglet Info, MongoDB | `schema.row_count_estimate` | Estimation moteur, même remarque |

Le produit affiche donc déjà des estimations moteur, sans jamais l'indiquer.
C'est une incohérence à corriger avant d'en introduire de nouvelles.

### 3.2 Chemins d'appel de `query_table`

- Navigateur de tables (`useInfiniteTableData`) : envoie `count_mode: none`.
- Bridge HTTP (`qore-server/src/routes/bridge.rs:176`) : seul appelant hors
  application ; transmet les options du client sans les restreindre.
- La CLI et le serveur MCP n'appellent pas `query_table`.

Conséquence : la « migration progressive des consommateurs » évoquée
initialement se réduit à une seule décision, celle du bridge.

### 3.3 Limites en vigueur

| Limite | Valeur | Portée |
|---|---|---|
| `page_size` (`types.rs:1159`) | `clamp(1, 10000)`, défaut 50 | Contrat QoreDB. `fetch_size()` peut donc valoir 10001 |
| `policy.max_result_rows` | `None` par défaut | Aucun plafond de lignes tant que l'utilisateur n'en configure pas |
| `policy.max_query_duration_ms` | `None` par défaut | Idem pour le temps |
| `index.max_result_window` | 10000, côté cluster | Limite Elasticsearch et OpenSearch sur `from + size` |
| `STREAM_SIZE_THRESHOLD` (`search_compat.rs:604`) | 10000 | Seuil interne : au-delà, `_search` passe en PIT + `search_after` |

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

| Décision | Gains | Contreparties | Réponse produit | Statut |
|---|---|---|---|---|
| Ne plus calculer le total au chargement standard | Première page plus rapide, un aller-retour en moins, moins de charge sur la base | Le total exact n'est pas immédiatement connu | Afficher les lignes chargées, puis un ordre de grandeur estimé, et un calcul exact explicite | Livré, estimation en verticale 2 |
| Sur-lire une ligne pour produire `has_more` | Détection fiable de la page suivante sans comptage séparé | Une ligne de plus est lue, et le budget de lignes peut être dépassé d'une unité | Coût borné, invisible, sauf là où le moteur impose une fenêtre : la sur-lecture doit alors être clampée | Livré, clamp à faire |
| Calculer le total exact à la demande | L'utilisateur garde le contrôle ; aucun coût pour ceux qui n'en ont pas besoin | Le comptage peut rester long sur une grande table | Action non bloquante, annulable, avec timeout par défaut, erreur non destructive | Livré, annulation en verticale 2 |
| Afficher une estimation moteur | Un ordre de grandeur immédiat, coût quasi nul, meilleure réponse que le silence | Un nombre approché peut être pris pour un total | Provenance et fraîcheur affichées, marqueur d'imprécision systématique, jamais un nombre nu | Verticale 2 |
| Rendre toute opération lourde annulable | L'utilisateur n'est jamais captif d'une action déclenchée par erreur | Un chemin d'annulation par driver, avec des garanties inégales | S'appuyer sur `CancelSupport` existant et déclarer honnêtement le niveau réel | Verticale 2 |
| Traiter le coût du tri et de la recherche | C'est le premier coût ressenti, avant la pagination profonde | Certaines colonnes deviennent explicitement coûteuses à trier ou chercher | Capacité déclarée, avertissement avant exécution, proposition d'index | Verticale 3 |
| Utiliser une pagination par curseur quand un ordre stable existe | Coût des pages profondes quasi constant, meilleure stabilité sous mutations | Pas de saut arbitraire vers la page N ; implémentation spécifique par moteur | Choix automatique par capacité, fallback explicite vers `OFFSET` | Verticale 4 |
| Borner les lignes conservées par l'interface | Mémoire stable pendant les longues sessions | Les lignes anciennes doivent être rechargées, et la sélection perd son référentiel | Fenêtre glissante sur curseurs, sélection redéfinie comme prédicat, export relu depuis la source | Verticale 5 |
| Limiter et charger progressivement les cellules lourdes | Moins de mémoire, de sérialisation et de blocage du rendu | Le contenu complet nécessite une action supplémentaire | Preview claire, taille affichée, chargement complet à la demande | Verticale 5 |
| Uniformiser le contrat sans masquer les différences moteur | UX cohérente et API plus simple | Toutes les garanties ne sont pas possibles partout | Matrice de capacités et fallback honnête, jamais de fausse garantie | Transverse |

## 5. Objectifs mesurables

Aucune mesure de référence n'existe aujourd'hui. Le premier jalon de la
verticale 2 est de produire ces mesures ; les seuils ci-dessous sont des cibles
à confirmer, puis à figer comme critère de non-régression.

| Indicateur | Cible | Conditions |
|---|---|---|
| Temps jusqu'à la première page | p95 < 400 ms | Table de 10 M lignes, sans filtre, réseau local |
| Temps d'une page suivante | p95 < 250 ms | Même table, scroll continu |
| Écart page 1 / page 100 | facteur < 2 | Avec une stratégie de curseur disponible |
| Allers-retours par page | 1 | Sans recherche active ; 2 aujourd'hui avec recherche |
| Mémoire par onglet | < 250 Mo | 100 000 lignes parcourues, 20 colonnes |
| Première frappe de recherche | p95 < 800 ms | Table de 1 M lignes, 20 colonnes |
| Taux de fallback curseur vers `OFFSET` | < 20 % | Sur un échantillon de schémas réels |
| Comptages exacts abandonnés par timeout | < 5 % | Sinon le timeout est mal calibré |

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

| Famille | Implémentation count-free |
|---|---|
| PostgreSQL compatible | `LIMIT page_size + 1`, sans requête `COUNT` |
| MySQL compatible | `LIMIT page_size + 1`, sans requête `COUNT` |
| Embarqué et analytique | Sur-lecture d'une ligne |
| SQL Server | `FETCH NEXT page_size + 1` ; `COUNT_BIG` en mode exact |
| Document | `limit(page_size + 1)` sans `count_documents` |
| Search | `size = page_size + 1` et `track_total_hits = false` |
| OLAP HTTP | `LIMIT page_size + 1` sans aller-retour `count()` |
| Redis | Sur-lecture pour hash, list, set, zset et stream. Le type `string` conserve un total exact figé à 1, il n'y a rien à sur-lire |

Deux appels internes ont été migrés : lecture d'une ligne avant capture
time-travel, échantillonnage des clés étrangères du générateur de données.

### 6.3 Défauts corrigés après audit

| Défaut | Correctif |
|---|---|
| La vue document affichait la borne inférieure comme un total (« 101 ligne(s) » pour 100 documents) | `DocumentResults` affiche les lignes chargées tant que `total_rows_exact` est faux, comme la barre d'état |
| La sur-lecture franchissait `max_result_window`, transformant la dernière page chargeable en erreur 400 | `window_clamped_fetch_size` rogne la ligne excédentaire au bord de la fenêtre ; `has_more` devient alors indécidable et reste vrai, le moteur tranchant à la page suivante. Couvert par trois tests |
| L'implémentation par défaut du trait ignorait `count_mode` et fabriquait une borne fausse | Retour au comportement historique : pas de sur-lecture, `preview_table` ne connaissant pas l'offset, donc aucun `has_more` à en déduire |
| L'onglet Info lançait un `COUNT(*)` brut avec les identifiants interpolés | Passe par `query_table` en `count_mode: exact` : identifiants échappés par le driver, politique de sécurité appliquée |

L'annulation et le timeout par défaut de ce comptage relèvent de la verticale 2.

### 6.4 Dette de contrat

`total_rows` porte deux sémantiques distinguées par un booléen adjacent. Le
défaut de `DocumentResults` en est la démonstration : rien n'oblige un
consommateur à lire `total_rows_exact`. Le contrat cible est un total absent
plutôt qu'un total ambigu, et l'abandon de `total_pages`, que personne ne lit :

```text
total_rows: number | null     // null tant que le total n'est pas connu
has_more: boolean
```

À faire avant que d'autres consommateurs ne se branchent sur le contrat actuel.
Le bridge HTTP est le seul appelant externe, la fenêtre est encore ouverte.

### 6.5 Garanties et limites

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
  que `search_after` n'est pas branché sur `query_table` : au bord de la
  fenêtre, `has_more` n'est plus déductible et le moteur renvoie son erreur à la
  page suivante. Le contrat n'a pas de moyen d'exprimer « il y a peut-être plus,
  mais je ne peux pas aller voir » ; c'est `max_offset_window` en verticale 4 ;
- la fenêtre supposée vaut 10000 : un cluster qui l'a relevée n'est jamais bridé
  par QoreDB, mais un cluster qui l'a abaissée verra l'erreur arriver plus tôt ;
- les structures Redis basées sur `SCAN` ne fournissent pas les garanties
  d'ordre d'une table relationnelle, et leur coût reste proportionnel à
  l'offset.

### 6.6 Tests

Existant : tests unitaires sur `from_optional_total`, un test d'intégration
DuckDB en mode `none`. Manquant : une couverture par famille, en priorité les
deux chemins les plus fragiles, Redis (`SCAN`, ordre non garanti) et Search
(fenêtre profonde). Un test par famille vérifiant qu'une page pleine signale
`has_more` et que la page finale ne le signale pas.

## 7. Verticale 2 — Total honnête et opérations annulables

Petite verticale, valeur immédiate. À livrer avant tout travail sur les
curseurs.

### 7.1 Estimation moteur

Un « 100 lignes chargées » n'informe pas. Un « environ 2,4 millions de lignes »
situe immédiatement l'utilisateur, pour un coût quasi nul : la valeur provient
des métadonnées, pas d'un parcours.

| Moteur | Source | Coût |
|---|---|---|
| PostgreSQL et compatibles | `pg_class.reltuples` | Lecture de catalogue |
| MySQL, MariaDB | `information_schema.tables.table_rows` | Lecture de catalogue |
| SQL Server | `sys.dm_db_partition_stats` | Lecture de catalogue |
| SQLite | Aucune estimation fiable | Pas d'estimation, total exact seulement |
| DuckDB, MotherDuck | `COUNT(*)` reste peu coûteux en colonnaire | Comptage exact direct |
| ClickHouse | `count()` MergeTree | Compteur de métadonnées |
| MongoDB | `estimatedDocumentCount()` | Métadonnées de collection |
| Elasticsearch, OpenSearch | `_count`, ou `hits.total` avec `track_total_hits` borné | Une requête légère |
| Redis | `HLEN`, `LLEN`, `SCARD`, `ZCARD`, `XLEN` | O(1), déjà exact |

Règles d'affichage :

- toujours préfixer d'un marqueur d'imprécision, jamais un nombre nu ;
- indiquer la fraîcheur lorsque le moteur l'expose (`last_analyze` sur
  PostgreSQL) ;
- une estimation ne remplace jamais un total exact déjà obtenu ;
- une estimation absente ou nulle n'affiche rien, elle n'affiche pas zéro ;
- l'estimation ne bloque jamais la première page, elle arrive après.

Le contrat gagne `count_mode: estimated` et la réponse un
`total_rows_source: exact | estimated | lower_bound`, qui remplace le booléen de
la section 6.4.

Corollaire : l'onglet Info passe par ce chemin, et étiquette comme estimations
les valeurs MySQL et MongoDB qu'il présente aujourd'hui comme des totaux.

### 7.2 Annulation

Le comptage exact est déjà livré et n'est pas annulable. Un `COUNT(*)` lancé par
erreur sur une table de plusieurs centaines de millions de lignes occupe une
connexion jusqu'au timeout, lequel vaut `None` par défaut.

- exposer un bouton d'annulation pendant le comptage, pas seulement un état
  d'attente ;
- brancher sur `CancelSupport` existant, et signaler honnêtement quand
  l'annulation n'est que `BestEffort` ;
- appliquer un timeout par défaut au comptage même quand
  `max_query_duration_ms` est absent : une action explicite ne doit pas pouvoir
  durer indéfiniment ;
- même traitement pour le `COUNT(*)` de l'onglet Info, une fois unifié.

### 7.3 Instrumentation locale

Les mesures de la section 5 ne peuvent exister sans point de collecte. QoreDB
est une application de bureau : rien ne sort de la machine.

- compteurs en mémoire par onglet, exposés dans un panneau de diagnostic ;
- aucune donnée de cellule, aucun contenu de curseur, aucun nom de table ;
- rétention limitée à la session, sauf export explicite pour un rapport de bug.

## 8. Verticale 3 — Coût du tri et de la recherche

Sur un client SQL, la pagination profonde n'est pas le premier coût ressenti. Un
tri sur colonne non indexée et une recherche multi-colonnes le sont, et aucun
des deux n'est traité aujourd'hui.

### 8.1 Recherche

État actuel, chemin PostgreSQL (`pg_compat.rs:1155-1200`), représentatif des
autres drivers SQL :

- une requête `information_schema.columns` est émise à chaque appel, donc à
  chaque page du scroll infini et à chaque frappe débouncée ;
- le prédicat est un `OR` de `LIKE '%terme%'` sur toutes les colonnes non
  binaires, avec un paramètre par colonne ;
- le motif commence par un joker : aucun index B-tree n'est utilisable ;
- sur une table large, cela produit des dizaines de prédicats et un parcours
  complet.

Direction :

- mettre en cache le schéma de colonnes par table et par session, il est déjà
  chargé ailleurs par `describe_table` ;
- restreindre par défaut la recherche aux colonnes textuelles, et rendre le
  périmètre visible et modifiable ;
- proposer une recherche ciblée sur une colonne, qui redevient indexable en
  ancrant le motif (`terme%`) ;
- avertir explicitement lorsque la recherche ne peut pas utiliser d'index, avec
  la raison ;
- pour Elasticsearch et OpenSearch, utiliser une vraie requête `match` plutôt
  qu'une émulation de `LIKE`.

### 8.2 Tri

`ORDER BY colonne_non_indexée LIMIT 100 OFFSET 100000` retrie l'ensemble à
chaque page. Le keyset de la verticale 4 n'y change rien : il exige justement
une clé indexée.

Direction :

- déclarer une capacité de tri par colonne, dérivée des index déjà exposés par
  `describe_table` ;
- signaler dans l'entête de colonne qu'un tri sera coûteux, avant de le
  déclencher ;
- proposer la création de l'index correspondant, le produit sait déjà le faire
  depuis l'onglet Info ;
- ne jamais trier silencieusement côté client sur un sous-ensemble chargé, ce
  qui produirait un ordre faux.

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

### 9.2 Contrat cible

Requête : `cursor`, `direction: forward | backward`, `page_size`.
Réponse : `next_cursor`, `previous_cursor`, `has_more`, `pagination_strategy`,
`ordering_guarantee`.

Le curseur ne doit jamais contenir un fragment SQL fourni par le client. Il est
décodé, typé, borné et validé avant d'être transformé en paramètres de requête.

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

### 9.4 Stratégie par driver

| Drivers | Stratégie préférée | Fallback |
|---|---|---|
| PostgreSQL, Supabase, Neon, TimescaleDB, CockroachDB | Keyset sur clé primaire ou index unique, avec tie-breaker stable | `OFFSET` signalé comme dégradé |
| MySQL, MariaDB | Keyset sur clé primaire ou index unique | `OFFSET` |
| SQLite, DuckDB, MotherDuck | Keyset lorsque le schéma fournit une clé stable | `OFFSET` |
| SQL Server | Keyset avec prédicats paramétrés et ordre unique | `OFFSET/FETCH` |
| MongoDB | `_id` ou couple `(sort_value, _id)` | `skip` |
| Elasticsearch, OpenSearch | `search_after`, avec PIT si une vue cohérente est requise ; le code existe déjà dans `stream_search` | `from/size` dans la fenêtre autorisée |
| ClickHouse | Clé de tri ou clé primaire MergeTree lorsque disponible | `OFFSET` avec avertissement de coût |
| Redis list | Index natif | Pagination actuelle |
| Redis zset | Couple `(score, member)` | Index |
| Redis stream | Identifiant de stream | Pagination actuelle |
| Redis hash et set | Curseur `HSCAN` ou `SSCAN`, sans promettre un ordre stable | Scan depuis le début si nécessaire |

### 9.5 Schémas sans clé stable

C'est le cas fréquent, pas l'exception : vues, vues matérialisées sans index,
tables sans clé primaire, résultats de requête ad hoc. Le comportement par
défaut y est `OFFSET`, avec `ordering_guarantee: none` remonté à l'interface et
affiché comme tel. Une table partitionnée ou une clé composite nullable relève
du même traitement tant que l'unicité n'est pas démontrée.

### 9.6 Interaction avec l'édition

Une ligne modifiée localement peut changer de position dans l'ordre keyset et
donc réapparaître ou disparaître de la fenêtre. À trancher dans cette verticale,
pas dans la suivante :

- une ligne éditée reste ancrée à sa position d'origine jusqu'au prochain
  rechargement explicite ;
- une ligne qui sort du prédicat de filtre après édition est signalée, pas
  masquée silencieusement ;
- une ligne insérée localement reste visible jusqu'au rechargement, même si le
  curseur l'aurait placée ailleurs.

### 9.7 Décision de setting

Il ne faut pas demander à l'utilisateur de choisir « cursor » ou « offset » dans
le parcours normal. QoreDB sélectionne automatiquement la meilleure stratégie
sûre selon le schéma et le driver.

Un réglage avancé « Forcer la stratégie de pagination » ne devient pertinent que
pour diagnostiquer un schéma atypique, contourner temporairement un bug moteur,
comparer des performances, ou préserver un comportement historique dans une
intégration. Il reste par connexion ou par table, jamais une préférence
générale. Il suppose un stockage de réglages par connexion, à vérifier avant de
s'engager.

### 9.8 Critères d'acceptation

- ordre déterministe, ou indication explicite qu'il ne l'est pas ;
- aucune concaténation de valeur de curseur dans le SQL ;
- filtres et tris identiques entre les pages ;
- couverture des clés simples, composites, nullables et des directions mixtes ;
- tests avec insertions et suppressions entre deux pages ;
- fallback contrôlé lorsque la clé de tri n'est pas unique ;
- pas de régression sur l'édition, la sélection ou l'export.

## 10. Verticale 5 — Mémoire et rendu progressif

### 10.1 Fenêtre de lignes bornée

Le scroll infini ne doit pas conserver indéfiniment toutes les lignes.

- conserver une fenêtre autour de la zone visible ;
- évincer les anciens chunks selon un budget de lignes ou de mémoire ;
- conserver les curseurs des chunks évincés, ce qui suppose la verticale 4
  livrée : sans curseur, un chunk évincé se relit par `OFFSET`, donc lentement
  et sans garantie de cohérence ;
- recharger silencieusement un chunk lorsque l'utilisateur remonte ;
- préserver la position, la sélection et les lignes modifiées localement.

### 10.2 Sélection et export sous fenêtre bornée

Sujet le plus risqué de cette verticale, à trancher avant d'écrire le premier
mécanisme d'éviction.

- « tout sélectionner » ne peut plus signifier « toutes les lignes chargées » :
  soit la sélection devient un prédicat (filtres et tri courants), soit elle est
  bornée explicitement à la fenêtre, avec l'indication correspondante ;
- une action de masse sur une sélection-prédicat doit annoncer le nombre de
  lignes concernées avant exécution, ce qui suppose un comptage, donc la
  verticale 2 ;
- l'export ne lit pas la fenêtre : il relit depuis la source avec les mêmes
  filtres et le même tri, en flux, sans passer par l'état de la grille ;
- une ligne modifiée localement et non enregistrée bloque l'éviction de son
  chunk, ou provoque un avertissement avant perte.

### 10.3 Taille de chunk adaptative

Le chunk s'adapte à la latence observée, au poids moyen des lignes, à la vitesse
de scroll et à la mémoire disponible.

Un profil utilisateur à trois crans (Économe, Équilibré, Rapide) a été écarté :
il constituerait une seconde commande sur un algorithme déjà adaptatif, et
l'utilisateur ne peut pas prédire ce que « Rapide » signifie. Le seul réglage
exposé est un plafond mémoire par onglet, dans les réglages avancés, avec un
défaut sûr. La taille de page brute reste un réglage avancé de diagnostic.

### 10.4 Cellules lourdes

Pour JSON, tableaux, texte long, binaire et géométries :

- mesurer la taille avant formatage complet lorsque le protocole le permet ;
- tronquer uniquement la preview, jamais la valeur logique sans indication ;
- afficher taille, type et état « aperçu » ;
- charger ou formater le contenu complet à la demande ;
- déplacer les transformations coûteuses hors du thread de rendu ;
- interdire le rendu direct de HTML ou SVG non fiable ;
- limiter copie, preview et export selon les politiques de sécurité.

### 10.5 Précision des types

Éviter les pertes silencieuses entre moteurs, Rust et JavaScript :

- sérialiser les entiers hors plage sûre JavaScript sans perte ;
- distinguer date, timestamp et timestamp avec fuseau ;
- conserver decimal et numeric sous une représentation précise ;
- préserver binaire, UUID, ObjectId, intervalle et types spécifiques ;
- afficher explicitement les valeurs non supportées au lieu de les convertir en
  chaîne ambiguë.

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

| Option potentielle | Décision |
|---|---|
| Calculer toujours le total exact | Ne pas exposer ; conserver l'action à la demande |
| Afficher une estimation moteur | Retenu, verticale 2, avec provenance et fraîcheur affichées |
| Périmètre de la recherche (colonnes) | Retenu, verticale 3, visible dans la barre de recherche plutôt que dans les réglages |
| Taille de page brute | Réglage avancé de diagnostic uniquement |
| Profil Économe, Équilibré, Rapide | Écarté, cf. 10.3 |
| Forcer cursor ou offset | Diagnostic avancé, par connexion ou table |
| Budget mémoire par onglet | Retenu, réglages avancés, défaut sûr |
| Taille maximale de preview d'une cellule | Retenu, avec plafond imposé par la politique de sécurité |
| Niveau de cohérence ou snapshot | Exposé uniquement dans les workflows qui en ont besoin |

Une option n'est ajoutée que si plusieurs implémentations sont viables, qu'aucune
ne domine clairement, que l'utilisateur en comprend la conséquence, que le choix
s'explique avec un vocabulaire produit, et qu'il ne permet pas de contourner une
limite de sécurité administrateur.

## 13. Ordre de livraison

Séquencé par valeur rendue et par dépendance, en lots livrables indépendamment.

1. Correctifs de la verticale 1 — appliqués, cf. section 6.3.
2. Contrat resserré — `total_rows: number | null`, `total_rows_source`,
   suppression de `total_pages`, décision sur le bridge. À faire tant qu'il n'y a
   qu'un consommateur externe.
3. Tests par famille de drivers, en commençant par Redis et Search.
4. Verticale 2 — instrumentation d'abord, pour disposer de la mesure de
   référence ; puis annulation ; puis estimation moteur.
5. Verticale 3 — cache du schéma de colonnes, puis périmètre de recherche, puis
   capacité de tri.
6. Verticale 4 — capacité déclarée, keyset SQL, MongoDB, branchement de
   `search_after` sur `query_table`, stratégies Redis, fallback et diagnostics.
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
