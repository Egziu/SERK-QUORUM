use thiserror::Error;

/// Errors raised by domain rules. Messages are safe to show to end users:
/// they never contain stored data, credentials or implementation details.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DomainError {
    #[error("{field}: {message}")]
    Validation {
        field: &'static str,
        message: String,
    },
    #[error("transition from {from} to {to} is not permitted")]
    InvalidTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("{0}")]
    Immutable(&'static str),
}

impl DomainError {
    pub fn validation(field: &'static str, message: impl Into<String>) -> Self {
        Self::Validation {
            field,
            message: message.into(),
        }
    }
}
