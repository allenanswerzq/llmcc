pub mod binder;
pub mod call_target;
pub mod collector;
mod type_expr;

pub use binder::BinderCore;
pub use collector::{
    apply_collected_symbols, apply_symbol_batch, collect_symbols_batch, CallCollection,
    ClassCollection, CollectedSymbols, CollectionResult, CollectorCore, DescriptorCollection,
    EnumCollection, FunctionCollection, ImplCollection, ImportCollection, ScopeSpec,
    StructCollection, SymbolSpec, VariableCollection,
};
