// SPDX-License-Identifier: BUSL-1.1

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GrantLevel {
    Read,
    Write,
}

impl GrantLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            GrantLevel::Read => "read",
            GrantLevel::Write => "write",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read" => Some(GrantLevel::Read),
            "write" => Some(GrantLevel::Write),
            _ => None,
        }
    }

    /// `write` supersedes `read` when a user holds a connection via several roles.
    pub fn max(self, other: GrantLevel) -> GrantLevel {
        match (self, other) {
            (GrantLevel::Write, _) | (_, GrantLevel::Write) => GrantLevel::Write,
            _ => GrantLevel::Read,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub is_admin: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct Role {
    pub id: String,
    pub name: String,
}

/// Resolved authorization for the current request. `Admin` is the bootstrap
/// shared token (full access); `User` is a JWT-authenticated principal with the
/// connection grants resolved from its roles.
#[derive(Clone)]
pub enum AuthContext {
    Admin,
    User {
        grants: HashMap<String, GrantLevel>,
    },
}

impl AuthContext {
    pub fn is_admin(&self) -> bool {
        matches!(self, AuthContext::Admin)
    }

    /// Effective access level for a connection, or `None` if not granted.
    /// Admin always has `Write`.
    pub fn access(&self, connection_id: &str) -> Option<GrantLevel> {
        match self {
            AuthContext::Admin => Some(GrantLevel::Write),
            AuthContext::User { grants, .. } => grants.get(connection_id).copied(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub exp: usize,
}
