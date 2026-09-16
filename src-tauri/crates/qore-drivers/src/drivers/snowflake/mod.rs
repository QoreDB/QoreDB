// SPDX-License-Identifier: Apache-2.0

//! Snowflake over the SQL API v2: no session, no native driver, a signed
//! JWT per hour and one HTTPS request per statement.

mod client;
mod driver;
mod response;

pub use driver::SnowflakeDriver;
