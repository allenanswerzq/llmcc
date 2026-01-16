//! # llmcc-error
//!
//! Unified error handling for llmcc - following OpenDAL's error handling practices.
//!
//! ## Design Philosophy
//!
//! - **ErrorKind**: Know what error occurred (e.g., ParseFailed, ResolutionFailed)
//! - **ErrorStatus**: Decide how to handle it (Permanent, Temporary, Persistent)
//! - **Error Context**: Assist in locating the cause with rich context
//! - **Error Source**: Wrap underlying errors without leaking raw types
//!
//! ## Usage
//!
//! ```rust
//! use llmcc_error::{Error, ErrorKind};
//!
//! fn example() -> Result<(), Error> {
//!     Err(Error::new(ErrorKind::ParseFailed, "unexpected token")
//!         .with_operation("rust::parse_file")
//!         .with_context("file", "src/main.rs")
//!         .with_context("line", "42"))
//! }
//! ```
//!
//! ## Principles
//!
//! - All functions return `Result<T, llmcc_error::Error>`
//! - External errors are wrapped with `set_source(err)`
//! - Same error handled once, subsequent ops only append context
//! - Don't abuse `From<OtherError>` to prevent raw error leakage

mod error;
mod kind;
mod status;

pub use error::Error;
pub use kind::ErrorKind;
pub use status::ErrorStatus;

/// Result type alias using llmcc Error
pub type Result<T> = std::result::Result<T, Error>;
