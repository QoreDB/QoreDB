// SPDX-License-Identifier: Apache-2.0

pub mod order;
pub mod select;

pub use order::{Order, OrderItem};
pub use select::{SelectItem, SelectQuery};

/// Entry point for building queries.
///
/// ```ignore
/// use qore_query::prelude::*;
/// let q = Query::select().from("users").all().build(Dialect::Postgres)?;
/// ```
pub struct Query;

impl Query {
    pub fn select() -> SelectQuery {
        SelectQuery::new()
    }
}
