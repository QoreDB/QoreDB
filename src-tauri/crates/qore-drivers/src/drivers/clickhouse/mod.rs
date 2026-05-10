// SPDX-License-Identifier: Apache-2.0

//! ClickHouse driver — speaks the HTTP protocol with
//! `JSONCompactEachRowWithNamesAndTypes` for arbitrary dynamic queries.

mod client;
mod describe;
mod driver;
mod literal;
mod response;
mod types;

pub use driver::ClickHouseDriver;
