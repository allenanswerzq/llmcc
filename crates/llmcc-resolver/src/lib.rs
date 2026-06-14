//! Symbol resolution and binding for cross-reference analysis.
use std::time::Instant;

use llmcc_core::symbol::Symbol;

pub mod binder;
pub mod collector;

pub use binder::{BindCtxt, bind_symbols};
pub use collector::{CollectCtxt, build_and_collect, collect_symbols};
pub use llmcc_core::ResolveOptions;

/// Select one symbol from ambiguous matches using resolver-wide precedence.
///
/// Current-unit symbols win first, then current-package symbols, then the last
/// match returned by the underlying scope lookup.
pub(crate) fn try_resolve_ambiguous<'a>(
    symbols: &[&'a Symbol],
    unit_index: usize,
    package_index: usize,
) -> Option<&'a Symbol> {
    symbols
        .iter()
        .rev()
        .find(|symbol| symbol.unit_index() == Some(unit_index))
        .or_else(|| {
            symbols
                .iter()
                .rev()
                .find(|symbol| symbol.package_index() == Some(package_index))
        })
        .or_else(|| symbols.last())
        .copied()
}

pub(crate) fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
