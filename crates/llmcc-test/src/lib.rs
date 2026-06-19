//! JSON-backed test runner for llmcc graph behavior.
//!
//! Test suites are JSON files. Each case declares virtual source files, a
//! language, a graph depth, and the expected [`llmcc_format::GraphDocument`].

mod case;
mod engine;

pub use case::{CaseLanguage, JsonCase, JsonSuite, SourceFile, SuiteFile, load_suite_files};
pub use engine::{CaseOutcome, CaseStatus, RunOptions, RunReport, run_path};
