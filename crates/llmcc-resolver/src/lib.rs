//! Symbol resolution and binding for cross-reference analysis.
pub mod binder;
pub mod collector;

pub use binder::{BinderScopes, bind_symbols_with};
pub use collector::{CollectorScopes, build_and_collect_symbols, collect_symbols_with};
pub use llmcc_core::ResolveOptions;
