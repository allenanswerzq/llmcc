pub mod binder;
// pub mod call_target;  // TODO: Create when implementing language-specific collectors
pub mod collector;
// mod type_expr;  // TODO: Create when implementing language-specific collectors

pub use binder::BinderScopes;
pub use collector::CollectorScopes;

// Language-specific collection types will be exported here once implemented
// pub use collector::{
//     apply_collected_symbols, apply_symbol_batch, collect_symbols_with, CallCollection,
//     ClassCollection, CollectedSymbols, CollectionResult, CollectorScopes, DescriptorCollection,
//     EnumCollection, FunctionCollection, ImplCollection, ImportCollection, ScopeSpec,
//     StructCollection, SymbolSpec, VariableCollection,
// };
