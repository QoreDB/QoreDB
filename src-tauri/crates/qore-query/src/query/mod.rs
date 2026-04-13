// SPDX-License-Identifier: Apache-2.0

pub mod select;

pub use select::SelectQuery;

pub struct Query;

impl Query {
    pub fn select() -> SelectQuery {
        SelectQuery::new()
    }
}
