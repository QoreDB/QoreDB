# QoreQuery — Plan Phase 2 (Query Builder)

> **Phase 2** de la roadmap QoreORM. Pré-requis : Phase 1 (`qore-core`, `qore-sql`, `qore-drivers` extraits) — ✅ terminée sur `feat/core-extraction`.

## Table des matières

0. [État d'avancement — MVP v0.1](#0-état-davancement--mvp-v01)
1. [Vision et objectifs](#1-vision-et-objectifs)
2. [Structure du crate `qore-query`](#2-structure-du-crate-qore-query)
3. [Modèle de données — `Column`, `Expr`, `Value`](#3-modèle-de-données)
4. [Compilation multi-dialecte](#4-compilation-multi-dialecte)
5. [Paramétrisation et sécurité](#5-paramétrisation-et-sécurité)
6. [Intégration progressive dans l'app](#6-intégration-progressive)
7. [Ordre d'implémentation MVP](#7-ordre-dimplémentation-mvp)
8. [Pièges et points d'attention](#8-pièges)
9. [Stratégie de tests](#9-tests)
10. [Roadmap vers Phase 3 (ORM)](#10-roadmap-phase-3-après-le-mvp)
11. [Plan de reprise — v0.2 et au-delà](#11-plan-de-reprise--v02-et-au-delà)

---

## 0. État d'avancement — MVP v0.1

> **Statut global : MVP v0.1 ✅ TERMINÉ** sur la branche `feat/core-extraction`.
> Le crate `qore-query` compile, tests verts, clippy strict clean.
> **Aucune intégration dans l'app QoreDB à ce jour** — c'est v0.2+ (cf. §11).

### Deliverables vs plan initial

| Semaine | Plan initial | Livré | Divergences notables |
| --- | --- | --- | --- |
| Sem 1 | Squelette crate + `Column<T>` + `Expr` de base | ✅ | — |
| Sem 2 | `SelectQuery` + WHERE + `SqlCompiler` | ✅ | — |
| Sem 3 | 4 dialectes + placeholders | ✅ + DuckDB (5ᵉ) | Trait `DialectOps` introduit, alias CockroachDB + MariaDB via `from_driver_id` |
| Sem 4 | JOINs + ORDER BY + LIMIT/OFFSET | ✅ | Trait `IntoOperand` introduit pour accepter colonnes/subqueries/littéraux uniformément |
| Sem 5 | Opérateurs complets | ✅ + text helpers | `starts_with`/`ends_with`/`contains` avec `ESCAPE '\\'` portable ; `and_all`/`or_any` combinators ; refactor `LIKE` vers `Expr::Like` avec flag `case_insensitive` + `escape` |
| Sem 6 | Subqueries + CAST + COALESCE + alias | ✅ | Macro `coalesce!` pour args hétérogènes ; `FromSource` enum Table\|Subquery ; `SelectItem::Projection` unifié |
| Sem 7 | Aggregates + GROUP BY/HAVING + proptest + doc | ✅ | 6 fonctions libres (`count`/`count_all`/`count_distinct`/`sum`/`avg`/`min`/`max`) ; validation stricte "HAVING sans GROUP BY" |

### Surface API livrée

- **5 dialectes** : Postgres, MySQL/MariaDB, SQLite, SQL Server, DuckDB (7/9 drivers QoreDB couverts)
- **12 variants `Expr`** : Column, Literal, Binary, Unary, InList, Between, Like, Subquery, InSubquery, Exists, Cast, Coalesce, Aggregate, CountStar
- **18 méthodes `Column<T>`** : eq/ne/gt/ge/lt/le, like/ilike, starts_with/ends_with/contains, is_null/is_not_null, in_/not_in/between, in_sub/not_in_sub, cast
- **27 méthodes `SelectQuery`** : from/from_as/from_subquery, all/columns/column/column_as/select_expr/select_expr_as, filter, group_by/group_by_qualified/group_by_expr, having, order_by/order_by_qualified/order_by_nulls, limit/offset, 4×(inner/left/right/full)_join(_as), build
- **13 fonctions libres** : col/tcol, cast/coalesce/coalesce!, exists/not_exists, count/count_all/count_distinct/sum/avg/min/max
- **6 variants `QueryError`** : MissingFrom, EmptyProjection, InvalidLiteral, InvalidExpr, Unsupported, MssqlOffsetRequiresOrderBy, AstTooDeep, TooManyParameters
- **Bornes de sécurité** : `MAX_AST_DEPTH = 1024`, `MAX_PARAMS = 65_535`

### Tests

- **107 unit/integration** sur 8 fichiers
- **3 proptest** × 256 cases/property = ~768 vérifs par run (compilation totale, SQL re-parseable via `sqlparser`, `params.len() == nb_placeholders`)
- **4 doctests** (incl. exemple headline de `lib.rs` et `coalesce!` macro)
- **`cargo clippy -p qore-query --all-targets -- -D warnings`** clean

### Topologie

```text
qore-core  ← types universels (Value, RowData, DataEngine trait)
   ↑
qore-query ← query builder (CETTE PHASE, standalone, zéro dep app)
   ↑
(futur) qore-orm ← macros derive Model, hydratation (Phase 3)

qore-sql   ← outils SQL pour sandbox (indépendant de qore-query)
qore-drivers ← implémentations Postgres/MySQL/… (indépendant)
```

**`qore-query` ne dépend que de `qore-core`.** Aucune dépendance circulaire, pas de couplage avec les drivers ni avec qore-sql.

---

## 1. Vision et objectifs

Un query builder Rust **type-safe**, **multi-dialecte** et **sans injection possible**, posé sur les briques déjà extraites :

- `qore_sql::SqlDialect` — quoting d'identifiants, format des valeurs littérales
- `qore_core::Value` — représentation universelle des valeurs
- `qore_core::DataEngine` — exécuteur final (branché en v0.4)

### API cible (MVP)

```rust
use qore_query::prelude::*;

let q = Query::select()
    .from("users")
    .columns(["id", "name", "email"])
    .filter(col("age").gt(18).and(col("active").eq(true)))
    .order_by("name", Order::Asc)
    .limit(10)
    .build(Dialect::Postgres)?;

// q.sql    = SELECT "id", "name", "email" FROM "users"
//            WHERE ("age" > $1 AND "active" = $2)
//            ORDER BY "name" ASC LIMIT 10
// q.params = [Value::Int(18), Value::Bool(true)]
```

### Non-objectifs (hors scope Phase 2)

- Macros `#[derive(Model)]` — c'est Phase 3
- Hydratation vers structs — Phase 3
- Migrations / DDL (CREATE TABLE, ALTER) — hors roadmap pour l'instant
- Generators TypeScript — Phase 4

---

## 2. Structure du crate `qore-query`

```text
src-tauri/crates/qore-query/
├── Cargo.toml              # dep: qore-core, qore-sql
├── src/
│   ├── lib.rs              # re-exports, prelude
│   ├── prelude.rs          # usage typique : use qore_query::prelude::*
│   ├── error.rs            # QueryError (Unsupported, InvalidExpr, ...)
│   ├── ident.rs            # Column<T>, Table (phantom-typed)
│   ├── built.rs            # BuiltQuery { sql, params: Vec<Value> }
│   ├── expr/
│   │   ├── mod.rs          # Expr enum, precedence
│   │   ├── ops.rs          # eq/gt/lt/and/or/in/like/ilike/between/is_null/not
│   │   └── fn_.rs          # COUNT, SUM, AVG, MIN, MAX, COALESCE, CAST
│   ├── query/
│   │   ├── mod.rs          # Query entry (select/insert/update/delete)
│   │   ├── select.rs       # SelectQuery (MVP focus)
│   │   ├── insert.rs       # v0.2
│   │   ├── update.rs       # v0.2
│   │   ├── delete.rs       # v0.2
│   │   ├── join.rs         # INNER/LEFT/RIGHT/FULL
│   │   ├── order.rs        # Order + NullOrder
│   │   ├── limit.rs        # Limit + Offset
│   │   └── group.rs        # GROUP BY, HAVING (v0.3)
│   └── compiler/
│       ├── mod.rs          # trait QueryCompiler
│       ├── sql.rs          # SqlCompiler — impl commun via SqlDialect
│       ├── postgres.rs     # overrides : $1 placeholders, ILIKE, RETURNING
│       ├── mysql.rs        # ? placeholders, backticks, pas de FULL JOIN
│       ├── sqlite.rs       # ? placeholders, pas de FULL JOIN, pas de RIGHT JOIN
│       └── mssql.rs        # @p1, OFFSET..FETCH NEXT, pas de LIMIT
└── tests/
    ├── select_postgres.rs  # snapshots insta
    ├── select_mysql.rs
    ├── select_sqlite.rs
    ├── select_mssql.rs
    └── proptest.rs         # fuzz structurel (SQL toujours bien formé)
```

### `Cargo.toml`

```toml
[package]
name = "qore-query"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
description = "Type-safe multi-dialect SQL query builder for QoreDB"

[dependencies]
qore-core = { path = "../qore-core" }
qore-sql  = { path = "../qore-sql" }
serde = { workspace = true }
thiserror = { workspace = true }

[dev-dependencies]
insta = "1"
proptest = "1"
```

---

## 3. Modèle de données

### 3.1 `Column<T>` — phantom-typed

Dès le MVP, on capture le type de colonne via un paramètre générique (`T = Value` par défaut pour l'usage "untyped"). Cela laisse Phase 3 générer des `Column<i32>` / `Column<String>` via macros **sans casser la surface API**.

```rust
pub struct Column<T = Value> {
    pub(crate) name: Cow<'static, str>,
    pub(crate) table: Option<Cow<'static, str>>,
    _marker: PhantomData<T>,
}

pub fn col(name: impl Into<Cow<'static, str>>) -> Column<Value> {
    Column { name: name.into(), table: None, _marker: PhantomData }
}

impl<T: Into<Value> + Clone> Column<T> {
    pub fn eq(self, v: T) -> Expr { Expr::binary(self, BinOp::Eq, v.into()) }
    pub fn gt(self, v: T) -> Expr { Expr::binary(self, BinOp::Gt, v.into()) }
    // ...
}
```

### 3.2 `Expr` — arbre d'expression

```rust
pub enum Expr {
    Column(ColumnRef),                          // référence de colonne
    Literal(Value),                             // valeur paramétrable
    Binary(Box<Expr>, BinOp, Box<Expr>),        // op binaire (eq, gt, and, or, ...)
    Unary(UnOp, Box<Expr>),                     // NOT, IS NULL, IS NOT NULL
    InList(Box<Expr>, Vec<Expr>),               // IN (...)
    Between(Box<Expr>, Box<Expr>, Box<Expr>),   // BETWEEN x AND y
    Function(FnName, Vec<Expr>),                // COUNT, SUM, COALESCE, CAST
    Subquery(Box<SelectQuery>),                 // (SELECT ...)
    Raw(String),                                // échappatoire — usage interne uniquement
}
```

**Règle d'or** : jamais de `Raw` exposé à l'API publique sans un `unsafe_raw` explicite pour empêcher l'injection involontaire.

### 3.3 Opérateurs

| Catégorie | Opérateurs |
| --- | --- |
| Comparaison | `eq`, `ne`, `gt`, `ge`, `lt`, `le` |
| Logique | `and`, `or`, `not` |
| Null | `is_null`, `is_not_null` |
| Texte | `like`, `ilike` (dialect-specific fallback), `starts_with`, `ends_with`, `contains` |
| Collection | `in_`, `not_in`, `between` |
| Agrégats | `count`, `sum`, `avg`, `min`, `max` (v0.3) |
| Fonctions | `coalesce`, `cast` (v0.3) |

---

## 4. Compilation multi-dialecte

### 4.1 Trait `QueryCompiler`

```rust
pub trait QueryCompiler {
    fn compile_select(&self, q: &SelectQuery) -> Result<BuiltQuery, QueryError>;
    fn compile_insert(&self, q: &InsertQuery) -> Result<BuiltQuery, QueryError>;  // v0.2
    fn compile_update(&self, q: &UpdateQuery) -> Result<BuiltQuery, QueryError>;  // v0.2
    fn compile_delete(&self, q: &DeleteQuery) -> Result<BuiltQuery, QueryError>;  // v0.2
}
```

Extension point pour Phase 2.5 (NoSQL) : un `MongoCompiler` qui implémente `QueryCompiler` mais produit un pipeline d'agrégation BSON au lieu de SQL.

### 4.2 `SqlCompiler` — impl générique

```rust
pub struct SqlCompiler {
    pub dialect: SqlDialect,   // réutilise qore_sql::SqlDialect
}
```

Toute la logique commune (quoting, ordre des clauses, parenthésage) vit ici. Les dialectes spécifiques **n'overrident que** ce qui diffère réellement.

### 4.3 Overrides par dialecte

| Dialecte | Différences clés |
| --- | --- |
| **Postgres** | Placeholders `$N`, `ILIKE` natif, `RETURNING`, arrays `= ANY(...)` |
| **MySQL** | Placeholders `?`, backticks, `ILIKE` → `LOWER() LIKE LOWER()`, pas de `FULL JOIN` |
| **SQLite** | Placeholders `?`, pas de `RIGHT`/`FULL JOIN`, pas de `ILIKE` (texte insensible par défaut si COLLATE NOCASE) |
| **MSSQL** | Placeholders `@p1`, brackets `[...]`, `OFFSET x ROWS FETCH NEXT y ROWS ONLY`, pas de `LIMIT` |

---

## 5. Paramétrisation et sécurité

- **Tout littéral devient un paramètre.** Zéro interpolation de string dans le SQL généré.
- Sortie : `BuiltQuery { sql: String, params: Vec<Value> }` — consommé par `DataEngine::execute(sql, &params)`.
- Les placeholders sont générés dans l'ordre par le compiler (index croissant pour PG/MSSQL, `?` séquentiel pour MySQL/SQLite).
- Les identifiants (noms de tables/colonnes) sont **toujours quotés** via `SqlDialect::quote_ident`.
- `Expr::Raw` n'est accessible qu'en `pub(crate)` au MVP. Si exposé plus tard, via une API `unsafe_raw(...)` explicite.

---

## 6. Intégration progressive

| Version | Livrable | Intégration dans l'app QoreDB |
| --- | --- | --- |
| **v0.1 (MVP)** | SELECT complet, 4 dialectes, paramétré | Aucune — crate isolé, validé par tests |
| **v0.2** | INSERT / UPDATE / DELETE | Remplace `qore_sql::generator` côté sandbox |
| **v0.3** | Subqueries, CTE, aggregates, GROUP BY/HAVING | Query runner interne (pagination browser, filtres table) |
| **v0.4** | `.fetch(&engine)` / `.stream(&engine)` async | Résultats browser, features time-travel |
| **v0.5** | `MongoCompiler` | Driver Mongo utilise le builder |

**Rationale** : lib pure au MVP pour solidifier l'API via tests snapshots/proptest ; on intègre seulement quand le contrat est stable. Évite de se coincer dans des call sites câblés trop tôt, ce qui compliquerait les itérations d'API.

---

## 7. Ordre d'implémentation MVP

Durée cible : **~7 semaines** (~36 jours de travail). Séquencé pour qu'à chaque fin de semaine on ait un artefact testable.

| Sem | Livrable | Tests attendus |
| --- | --- | --- |
| 1 | Squelette crate, `Value` wrap, `Column<T>`, `Expr` de base | unitaires sur constructeurs |
| 1-2 | `SelectQuery` + WHERE (eq, and, or) + `SqlCompiler` générique | snapshots PG basiques |
| 2-3 | Placeholders 4 dialectes + quoting | snapshots × 4 dialectes |
| 3-4 | JOINs (INNER/LEFT/RIGHT/FULL), ORDER BY, LIMIT/OFFSET | MSSQL `OFFSET..FETCH` ; SQLite erreurs sur FULL JOIN |
| 4-5 | Opérateurs complets (in, like/ilike, between, is_null, not) | MySQL fallback `ILIKE` |
| 5-6 | Subqueries (WHERE/FROM), CAST, COALESCE, alias de table et colonne | snapshots subquery |
| 6-7 | Aggregates + GROUP BY/HAVING, proptest, doc publique | proptest : SQL toujours parseable par `sqlparser` |

Chaque semaine = 1 commit (ou plus) sur `feat/qore-query`.

---

## 8. Pièges

| Piège | Mitigation |
| --- | --- |
| Précédence AND/OR ambiguë | Parenthèses systématiques autour des groupes, pas de best-effort |
| MSSQL `OFFSET` sans `ORDER BY` illégal | Erreur compile-time impossible → `QueryError::MssqlOffsetRequiresOrderBy` runtime |
| `IN (NULL, ...)` ne matche pas NULL | Ne jamais inférer `OR x IS NULL` ; doc explicite |
| MySQL `ILIKE` inexistant | Fallback `LOWER(x) LIKE LOWER(pattern)` — documenter la perte de performance (perd l'index) |
| SQLite sans `FULL OUTER JOIN`/`RIGHT JOIN` | `Err(QueryError::Unsupported)` avec message clair |
| Injection via `Raw` | `pub(crate)` uniquement au MVP ; API `unsafe_raw` gated plus tard |
| Ambiguïté tables dans JOINs | Exiger alias de table quand même colonne utilisée des deux côtés — détecté au compile |
| `ORDER BY` avec placeholder | Interdit par certains dialectes ; ne JAMAIS paramétrer un identifiant de colonne |
| Valeurs NaN / Infinity dans `f64` | Rejetées à la construction de `Literal` → `QueryError::InvalidLiteral` |

---

## 9. Tests

### 9.1 Snapshots (`insta`)
Un fichier de snapshots par dialecte × type de requête. Chaque test :
```rust
#[test]
fn select_with_join_postgres() {
    let q = Query::select()...build(Dialect::Postgres).unwrap();
    insta::assert_snapshot!(format_built(&q));
}
```
Le format attendu inclut SQL + liste des params pour détecter toute régression.

### 9.2 Property-based (`proptest`)
Des stratégies qui génèrent des `Expr` arbitraires, puis on vérifie :
- Le SQL compilé parse sans erreur via `sqlparser` (tous dialectes confondus, parseur générique)
- Aucun `Value` littéral n'apparaît inline dans le SQL (tout passe par params)
- Le nombre de `?`/`$N` dans le SQL = longueur de `params`

### 9.3 Intégration (feature `it-tests`, optionnelle)
Sous feature flag : exécution réelle contre PG/MySQL/SQLite via Docker Compose. Non bloquant pour le CI principal au MVP.

---

## 10. Roadmap Phase 3 (après le MVP)

Une fois `qore-query` stable :

1. **Crate `qore-orm`** — dépend de `qore-query`
2. Macro `#[derive(Model)]` qui génère :
   - `impl User { fn id() -> Column<i64> { ... } }`
   - `impl User { fn name() -> Column<String> { ... } }`
   - Hydratation `User::from_row(&RowData)`
3. `User::find().filter(User::age().gt(18)).exec(&db)` — full type safety

La surface de `qore-query` ne doit pas changer pour passer à Phase 3, seulement être complétée. C'est le test principal de qualité de design en Phase 2.

---

## Annexe A — Checklist MVP

```
SEMAINE 1
  [ ] Créer src-tauri/crates/qore-query/
  [ ] Cargo.toml (dep qore-core, qore-sql)
  [ ] lib.rs + prelude.rs
  [ ] ident.rs (Column<T>, col())
  [ ] expr/mod.rs (Expr, BinOp, UnOp)
  [ ] error.rs (QueryError)
  [ ] cargo check --workspace OK
  [ ] cargo clippy -p qore-query OK

SEMAINE 2
  [ ] query/select.rs (struct + builder)
  [ ] compiler/mod.rs (trait QueryCompiler)
  [ ] compiler/sql.rs (SqlCompiler générique)
  [ ] built.rs (BuiltQuery)
  [ ] Premiers snapshots PG

SEMAINE 3
  [ ] compiler/postgres.rs, mysql.rs, sqlite.rs, mssql.rs
  [ ] Placeholders par dialecte
  [ ] Snapshots × 4 dialectes

SEMAINE 4
  [ ] query/join.rs
  [ ] query/order.rs + limit.rs
  [ ] MSSQL OFFSET..FETCH spécifique
  [ ] Gestion FULL JOIN non supporté

SEMAINE 5
  [ ] Opérateurs complets (in, like/ilike, between, is_null, not)
  [ ] MySQL ILIKE fallback

SEMAINE 6
  [ ] Subqueries (WHERE IN (SELECT...), FROM (SELECT...))
  [ ] CAST, COALESCE
  [ ] Alias de table/colonne

SEMAINE 7
  [ ] Aggregates (count, sum, avg, min, max)
  [ ] GROUP BY + HAVING
  [ ] proptest : roundtrip sqlparser
  [ ] Documentation crate (rustdoc)
  [ ] Commit final + merge
```

## Annexe B — Dialectes : matrice des spécificités (livrées MVP + à venir)

| Feature | PG | MySQL | SQLite | MSSQL | DuckDB |
| --- | --- | --- | --- | --- | --- |
| Placeholder | `$N` | `?` | `?` | `@pN` | `?` |
| Quote ident | `"x"` | `` `x` `` | `"x"` | `[x]` | `"x"` |
| `ILIKE` natif | ✅ | ❌ (fallback `LOWER`) | ❌ | ❌ | ✅ |
| `RIGHT JOIN` | ✅ | ✅ | ❌ | ✅ | ✅ |
| `FULL JOIN` | ✅ | ❌ | ❌ | ✅ | ✅ |
| `LIMIT n OFFSET m` | ✅ | ✅ | ✅ | ❌ (`OFFSET..FETCH`) | ✅ |
| `NULLS FIRST/LAST` natif | ✅ | ❌ (CASE WHEN) | ✅ | ❌ (CASE WHEN) | ✅ |
| `ESCAPE '\'` LIKE | ✅ | ✅ | ✅ (émis) | ✅ (émis) | ✅ |
| `RETURNING` | ✅ | ❌ | ✅ | `OUTPUT` | ✅ |
| Array type | ✅ | ❌ | ❌ | ❌ | ✅ |

### CAST — rendu par dialecte

| `SqlType` | PG | MySQL | SQLite | MSSQL | DuckDB |
| --- | --- | --- | --- | --- | --- |
| `Int` | `INT` | `SIGNED` | `INTEGER` | `INT` | `INTEGER` |
| `BigInt` | `BIGINT` | `SIGNED` | `INTEGER` | `BIGINT` | `BIGINT` |
| `Real` | `REAL` | `FLOAT` | `REAL` | `REAL` | `REAL` |
| `Double` | `DOUBLE PRECISION` | `DOUBLE` | `REAL` | `FLOAT` | `DOUBLE` |
| `Text` | `TEXT` | `CHAR` | `TEXT` | `NVARCHAR(MAX)` | `VARCHAR` |
| `Bool` | `BOOLEAN` | `SIGNED` | `INTEGER` | `BIT` | `BOOLEAN` |
| `Date` | `DATE` | `DATE` | `TEXT` | `DATE` | `DATE` |
| `Timestamp` | `TIMESTAMP` | `DATETIME` | `TEXT` | `DATETIME2` | `TIMESTAMP` |
| `Blob` | `BYTEA` | `BINARY` | `BLOB` | `VARBINARY(MAX)` | `BLOB` |

---

## 11. Plan de reprise — v0.2 et au-delà

> **Point d'entrée pour reprise de la tâche après merge `feat/core-extraction` → `main`.**
> Cette section décrit ce qui reste, ordonné par valeur et dépendances.

### 11.1 Intégration `qore-query` dans l'app (priorité 1 dès reprise)

Le MVP v0.1 est une lib pure. **Avant d'ajouter de nouvelles features, valider l'API par une intégration réelle.**

**Étapes recommandées** :

1. **Ajouter la dépendance** :
   ```toml
   # src-tauri/Cargo.toml
   [dependencies]
   qore-query = { path = "crates/qore-query" }
   ```

2. **Premier usage candidat — pagination du browser** :
   - Cibler `src-tauri/src/commands/query.rs` où un SELECT paramétré est construit pour afficher une page du tableau
   - Remplacer la construction string actuelle par `Query::select().from(table).all().limit(n).offset(page*n).build(dialect)`
   - `Dialect::from_driver_id(driver_id)` pour choisir le bon dialecte depuis l'`EngineContext`
   - Valider par tests manuels (Tauri dev + connexion PG/MySQL/SQLite)

3. **Deuxième usage — filtres du browser** :
   - Les filtres utilisateur (UI `BrowserFilterBar`) génèrent aujourd'hui des clauses WHERE à la main
   - Mapper sur `col("x").eq/gt/lt/like/contains(...)` avec `Expr::and_all` pour combiner
   - Bonus sécurité : `contains()` évitera le bug actuel "`%` dans la recherche = wildcard" (cf. Semaine 5)

4. **Critère de succès v0.2-integration** :
   - Zéro `format!("SELECT ... WHERE {}", user_input)` restant dans `commands/query.rs`
   - Les filtres ne casse plus sur `%`/`_`/`'` dans le champ de recherche
   - Aucune régression des E2E Postgres/MySQL/SQLite

### 11.2 Mutations — INSERT / UPDATE / DELETE (v0.2 core)

Pré-requis : §11.1 terminée (API validée par usage).

**Scope** :

- `Query::insert().into("t").values(cols_values)` → `INSERT INTO t (c1, c2) VALUES ($1, $2)`
- `.values_many(rows)` pour bulk insert
- `.on_conflict(strategy)` — `DoNothing`, `DoUpdate { set, where_? }` (Postgres `ON CONFLICT`, MySQL `ON DUPLICATE KEY`, SQLite `ON CONFLICT`, MSSQL via `MERGE`)
- `.returning([cols])` pour Postgres/SQLite/DuckDB ; fallback explicit error sur MySQL/MSSQL ou émission `OUTPUT` pour MSSQL
- `Query::update("t").set([(col, val), ...]).filter(expr)` → `UPDATE t SET c1 = $1, c2 = $2 WHERE ...`
- `Query::delete_from("t").filter(expr)` → `DELETE FROM t WHERE ...`

**Nouvelles variantes AST** :
- `InsertQuery`, `UpdateQuery`, `DeleteQuery` structs
- Trait `QueryCompiler` étendu : `compile_insert`, `compile_update`, `compile_delete`

**Point d'intégration app** : remplacer une partie de `qore_sql::generator` côté sandbox (migrations INSERT/UPDATE/DELETE) par `qore-query` en parallèle, puis supprimer le code legacy.

**Estimation** : 4-6 semaines.

### 11.3 Exécution — `.fetch()` / `.stream()` (v0.3)

Jusqu'ici `BuiltQuery { sql, params }` est passif. Étape suivante : permettre au builder de consommer directement un `&dyn DataEngine` :

```rust
let rows: Vec<RowData> = Query::select()....build(dialect)?
    .fetch(&engine)
    .await?;

// Streaming pour gros résultats
let mut stream = built_query.stream(&engine).await?;
while let Some(row) = stream.next().await { ... }
```

**Pré-requis** :
- Pont `Value → driver bind type` (ex: `sqlx::Value`, `tiberius::ColumnData`) — probablement dans `qore-drivers` ou nouveau `qore-bind`
- Pont inverse `driver row → RowData` — déjà existant côté `qore-drivers::drivers/*`

**Décision architecturale à prendre** : le trait `DataEngine::execute(sql, params)` prend-il `&[Value]` ou un type plus rich ? Aujourd'hui les drivers construisent leurs binds depuis `Vec<Value>`, donc ça devrait couler naturellement.

**Estimation** : 2-3 semaines.

### 11.4 Constructions SQL avancées (v0.3)

Par valeur pour les cas d'usage data-analyst :

- **CTE** : `Query::with("active_users", inner_query).select()...`
- **UNION / INTERSECT / EXCEPT** : `q1.union(q2)` / `union_all`
- **Window functions** : `ROW_NUMBER() OVER (PARTITION BY x ORDER BY y)` — lourd, probablement Phase 3
- **`DISTINCT`** au niveau projection (pas que sur COUNT)
- **`CASE WHEN … ELSE … END`** comme expression first-class

**Estimation** : 3-4 semaines pour CTE + UNION + DISTINCT + CASE.

### 11.5 Mongo — `MongoCompiler` (v0.5)

NoSQL. Implémente un **second** trait (pas `SqlCompiler`) qui produit des pipelines d'agrégation BSON :

```rust
pub trait MongoCompiler {
    fn compile_select_to_pipeline(&self, q: &SelectQuery) -> Result<MongoPipeline, QueryError>;
}
```

Même AST côté user — le builder `Query::select()...` reste identique. Le compile diffère.

**Pré-requis** : MongoDB intégré dans `qore-drivers` (déjà le cas).

**Décision à prendre** : Redis n'aura pas de query builder (API commands directe). Documenter dans lib.rs.

**Estimation** : 4-8 semaines selon ambition (agrégation Mongo riche ou sous-ensemble).

### 11.6 Phase 3 — `qore-orm` (séparé)

Phase 3 vient après v0.4 stable. Macros derive, typed column refs, relations, hydratation. Voir §10.

---

### Ordre recommandé pour la reprise

```
MVP v0.1 ✅ (fait)
   │
   ▼
§11.1 Intégration app (2-3 sem) ← COMMENCER ICI
   │
   ▼
§11.2 INSERT/UPDATE/DELETE (4-6 sem)
   │
   ├──▶ §11.3 fetch/stream (2-3 sem, peut être parallèle)
   │
   ▼
§11.4 CTE / UNION / CASE (3-4 sem)
   │
   ├──▶ §11.5 MongoCompiler (indépendant)
   │
   ▼
Phase 3 — qore-orm (séparé, 3-6 mois)
```

**Rétro-coupe-file** : à chaque reprise, commencer par relire §0 (état d'avancement) et §11 (plan). Le reste du document documente le **design** — utile pour onboarding mais pas pour l'exécution quotidienne.

---

### Fichiers clés pour la reprise

| Besoin | Fichier |
| --- | --- |
| Ajouter un variant `Expr` | `crates/qore-query/src/expr/mod.rs` + `src/compiler/sql.rs` (match arm) |
| Ajouter un dialecte | `crates/qore-query/src/dialect.rs` + `src/compiler/<nom>.rs` |
| Ajouter une méthode Column | `crates/qore-query/src/ident.rs` |
| Ajouter une méthode SelectQuery | `crates/qore-query/src/query/select.rs` |
| Ajouter une capability DialectOps | `crates/qore-query/src/compiler/mod.rs` (trait def) + override chaque dialecte |
| Nouveau type d'erreur | `crates/qore-query/src/error.rs` |
| Tests | `crates/qore-query/tests/<feature>.rs` (1 fichier par thème) |

### Commandes utiles

```bash
# Dev cycle
cd src-tauri
cargo check -p qore-query
cargo test -p qore-query
cargo clippy -p qore-query --all-targets -- -D warnings

# Vérifier que rien ne casse workspace-wide
cargo check --workspace
cargo test --workspace  # certains tests exigent Docker (docker-compose up -d)

# Voir la surface publique
cargo doc -p qore-query --no-deps --open
```
