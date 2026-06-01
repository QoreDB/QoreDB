// SPDX-License-Identifier: Apache-2.0

use qore_core::EngineError;

#[derive(Debug)]
pub enum ServiceError {
    Engine(EngineError),
    Validation(String),
}

impl ServiceError {
    pub fn sanitized(&self) -> String {
        match self {
            ServiceError::Engine(e) => e.sanitized_message(),
            ServiceError::Validation(msg) => msg.clone(),
        }
    }
}

impl From<EngineError> for ServiceError {
    fn from(e: EngineError) -> Self {
        ServiceError::Engine(e)
    }
}
