// SPDX-License-Identifier: Apache-2.0

//! Normalized error types for the QoreDB Data Engine
//!
//! All driver-specific errors are mapped to these unified error types
//! to provide consistent error handling across the application.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Unified error type for all data engine operations
#[derive(Debug, Error, Serialize, Deserialize)]
pub enum EngineError {
    #[error("Connection failed: {message}")]
    ConnectionFailed { message: String },

    #[error("Authentication failed: {message}")]
    AuthenticationFailed { message: String },

    #[error("Query syntax error: {message}")]
    SyntaxError { message: String },

    #[error("Query execution error: {message}")]
    ExecutionError { message: String },

    #[error("Operation timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },

    #[error("Driver not found: {driver_id}")]
    DriverNotFound { driver_id: String },

    #[error("Session not found or expired: {session_id}")]
    SessionNotFound { session_id: String },

    #[error("Operation cancelled")]
    Cancelled,

    #[error("SSL/TLS error: {message}")]
    SslError { message: String },

    #[error("SSH tunnel error: {message}")]
    SshError { message: String },

    #[error("Proxy error: {message}")]
    ProxyError { message: String },

    #[error("Internal error: {message}")]
    Internal { message: String },

    #[error("Feature not supported: {message}")]
    NotSupported { message: String },

    #[error("Transaction error: {message}")]
    TransactionError { message: String },

    #[error("Validation error: {message}")]
    ValidationError { message: String },

    #[error("Too many concurrent queries ({current}/{limit})")]
    TooManyConcurrentQueries { current: u32, limit: u32 },

    #[error("Result exceeds row limit ({rows}/{limit} rows)")]
    ResultTooLarge { rows: u64, limit: u64 },
}

impl EngineError {
    pub fn connection_failed(msg: impl Into<String>) -> Self {
        Self::ConnectionFailed {
            message: msg.into(),
        }
    }

    pub fn auth_failed(msg: impl Into<String>) -> Self {
        Self::AuthenticationFailed {
            message: msg.into(),
        }
    }

    pub fn syntax_error(msg: impl Into<String>) -> Self {
        Self::SyntaxError {
            message: msg.into(),
        }
    }

    pub fn execution_error(msg: impl Into<String>) -> Self {
        Self::ExecutionError {
            message: msg.into(),
        }
    }

    pub fn driver_not_found(id: impl Into<String>) -> Self {
        Self::DriverNotFound {
            driver_id: id.into(),
        }
    }

    pub fn session_not_found(id: impl Into<String>) -> Self {
        Self::SessionNotFound {
            session_id: id.into(),
        }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal {
            message: msg.into(),
        }
    }

    pub fn not_supported(msg: impl Into<String>) -> Self {
        Self::NotSupported {
            message: msg.into(),
        }
    }

    pub fn transaction_error(msg: impl Into<String>) -> Self {
        Self::TransactionError {
            message: msg.into(),
        }
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::ValidationError {
            message: msg.into(),
        }
    }

    pub fn too_many_queries(current: u32, limit: u32) -> Self {
        Self::TooManyConcurrentQueries { current, limit }
    }

    pub fn result_too_large(rows: u64, limit: u64) -> Self {
        Self::ResultTooLarge { rows, limit }
    }
}

impl EngineError {
    /// Sanitize error messages before sending to the frontend.
    /// Removes credentials, file paths, and connection strings from error text.
    pub fn sanitized_message(&self) -> String {
        sanitize_error_message(&self.to_string())
    }
}

/// Strips sensitive patterns from an error message.
pub fn sanitize_error_message(msg: &str) -> String {
    use regex::Regex;

    // Lazy-init compiled patterns (compiled once, reused)
    use std::sync::OnceLock;
    static PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

    let patterns = PATTERNS.get_or_init(|| {
        vec![
            // Connection strings with credentials: postgres://user:pass@host → postgres://***@host
            (
                Regex::new(r"(?i)((?:postgres|mysql|mongodb|redis|rediss)://)([^@]+)@")
                    .unwrap(),
                "${1}***@",
            ),
            // password=... in query strings
            (
                Regex::new(r"(?i)(password|passwd|pwd)\s*=\s*\S+").unwrap(),
                "${1}=***",
            ),
            // Absolute file paths (Unix and Windows)
            (
                Regex::new(r"(/(?:Users|home|tmp|var|etc)/\S+|[A-Z]:\\[^\s:]+)").unwrap(),
                "[path]",
            ),
        ]
    });

    let mut result = msg.to_string();
    for (re, replacement) in patterns {
        result = re.replace_all(&result, *replacement).into_owned();
    }
    result
}

/// Result type alias for engine operations
pub type EngineResult<T> = Result<T, EngineError>;
