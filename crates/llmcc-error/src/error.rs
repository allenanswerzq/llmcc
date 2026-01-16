//! The main Error type for llmcc.

use crate::{ErrorKind, ErrorStatus};
use std::fmt;

/// Unified error type for all llmcc operations.
pub struct Error {
    kind: ErrorKind,
    message: String,
    status: ErrorStatus,
    operation: &'static str,
    context: Vec<(&'static str, String)>,
    source: Option<Box<dyn std::error::Error + Send + Sync + 'static>>,
}

impl Error {
    /// Create a new error with the given kind and message.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        let status = if kind.is_retryable() {
            ErrorStatus::Temporary
        } else {
            ErrorStatus::Permanent
        };

        Self {
            kind,
            message: message.into(),
            status,
            operation: "",
            context: Vec::new(),
            source: None,
        }
    }

    /// Get the error kind.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Get the error message
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Get the error status
    pub fn status(&self) -> ErrorStatus {
        self.status
    }

    /// Get the operation that caused this error
    pub fn operation(&self) -> &'static str {
        self.operation
    }

    /// Get the context key-value pairs
    pub fn context(&self) -> &[(&'static str, String)] {
        &self.context
    }

    /// Get the source error (if any).
    pub fn source_ref(&self) -> Option<&(dyn std::error::Error + Send + Sync + 'static)> {
        self.source.as_ref().map(|e| e.as_ref())
    }

    /// Set the error status.
    pub fn with_status(mut self, status: ErrorStatus) -> Self {
        self.status = status;
        self
    }

    /// Mark as temporary (retryable)
    pub fn temporary(mut self) -> Self {
        self.status = ErrorStatus::Temporary;
        self
    }

    /// Mark as permanent (not retryable)
    pub fn permanent(mut self) -> Self {
        self.status = ErrorStatus::Permanent;
        self
    }

    /// Set the operation that caused this error.
    ///
    /// If an operation was already set, the previous one is moved to context
    /// as "called" to preserve the call chain.
    pub fn with_operation(mut self, operation: &'static str) -> Self {
        if !self.operation.is_empty() {
            self.context.push(("called", self.operation.to_string()));
        }
        self.operation = operation;
        self
    }

    /// Add context to the error
    pub fn with_context(mut self, key: &'static str, value: impl Into<String>) -> Self {
        self.context.push((key, value.into()));
        self
    }

    /// Set the source error.
    ///
    /// # Panics (debug only)
    /// Panics in debug mode if source was already set.
    pub fn set_source<E>(mut self, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        debug_assert!(self.source.is_none(), "source error already set");
        self.source = Some(Box::new(source));
        self
    }

    /// Mark as persistent after failed retries.
    pub fn persist(mut self) -> Self {
        self.status = self.status.persist();
        self
    }

    /// Check if this error is retryable
    pub fn is_retryable(&self) -> bool {
        self.status.is_retryable()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}) at {}", self.kind, self.status, self.operation)?;

        if !self.context.is_empty() {
            write!(f, ", context {{ ")?;
            for (i, (key, value)) in self.context.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}: {}", key, value)?;
            }
            write!(f, " }}")?;
        }

        if !self.message.is_empty() {
            write!(f, " => {}", self.message)?;
        }

        Ok(())
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{} ({}) at {}", self.kind, self.status, self.operation)?;

        if !self.message.is_empty() {
            writeln!(f)?;
            writeln!(f, "    Message: {}", self.message)?;
        }

        if !self.context.is_empty() {
            writeln!(f)?;
            writeln!(f, "    Context:")?;
            for (key, value) in &self.context {
                writeln!(f, "        {}: {}", key, value)?;
            }
        }

        if let Some(source) = &self.source {
            writeln!(f)?;
            writeln!(f, "    Source: {:?}", source)?;
        }

        Ok(())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        let kind = match err.kind() {
            std::io::ErrorKind::NotFound => ErrorKind::FileNotFound,
            std::io::ErrorKind::PermissionDenied => ErrorKind::PermissionDenied,
            _ => ErrorKind::IoFailed,
        };
        Error::new(kind, err.to_string())
            .with_operation("io")
            .set_source(err)
    }
}

impl From<String> for Error {
    fn from(msg: String) -> Self {
        Error::new(ErrorKind::Unexpected, msg)
    }
}

impl From<&str> for Error {
    fn from(msg: &str) -> Self {
        Error::new(ErrorKind::Unexpected, msg)
    }
}

impl Error {
    /// Create an Unexpected error.
    pub fn unexpected(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unexpected, message)
    }

    /// Create an Unsupported error
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Unsupported, message)
    }

    /// Create a ParseFailed error
    pub fn parse_failed(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::ParseFailed, message)
    }

    /// Create a SyntaxError
    pub fn syntax_error(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::SyntaxError, message)
    }

    /// Create a SymbolNotFound error
    pub fn symbol_not_found(symbol: impl Into<String>) -> Self {
        let symbol = symbol.into();
        Self::new(
            ErrorKind::SymbolNotFound,
            format!("symbol '{}' not found", symbol),
        )
        .with_context("symbol", symbol)
    }

    /// Create a FileNotFound error
    pub fn file_not_found(path: impl Into<String>) -> Self {
        let path = path.into();
        Self::new(
            ErrorKind::FileNotFound,
            format!("file '{}' not found", path),
        )
        .with_context("path", path)
    }

    /// Create a BlockNotFound error
    pub fn block_not_found(block_id: impl Into<String>) -> Self {
        let block_id = block_id.into();
        Self::new(
            ErrorKind::BlockNotFound,
            format!("block '{}' not found", block_id),
        )
        .with_context("block_id", block_id)
    }

    /// Create an UnsupportedLanguage error
    pub fn unsupported_language(lang: impl Into<String>) -> Self {
        let lang = lang.into();
        Self::new(
            ErrorKind::UnsupportedLanguage,
            format!("language '{}' is not supported", lang),
        )
        .with_context("language", lang)
    }

    /// Create a CircularDependency error
    pub fn circular_dependency(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::CircularDependency, message)
    }

    /// Create an InvalidArgument error
    pub fn invalid_argument(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidArgument, message)
    }

    /// Create an AssertionFailed error
    pub fn assertion_failed(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::AssertionFailed, message)
    }

    /// Create a Timeout error
    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Timeout, message)
    }

    /// Create a NotImplemented error
    pub fn not_implemented(feature: impl Into<String>) -> Self {
        let feature = feature.into();
        Self::new(
            ErrorKind::NotImplemented,
            format!("'{}' is not implemented", feature),
        )
        .with_context("feature", feature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_creation() {
        let err = Error::new(ErrorKind::ParseFailed, "unexpected token");
        assert_eq!(err.kind(), ErrorKind::ParseFailed);
        assert_eq!(err.message(), "unexpected token");
        assert_eq!(err.status(), ErrorStatus::Permanent);
    }

    #[test]
    fn test_error_with_context() {
        let err = Error::new(ErrorKind::SymbolNotFound, "not found")
            .with_operation("resolver::resolve")
            .with_context("symbol", "MyStruct")
            .with_context("file", "src/lib.rs");

        assert_eq!(err.operation(), "resolver::resolve");
        assert_eq!(err.context().len(), 2);
        assert_eq!(err.context()[0], ("symbol", "MyStruct".to_string()));
    }

    #[test]
    fn test_operation_chaining() {
        let err = Error::new(ErrorKind::ResolutionFailed, "failed")
            .with_operation("resolver::resolve_type")
            .with_operation("graph::build");

        assert_eq!(err.operation(), "graph::build");
        assert_eq!(err.context().len(), 1);
        assert_eq!(
            err.context()[0],
            ("called", "resolver::resolve_type".to_string())
        );
    }

    #[test]
    fn test_temporary_status() {
        let err = Error::new(ErrorKind::Timeout, "operation timed out");
        assert!(err.is_retryable()); // Timeout defaults to temporary

        let err = Error::new(ErrorKind::ParseFailed, "syntax error");
        assert!(!err.is_retryable()); // ParseFailed defaults to permanent
    }

    #[test]
    fn test_persist() {
        let err = Error::new(ErrorKind::IoFailed, "connection refused").temporary();
        assert!(err.is_retryable());

        let err = err.persist();
        assert!(!err.is_retryable());
        assert_eq!(err.status(), ErrorStatus::Persistent);
    }

    #[test]
    fn test_display() {
        let err = Error::new(ErrorKind::ParseFailed, "unexpected EOF")
            .with_operation("rust::parse")
            .with_context("file", "main.rs")
            .with_context("line", "42");

        let display = format!("{}", err);
        assert!(display.contains("ParseFailed"));
        assert!(display.contains("permanent"));
        assert!(display.contains("rust::parse"));
        assert!(display.contains("file: main.rs"));
    }

    #[test]
    fn test_convenience_constructors() {
        let err = Error::symbol_not_found("MyStruct");
        assert_eq!(err.kind(), ErrorKind::SymbolNotFound);
        assert!(err.message().contains("MyStruct"));

        let err = Error::file_not_found("config.toml");
        assert_eq!(err.kind(), ErrorKind::FileNotFound);

        let err = Error::unsupported_language("brainfuck");
        assert_eq!(err.kind(), ErrorKind::UnsupportedLanguage);
    }

    #[test]
    fn test_set_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = Error::new(ErrorKind::FileNotFound, "config.json not found").set_source(io_err);

        assert!(err.source_ref().is_some());
    }
}
