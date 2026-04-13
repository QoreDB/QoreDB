# QoreQuery — Plan Phase 2 (Query Builder)

> **Phase 2** de la roadmap QoreORM. Pré-requis : Phase 1 (`qore-core`, `qore-sql`, `qore-drivers` extraits) — ✅ terminée sur `feat/core-extraction`.

## Table des matières

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

## Annexe B — Dialectes : matrice des spécificités

| Feature | PG | MySQL | SQLite | MSSQL |
| --- | --- | --- | --- | --- |
| Placeholder | `$N` | `?` | `?` | `@pN` |
| Quote ident | `"x"` | `` `x` `` | `"x"` | `[x]` |
| `ILIKE` | natif | fallback `LOWER LIKE LOWER` | fallback idem | fallback idem |
| `RIGHT JOIN` | ✅ | ✅ | ❌ | ✅ |
| `FULL JOIN` | ✅ | ❌ | ❌ | ✅ |
| `LIMIT` | ✅ | ✅ | ✅ | ❌ (`OFFSET..FETCH`) |
| `RETURNING` | ✅ | ❌ | ✅ | `OUTPUT` |
| Array type | ✅ | ❌ | ❌ | ❌ |
