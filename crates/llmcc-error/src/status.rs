//! Error status for retry logic

use std::fmt;

/// The status of an error, indicating whether it can be retried.
///
/// This helps users decide how to handle errors:
/// - `Permanent`: Don't retry, the error won't resolve without external changes
/// - `Temporary`: Can retry, the error might resolve on its own
/// - `Persistent`: Was temporary but persisted after retries, stop retrying
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ErrorStatus {
    /// Error is permanent - don't retry without external changes.
    ///
    /// Examples: ParseFailed, SymbolNotFound, FileNotFound
    #[default]
    Permanent,

    /// Error is temporary - retry may succeed.
    ///
    /// Examples: Timeout, ResourceExhausted, IoFailed
    Temporary,

    /// Error was temporary but persisted after retries.
    ///
    /// The user should stop retrying and investigate.
    Persistent,
}

impl ErrorStatus {
    /// Check if retry is recommended
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorStatus::Temporary)
    }

    /// Mark as persistent after failed retries
    pub fn persist(self) -> Self {
        match self {
            ErrorStatus::Temporary => ErrorStatus::Persistent,
            other => other,
        }
    }

    /// Get status as a string
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorStatus::Permanent => "permanent",
            ErrorStatus::Temporary => "temporary",
            ErrorStatus::Persistent => "persistent",
        }
    }
}

impl fmt::Display for ErrorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_retryable() {
        assert!(!ErrorStatus::Permanent.is_retryable());
        assert!(ErrorStatus::Temporary.is_retryable());
        assert!(!ErrorStatus::Persistent.is_retryable());
    }

    #[test]
    fn test_persist() {
        assert_eq!(ErrorStatus::Temporary.persist(), ErrorStatus::Persistent);
        assert_eq!(ErrorStatus::Permanent.persist(), ErrorStatus::Permanent);
        assert_eq!(ErrorStatus::Persistent.persist(), ErrorStatus::Persistent);
    }
}
