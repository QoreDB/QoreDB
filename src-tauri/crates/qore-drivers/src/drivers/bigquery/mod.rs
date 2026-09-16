// SPDX-License-Identifier: Apache-2.0

//! BigQuery over its REST API: a service account traded for an hourly
//! access token, jobs polled by id, and the free `tabledata.list` used
//! wherever a plain read does not need SQL.

mod client;
mod driver;
mod response;

pub use driver::BigQueryDriver;
