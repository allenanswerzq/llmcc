//! Symbol resolution and binding for cross-reference analysis.
use std::time::Instant;

use llmcc_core::symbol::Symbol;

pub mod binder;
pub mod collector;

pub use binder::{BinderScopes, bind_symbols_with};
pub use collector::{CollectorScopes, build_and_collect_symbols, collect_symbols_with};
pub use llmcc_core::ResolveOptions;

/// Select one symbol from ambiguous matches using resolver-wide precedence.
///
/// Current-unit symbols win first, then current-crate symbols, then the last
/// match returned by the underlying scope lookup.
pub(crate) fn select_preferred_symbol<'a>(
    symbols: &[&'a Symbol],
    unit_index: usize,
    crate_index: usize,
) -> Option<&'a Symbol> {
    symbols
        .iter()
        .rev()
        .find(|symbol| symbol.unit_index() == Some(unit_index))
        .or_else(|| {
            symbols
                .iter()
                .rev()
                .find(|symbol| symbol.crate_index() == Some(crate_index))
        })
        .or_else(|| symbols.last())
        .copied()
}

pub(crate) fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
