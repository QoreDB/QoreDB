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

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl EngineError {
    pub fn connection_failed(msg: impl Into<String>) -> Self {
        Self::ConnectionFailed { message: msg.into() }
    }

    pub fn auth_failed(msg: impl Into<String>) -> Self {
        Self::AuthenticationFailed { message: msg.into() }
    }

    pub fn syntax_error(msg: impl Into<String>) -> Self {
        Self::SyntaxError { message: msg.into() }
    }

    pub fn execution_error(msg: impl Into<String>) -> Self {
        Self::ExecutionError { message: msg.into() }
    }

    pub fn driver_not_found(id: impl Into<String>) -> Self {
        Self::DriverNotFound { driver_id: id.into() }
    }

    pub fn session_not_found(id: impl Into<String>) -> Self {
        Self::SessionNotFound { session_id: id.into() }
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal { message: msg.into() }
    }
}

/// Result type alias for engine operations
pub type EngineResult<T> = Result<T, EngineError>;
