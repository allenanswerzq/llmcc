//! Test infrastructure and corpus management.
pub mod corpus;
mod expectation;
mod materialize;
pub mod options;
mod pipeline;
pub mod runner;
pub mod snapshot;

pub use corpus::{Corpus, CorpusCase, CorpusCaseExpectation, CorpusFile, TestFile};
pub use options::{CommonTestOptions, GraphOptions, ProcessingOptions};
pub use pipeline::PipelineOptions;
pub use runner::{
    CaseOutcome, CaseStatus, RunnerConfig, run_cases, run_cases_for_file,
    run_cases_for_file_with_parallel,
};
pub use snapshot::{Snapshot, SnapshotContext};
