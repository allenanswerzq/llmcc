//! Error kinds for llmcc operations.

use strum_macros::{Display, IntoStaticStr};

/// Categorized error kinds for structured error handling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, Display)]
#[non_exhaustive]
pub enum ErrorKind {
    // General
    Unexpected,
    Unsupported,
    ConfigInvalid,
    NotImplemented,

    // Parse
    ParseFailed,
    SyntaxError,
    EncodingError,

    // Resolution
    ResolutionFailed,
    SymbolNotFound,
    AmbiguousSymbol,
    CircularDependency,
    ImportFailed,

    // Type
    TypeMismatch,
    UnknownType,

    // Graph
    BlockNotFound,
    InvalidBlockRef,
    GraphBuildFailed,
    CycleDetected,

    // File/IO
    FileNotFound,
    PermissionDenied,
    IoFailed,
    TraversalFailed,

    // Language
    UnsupportedLanguage,
    LanguageDetectionFailed,
    GrammarError,

    // Serialization
    SerializationFailed,
    DeserializationFailed,
    InvalidFormat,

    // Resource
    MemoryLimitExceeded,
    Timeout,
    ResourceExhausted,

    // Validation
    InvalidArgument,
    AssertionFailed,
    InvariantViolation,
}

impl ErrorKind {
    /// Returns the error kind as a static string.
    pub fn as_str(&self) -> &'static str {
        (*self).into()
    }

    /// Check if this error kind is retryable by default.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            ErrorKind::Timeout | ErrorKind::ResourceExhausted | ErrorKind::IoFailed
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_kind_display() {
        assert_eq!(ErrorKind::ParseFailed.to_string(), "ParseFailed");
        assert_eq!(ErrorKind::SymbolNotFound.to_string(), "SymbolNotFound");
    }

    #[test]
    fn test_is_retryable() {
        assert!(ErrorKind::Timeout.is_retryable());
        assert!(ErrorKind::IoFailed.is_retryable());
        assert!(!ErrorKind::ParseFailed.is_retryable());
        assert!(!ErrorKind::SymbolNotFound.is_retryable());
    }
}
