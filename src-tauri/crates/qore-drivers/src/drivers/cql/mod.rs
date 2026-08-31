// SPDX-License-Identifier: Apache-2.0

//! A CQL v4 client written against the protocol spec rather than built on a
//! cluster driver. What that gives up — token-aware routing, load balancing,
//! speculative execution, topology discovery — is what a single interactive
//! connection never exercises. One client serves Cassandra, ScyllaDB and any
//! other engine speaking the same wire protocol.

pub mod connection;
pub mod frame;
pub mod value;
