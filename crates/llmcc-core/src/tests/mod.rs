//! Common test utilities and fixtures for llmcc-core tests.
//!
//! This module provides reusable test infrastructure including:
//! - SimpleLanguage: A minimal test language with custom parser
//! - Simple parse tree and node implementations
//! - Helper functions for setting up test environments

pub mod simple_lang;

pub use simple_lang::{LangSimple, SimpleParseNode, SimpleParseTree};
