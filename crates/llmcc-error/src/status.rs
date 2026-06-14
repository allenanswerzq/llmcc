//! Error status for retry logic.

use strum_macros::{Display, IntoStaticStr};

/// Error status indicating whether retry is recommended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum ErrorStatus {
    /// Error is permanent - don't retry.
    #[default]
    Permanent,
    /// Error is temporary - retry may succeed.
    Temporary,
    /// Was temporary but persisted after retries.
    Persistent,
}

impl ErrorStatus {
    /// Check if retry is recommended.
    pub fn is_retryable(&self) -> bool {
        matches!(self, ErrorStatus::Temporary)
    }

    /// Mark as persistent after failed retries.
    pub fn persist(self) -> Self {
        match self {
            ErrorStatus::Temporary => ErrorStatus::Persistent,
            other => other,
        }
    }

    /// Get status as a string.
    pub fn as_str(&self) -> &'static str {
        (*self).into()
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
    }

    #[test]
    fn test_status_strings() {
        assert_eq!(ErrorStatus::Permanent.as_str(), "permanent");
        assert_eq!(ErrorStatus::Temporary.to_string(), "temporary");
        assert_eq!(ErrorStatus::Persistent.to_string(), "persistent");
    }
}
