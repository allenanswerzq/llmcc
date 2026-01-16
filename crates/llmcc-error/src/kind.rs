//! Error kinds for llmcc operations

use strum_macros::{Display, IntoStaticStr};

/// The kind of error that occurred.
///
/// This enum categorizes errors to help users write clear error handling logic.
/// Users can match on ErrorKind to decide how to handle specific error cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, IntoStaticStr, Display)]
#[non_exhaustive]
pub enum ErrorKind {
    // =========================================================================
    // General errors
    // =========================================================================
    /// An unexpected error occurred - catch-all for unhandled cases
    Unexpected,

    /// The requested feature or operation is not supported
    Unsupported,

    /// Invalid configuration or parameters
    ConfigInvalid,

    /// Feature or operation not yet implemented
    NotImplemented,

    // =========================================================================
    // Parse errors
    // =========================================================================
    /// Failed to parse source code
    ParseFailed,

    /// Invalid syntax in source file
    SyntaxError,

    /// Encoding error (invalid UTF-8, etc.)
    EncodingError,

    // =========================================================================
    // Resolution errors
    // =========================================================================
    /// Symbol resolution failed
    ResolutionFailed,

    /// Symbol not found in scope
    SymbolNotFound,

    /// Ambiguous symbol reference
    AmbiguousSymbol,

    /// Circular dependency detected
    CircularDependency,

    /// Import/module resolution failed
    ImportFailed,

    // =========================================================================
    // Type errors
    // =========================================================================
    /// Type mismatch or incompatibility
    TypeMismatch,

    /// Unknown type reference
    UnknownType,

    // =========================================================================
    // Graph errors
    // =========================================================================
    /// Block not found in graph
    BlockNotFound,

    /// Invalid block reference
    InvalidBlockRef,

    /// Graph construction failed
    GraphBuildFailed,

    /// Cycle detected in dependency graph
    CycleDetected,

    // =========================================================================
    // File/IO errors
    // =========================================================================
    /// File not found
    FileNotFound,

    /// Permission denied
    PermissionDenied,

    /// IO operation failed
    IoFailed,

    /// Directory traversal failed
    TraversalFailed,

    // =========================================================================
    // Language-specific errors
    // =========================================================================
    /// Unsupported language
    UnsupportedLanguage,

    /// Language detection failed
    LanguageDetectionFailed,

    /// Tree-sitter grammar error
    GrammarError,

    // =========================================================================
    // Serialization errors
    // =========================================================================
    /// Serialization failed
    SerializationFailed,

    /// Deserialization failed
    DeserializationFailed,

    /// Invalid format
    InvalidFormat,

    // =========================================================================
    // Resource errors
    // =========================================================================
    /// Memory limit exceeded
    MemoryLimitExceeded,

    /// Timeout occurred
    Timeout,

    /// Resource exhausted
    ResourceExhausted,

    // =========================================================================
    // Validation errors
    // =========================================================================
    /// Invalid argument passed to function
    InvalidArgument,

    /// Assertion failed
    AssertionFailed,

    /// Invariant violation
    InvariantViolation,
}

impl ErrorKind {
    /// Returns the error kind as a static string
    pub fn as_str(&self) -> &'static str {
        (*self).into()
    }

    /// Check if this error kind is retryable by default
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
