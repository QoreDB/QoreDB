# RFC — `DataEngine` v2 : abstraction moins SQL-centric

> **Statut** : Draft — en cours de review
> **Auteur** : équipe core QoreDB
> **Créé** : 2026-04-24
> **Dernière maj** : 2026-04-24
> **Référence plan** : `doc/todo/NOSQL_PLAN.md` § 3.1
> **Breaking change** : oui — à planifier pour une release majeure

---

## 1. Résumé

Le trait `DataEngine` et les types de `qore-core::types` ont été conçus quand QoreDB ne ciblait que des bases relationnelles. Avec l'arrivée de MongoDB puis Redis, et après Phase 2 (index management, Lua scripting, `$text`/`$regex`, `bulkWrite`), l'abstraction a été étirée par des hacks localisés. Cette RFC propose une refonte ciblée (pas un rewrite) pour :

1. rendre les primitives NoSQL first-class sans passer par un pseudo-SQL,
2. redonner du typage aux opérations qui sont aujourd'hui des strings JSON,
3. permettre l'ajout futur de change streams, pub/sub, consumer groups sans couche d'adaptation douteuse.

La v1 et la v2 coexistent le temps d'une release ; chaque driver migre à son rythme, et la couche UI reste stable grâce à un module compat.

---

## 2. Non-objectifs

Ce que cette RFC **ne** cherche **pas** à faire :

- Remplacer SQLx ou le driver `mongodb` — on touche uniquement la couche d'abstraction côté `qore-core` / `qore-drivers`.
- Unifier les dialectes SQL entre moteurs (pas un ORM).
- Introduire un langage de requête propriétaire (pas de "QoreQL").
- Casser le format on-wire des sessions existantes ou les connexions enregistrées.

Ces points ont été discutés et exclus volontairement pour garder la refonte contenue (~6–8 semaines de dev, pas 6 mois).

---

## 3. Problèmes actuels (avec références code)

Les pain points ci-dessous ont tous été rencontrés concrètement en Phase 1 et 2.

### 3.1 `FilterOperator` reste SQL-centric

`qore-core/src/types.rs:867` expose un enum de 9 variants calqués sur des opérateurs SQL (`Eq`, `Neq`, `Gt`, `Like`…). Pour 2.3 on a dû ajouter `Regex` et `Text` mais :

- les **flags regex** et la **langue fulltext** sont stockées à côté dans un nouveau struct `FilterOptions` (pour garder `Copy` sur le variant) — ça marche mais l'API est peu naturelle ;
- les opérateurs NoSQL réellement natifs (`$elemMatch`, `$all`, `$size`, `$exists`, `$type`) ne peuvent pas être exprimés sans rouvrir l'enum ;
- chaque driver doit éviter tout `NotSupported` (cf. décision d'archi du 2026-04-24), donc ajouter un variant oblige à modifier les 8 drivers en même temps.

### 3.2 Opérations MongoDB passent par un JSON stringifié

`drivers/mongodb.rs:678` parse `query: &str` comme un `serde_json::Value`, puis fait un grand `match op.as_str()` sur des strings (`"createindex"`, `"bulkwrite"`, `"findoneandupdate"`…). Résultat :

- **pas de typage statique** côté frontend (le client TS doit produire le bon JSON à la main) ;
- les erreurs de shape sont rapportées à l'exécution seulement (`"Missing 'database' field"`) ;
- chaque nouvelle opération ajoute une branche dans un match long de ~1000 lignes ;
- le dispatch lowercases + strip underscores pour être tolérant → source de bugs silencieux.

### 3.3 `TableSchema` ne représente pas bien le non-relationnel

```rust
pub struct TableSchema {
    pub columns: Vec<TableColumn>,
    pub primary_key: Option<Vec<String>>,
    pub foreign_keys: Vec<ForeignKey>,
    pub row_count_estimate: Option<u64>,
    pub indexes: Vec<TableIndex>,
}
```

- **MongoDB** n'a ni PK ni FK stricte ; on remplit `primary_key: Some(vec!["_id"])` par convention.
- **Redis** n'a pas du tout de schéma ; le driver fabrique des `TableColumn` synthétiques (`type`, `encoding`, `ttl`) pour que l'UI ne soit pas vide.
- Les **colonnes virtuelles** (score `$meta: "textScore"`, computed fields) ne sont pas représentables : `QueryResult.columns` vient du schéma stocké, pas de la requête.

### 3.4 `DataEngine` suppose un flow SELECT → rows

Le trait force un modèle `query → QueryResult { columns, rows }` paginé. Cela ne colle pas pour :

- **MongoDB change streams** (3.3) : un cursor infini qui émet des events typés.
- **Redis Pub/Sub** (3.2) : des messages asynchrones poussés vers l'UI.
- **Redis consumer groups** (3.2) : lecture incrémentale avec ack manuel.

Le `StreamSender` existant (`traits.rs`) est pensé pour streamer les rows d'un SELECT long, pas pour un flux événementiel.

### 3.5 `Value` est lossy pour BSON / types riches

```rust
pub enum Value {
    Null, Bool(bool), Int(i64), Float(f64),
    Text(String), Bytes(Vec<u8>),
    Json(serde_json::Value), Array(Vec<Value>),
}
```

- `ObjectId`, `Decimal128`, `Timestamp`, `Regex`, `MinKey`/`MaxKey` BSON tombent tous dans `Json(…)` ou `Text(…)` et perdent leur type.
- L'UI affiche un `ObjectId` comme `"{$oid: ..."}`, pas comme un handle cliquable.
- Impossible de distinguer un int64 d'un int32 (toujours `Int(i64)`).
- Pas de support `Uuid` natif (vient en `Text`).

### 3.6 Capabilities implicites

Le trait expose `supports_transactions()`, `supports_mutations()`, `supports_streaming()`, `supports_maintenance()`. Mais :

- la liste n'est pas extensible sans modifier le trait (un ajout casse tous les drivers),
- elle ne dit pas **quoi** exactement (ex: MongoDB supporte les transactions **si replica set**, sinon non — le bool ment),
- la couche UI ne peut pas faire de feature-detection fine-grained.

---

## 4. Design proposé

### 4.1 Vue d'ensemble

```
┌─────────────────────────────────────────────────────────────────┐
│                     Frontend (TS)                                │
│   Builders typés par driver (SqlBuilder, MongoBuilder, ...)      │
└───────────────────────────┬─────────────────────────────────────┘
                            │ Tauri invoke (payload typé)
┌───────────────────────────▼─────────────────────────────────────┐
│                    qore-core v2                                  │
│  ┌──────────────┐  ┌──────────────┐  ┌────────────────────────┐ │
│  │ Capabilities │  │ Operation    │  │ ResultStream           │ │
│  │   (bitset)   │  │   (enum)     │  │  (items | events)      │ │
│  └──────┬───────┘  └──────┬───────┘  └───────────┬────────────┘ │
│         └──────────┬──────┴──────────────────────┘              │
│                    ▼                                            │
│          trait DataEngine v2                                    │
│          { execute(Operation) -> ResultStream }                 │
└───────────────────────────┬─────────────────────────────────────┘
                            │
      ┌────────┬────────┬───┴────┬────────┬────────┐
      │        │        │        │        │        │
   Postgres  MySQL  SQLite  DuckDB  MongoDB  Redis
     (v2)    (v2)    (v2)    (v2)    (v2)    (v2)
```

### 4.2 Capabilities — bitset versionné

Remplace les `supports_*()` par une structure dédiée, étendue sans breaking change :

```rust
// qore-core/src/capabilities.rs
#[derive(Debug, Clone, Default)]
pub struct DriverCapabilities {
    pub schema_version: u32,          // bumpé à chaque ajout de flag
    pub transactions: Tri,            // Yes / No / Conditional(&str)
    pub streaming_query: bool,        // un SELECT peut être streamé par batch
    pub change_streams: bool,         // stream d'events typés (Mongo)
    pub pub_sub: bool,                // Redis pub/sub
    pub consumer_groups: bool,        // Redis streams / XREADGROUP
    pub aggregation_pipeline: bool,   // Mongo $group / PG window func
    pub full_text_native: bool,       // index fulltext natif
    pub regex_native: bool,           // opérateur regex natif
    pub explain_plan: bool,
    pub bulk_write: bool,
    pub upsert: bool,
    // … extensible
}

pub enum Tri {
    Yes,
    No,
    Conditional(&'static str),  // ex: "requires replica set"
}
```

- **Versioning** : `schema_version` permet à l'UI de savoir si un flag qu'elle interroge existe dans le driver chargé.
- **Tri-state** : un `Conditional` force l'UI à gérer le cas limite plutôt que de traiter un `true` trompeur.

### 4.3 `Operation` — enum typé en remplacement du dispatch par string

```rust
// qore-core/src/operation.rs
pub enum Operation {
    // Lectures
    Query(QueryOp),                    // SELECT / find
    Aggregate(AggregateOp),            // pipeline générique
    DescribeSchema(DescribeSchemaOp),

    // Mutations
    Insert(InsertOp),
    Update(UpdateOp),
    Delete(DeleteOp),
    Upsert(UpsertOp),
    BulkWrite(BulkWriteOp),
    FindAndModify(FindAndModifyOp),

    // Admin
    CreateIndex(CreateIndexOp),
    DropIndex(DropIndexOp),
    CreateCollection(CreateCollectionOp),
    DropCollection(DropCollectionOp),

    // Subscriptions (nouveaux)
    Watch(WatchOp),                    // change streams Mongo
    Subscribe(SubscribeOp),            // Redis pub/sub
    ConsumerRead(ConsumerReadOp),      // Redis XREADGROUP

    // Fallback
    RawText { text: String, lang: RawLang },  // SQL / Lua / shell
}
```

Chaque variant est un struct nommé avec les champs nécessaires. Le dispatcher driver-side devient un `match op { … }` checké par le compilateur — fini le `match op.as_str()` et les strings typos.

**Point ouvert** : le SQL libre (`RawText`) reste nécessaire. Les drivers SQL mappent `Query(QueryOp)` vers un SQL interne, mais l'utilisateur peut toujours écrire du SQL ad-hoc via `RawText`.

### 4.4 `FilterExpr` — arbre d'expression, plus un opérateur plat

Remplace `ColumnFilter { column, operator, value, options }` par un arbre :

```rust
pub enum FilterExpr {
    // Comparaisons
    Eq(FieldPath, Value),
    Neq(FieldPath, Value),
    Gt(FieldPath, Value),
    Gte(FieldPath, Value),
    Lt(FieldPath, Value),
    Lte(FieldPath, Value),

    // String
    Like(FieldPath, String),
    Regex { field: FieldPath, pattern: String, flags: RegexFlags },
    Text { query: String, language: Option<String> },  // scope global

    // Collection / document
    In(FieldPath, Vec<Value>),
    NotIn(FieldPath, Vec<Value>),
    Exists(FieldPath),
    NotExists(FieldPath),
    ElemMatch { field: FieldPath, inner: Box<FilterExpr> },
    Size(FieldPath, u32),
    TypeOf(FieldPath, TypeTag),

    // Logique
    And(Vec<FilterExpr>),
    Or(Vec<FilterExpr>),
    Not(Box<FilterExpr>),
}

pub struct FieldPath(pub Vec<String>);  // ex: ["address", "city"]
pub struct RegexFlags { pub i: bool, pub m: bool, pub x: bool, pub s: bool }
```

Chaque driver traduit `FilterExpr` en son dialecte natif. Les opérateurs qu'il ne sait pas rendre nativement sont soit :
- convertis en fallback équivalent (ex: `Text` → `LIKE '%…%'` sur SQLite — comportement actuel conservé),
- ou, s'il n'y a pas de mapping raisonnable, retourne une `EngineError::UnsupportedFilter(…)` **à la traduction**, pas à l'exécution — la couche UI peut ainsi désactiver les opérateurs qu'un driver ne supporte pas, par driver, via `DriverCapabilities` enrichies.

### 4.5 `Row` / `QueryResult` — colonnes virtuelles et meta

```rust
pub struct QueryResult {
    pub columns: Vec<ColumnInfo>,       // schéma source
    pub virtual_columns: Vec<ColumnInfo>, // score, computed, meta
    pub rows: Vec<Row>,
    pub affected_rows: Option<u64>,
    pub warnings: Vec<ResultWarning>,   // nouveaux
    pub timing: TimingInfo,
}

pub struct Row {
    pub values: Vec<Value>,              // index par columns
    pub virtual_values: Vec<Value>,      // index par virtual_columns
}

pub struct ResultWarning {
    pub code: &'static str,              // "text_fallback_to_like"
    pub message: String,
    pub driver: &'static str,
}
```

- **Colonnes virtuelles** = résolution propre du pain point 2.3 (propagation du `textScore`).
- **Warnings** remplacent le silence ou les `tracing::warn!` qu'on ajoute au compte-gouttes. La couche UI peut afficher "⚠ fallback LIKE sur SQLite car pas d'index text".
- Rétro-compat : les clients v1 qui regardent `rows[].values` voient exactement les mêmes données que maintenant.

### 4.6 `Value` v2 — types riches optionnels

```rust
pub enum Value {
    Null,
    Bool(bool),
    Int32(i32),
    Int64(i64),
    Float64(f64),
    Decimal128(Decimal128),              // nouveau — Mongo / PG numeric
    Text(String),
    Bytes(Vec<u8>),
    Uuid(Uuid),                          // nouveau — PG uuid
    ObjectId([u8; 12]),                  // nouveau — Mongo _id
    Timestamp(UnixNanos),                // nouveau
    Date(NaiveDate),
    Time(NaiveTime),
    DateTime(OffsetDateTime),
    Interval(Duration),
    Array(Vec<Value>),
    Document(BTreeMap<String, Value>),   // remplace Json(…) côté Mongo
    Json(serde_json::Value),             // conservé pour PG jsonb, etc.
}
```

- `ObjectId` / `Uuid` / `Decimal128` gagnent un affichage natif côté UI.
- `Document` permet de ne plus roundtripper via `serde_json::Value` pour les docs Mongo.
- Rétro-compat wire : les clients v1 qui `match` sur `Int(…)` continueront à recevoir `Int64` via une alias `Int = Int64` pour une release.

### 4.7 Streaming — `ResultStream` unifié

```rust
pub enum ResultStream {
    Rows(Box<dyn Stream<Item = Result<Row>> + Send + Unpin>),
    Events(Box<dyn Stream<Item = Result<Event>> + Send + Unpin>),
    Batches(Box<dyn Stream<Item = Result<QueryResult>> + Send + Unpin>),
}

pub enum Event {
    ChangeStream(ChangeEvent),           // Mongo
    PubSub(PubSubMessage),               // Redis
    ConsumerMessage(ConsumerMessage),    // Redis streams
}
```

`DataEngine::execute` retourne `Result<ResultStream>`. La couche Tauri ponte vers un channel unique par type. Les opérations SELECT rendent `Rows`, les change streams `Events`, les bulk rendent `Batches` — chacun avec sa sémantique dédiée.

### 4.8 Trait `DataEngine` v2

```rust
#[async_trait]
pub trait DataEngineV2: Send + Sync {
    fn capabilities(&self) -> &DriverCapabilities;

    async fn execute(
        &self,
        session: SessionId,
        op: Operation,
        ctx: ExecContext,
    ) -> EngineResult<ResultStream>;

    async fn describe_schema(
        &self,
        session: SessionId,
        scope: SchemaScope,
    ) -> EngineResult<Option<SchemaDescription>>;

    async fn cancel(
        &self,
        session: SessionId,
        query_id: QueryId,
    ) -> EngineResult<()>;

    // Transactions restent factorisées
    async fn begin(&self, session: SessionId, opts: TxOptions) -> EngineResult<()>;
    async fn commit(&self, session: SessionId) -> EngineResult<()>;
    async fn rollback(&self, session: SessionId) -> EngineResult<()>;
}
```

`ExecContext` porte le `query_id`, le flag `acknowledged_dangerous`, le `timeout`, le `namespace` — aujourd'hui passé en paramètres dispersés sur `execute_query`.

---

## 5. Plan de migration

### 5.1 Principe : v1 et v2 coexistent

Pendant une release (cible : 0.3.x), les deux traits vivent dans le même crate. La v1 est marquée `#[deprecated]` mais reste fonctionnelle. Les drivers migrent un à un et publient `impl DataEngineV1 + DataEngineV2` en parallèle pendant leur migration.

### 5.2 Ordre de migration proposé

1. **SQLite** (périmètre limité, tests rapides) — valide le design sur un driver SQL simple.
2. **PostgreSQL / pg_compat** — premier gros driver, migre aussi CockroachDB qui délègue.
3. **MySQL / MariaDB** — même groupe.
4. **DuckDB, SQL Server** — complète le front SQL.
5. **MongoDB** — driver avec le plus à gagner du refactor (supprime le match sur string).
6. **Redis** — dernier, car c'est celui qui bénéficie le plus des nouveaux types d'Operation (Subscribe/ConsumerRead).

Chaque driver migré reçoit une PR dédiée avec ses tests de non-régression.

### 5.3 Couche UI

Le frontend crée des **builders** par driver (`SqlQueryBuilder`, `MongoQueryBuilder`, `RedisCommandBuilder`) qui construisent des `Operation` typées. Pendant la transition :

- `executeQuery(sessionId, query: string, opts)` reste exposé (rétro-compat) et wrappe un `Operation::RawText { text: query, lang: Auto }`.
- `executeOperation(sessionId, op: Operation, opts)` devient la nouvelle API préférée.

### 5.4 Déprécation

- Release N : v2 disponible, tous drivers fournissent les deux impls.
- Release N+1 : v1 marquée `#[deprecated]`, warnings à la compilation.
- Release N+2 : v1 supprimée.

---

## 6. Trade-offs

| Option retenue | Alternative | Raison du choix |
| --- | --- | --- |
| Coexistence v1/v2 via trait séparé | Rewrite direct, tag release majeure | Permet de livrer incrémental, testable driver-par-driver. Un rewrite direct laisserait `main` cassé pendant des semaines. |
| Enum `Operation` avec ~15 variants | Trait `Operation` polymorphe | Enum = match exhaustif, meilleur rapport lisibilité/flex. Trait polymorphe force le boxing partout. |
| `FilterExpr` arbre récursif | Garder `ColumnFilter` plat + callbacks | Arbre permet logique composée (`$and`/`$or`/`$not`) qui manque aujourd'hui. Pas plus coûteux à sérialiser. |
| `Value` étendu avec types natifs | Tout passer par `Json` | Types natifs = affichage UI correct, pas de roundtrip lossy. Coût : ~5 variants en plus, gérés une fois. |
| Warnings dans `QueryResult` | Logs `tracing` uniquement | Les warnings suivent la query et remontent à l'UI, pas juste à l'ops. |

---

## 7. Questions ouvertes

1. **`RawText` pour MongoDB** : est-ce qu'on garde la possibilité de passer du JSON brut ? Avantage : rétro-compat totale avec les snippets Phase 2. Inconvénient : perpétue le pattern "JSON parsé à runtime".
2. **`Document` vs `Json` dans `Value`** : deux variants pour des usages proches. On peut soit fusionner (`Json` devient le seul, on perd le BSON ordering), soit garder séparés (cohérent avec la sémantique Mongo).
3. **Cancellation des streams** : un `Watch` qui tourne 10 minutes doit-il être annulable via `cancel(query_id)` comme un SELECT ? Probablement oui, mais nécessite un `AbortHandle` par stream — à clarifier.
4. **Plugin system tiers** : la v2 rend théoriquement possible un système de plugins (driver compilé séparément). Hors scope immédiat, mais la conception devrait le laisser ouvert (pas de types internes fuit, pas de `'static` lifetimes inutiles).
5. **Compat TypeScript** : les types Rust génèrent actuellement du TS manuellement dans `src/lib/tauri.ts`. L'arbre `FilterExpr` + `Operation` est complexe ; envisager `ts-rs` ou `tauri-specta` pour la génération auto.

---

## 8. Exemples

### 8.1 Find MongoDB avec filter composé

**v1 (actuel)** :

```json
{
  "operation": "find",
  "database": "app",
  "collection": "users",
  "filter": { "$and": [ { "age": { "$gt": 18 } }, { "country": "FR" } ] }
}
```

**v2** :

```rust
Operation::Query(QueryOp {
    namespace: Namespace::mongo("app", "users"),
    filter: FilterExpr::And(vec![
        FilterExpr::Gt(FieldPath::of("age"), Value::Int32(18)),
        FilterExpr::Eq(FieldPath::of("country"), Value::Text("FR".into())),
    ]),
    projection: None,
    sort: None,
    paging: Paging::default(),
})
```

### 8.2 Change stream MongoDB

**v1 (actuel)** : non supporté (nécessite le pattern du plan 3.3).

**v2** :

```rust
let stream = engine.execute(
    session,
    Operation::Watch(WatchOp {
        scope: WatchScope::Collection(ns),
        pipeline: vec![],
        resume_token: None,
    }),
    ctx,
).await?;

match stream {
    ResultStream::Events(mut s) => {
        while let Some(event) = s.next().await {
            // … push Tauri event to UI
        }
    }
    _ => unreachable!("Watch returns Events"),
}
```

### 8.3 Redis EVAL (remplace le string-builder de Phase 2.2)

**v1 (actuel)** : `buildEvalScript({ script, keys, args })` concatène une string `EVAL "…" 2 k1 k2 a1`.

**v2** :

```rust
Operation::RawText {
    text: script.to_string(),
    lang: RawLang::Redis(RedisRawCmd::Eval { keys, args }),
}
```

Le driver Redis a désormais un type pour "EVAL avec KEYS + ARGV" ; fini le quoting côté frontend.

---

## 9. Métriques de succès

Après migration complète :

- **Lignes de code** : `drivers/mongodb.rs` doit passer sous 1500 lignes (actuel : 2205) par suppression du match sur string.
- **Exhaustive match** : tous les dispatchers d'opération checkés par le compilateur (zéro fallback `_ =>` sur strings).
- **Types propagés bout-en-bout** : zéro `string op` dans la Tauri invoke pour les 8 drivers.
- **Couverture capabilities** : chaque feature UI (change streams, pub/sub, bulkWrite…) est gated par un flag de `DriverCapabilities`, testé dans chaque driver.
- **Tests** : au minimum non-régression des 107 tests unitaires actuels + 1 suite par driver pour `FilterExpr → SQL natif` (même en absence d'infra d'intégration, la traduction est pure et testable).

---

## 10. Prochaines étapes

1. **Review de cette RFC** par les mainteneurs (SQL + NoSQL, frontend).
2. **Prototype SQLite** (~1 semaine) pour valider le design sur un driver complet.
3. **Décider de la release cible** (vraisemblablement 0.3.0 en tant que majeure interne).
4. **Ouvrir une sous-tâche par driver** dans `NOSQL_PLAN.md` § 3.1.
5. **Annoncer la dépréciation v1** dans `CHANGELOG.md` dès la release N pour préparer les consommateurs externes (MCP, CLI future).

---

_Fin du document draft. Les commentaires et contre-propositions sont attendus en PR avant ouverture du prototype SQLite._
