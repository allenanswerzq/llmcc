//! Unified error handling for llmcc.
//!
//! - [`ErrorKind`]: What error occurred (ParseFailed, SymbolNotFound, etc.)
//! - [`ErrorStatus`]: Whether the error is retryable
//! - [`Error`]: Rich error with context, operation chain, and source

mod error;
mod kind;
mod status;

pub use error::Error;
pub use kind::ErrorKind;
pub use status::ErrorStatus;

/// Result type alias using llmcc Error.
pub type Result<T> = std::result::Result<T, Error>;
