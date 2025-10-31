mod bind;
mod collect;
pub mod descriptor;
pub mod token;

pub use crate::bind::bind_symbols;
pub use crate::collect::{
    apply_symbol_batch, collect_symbols, collect_symbols_batch, CollectedSymbols, CollectionResult,
    SymbolBatch,
};
pub use crate::descriptor::{
    CallArgument, CallDescriptor, CallTarget, ChainSegment, EnumDescriptor, EnumVariant,
    EnumVariantField, EnumVariantKind, FnVisibility, FunctionDescriptor, FunctionParameter,
    StructDescriptor, StructField, StructKind, TypeExpr, VariableDescriptor, VariableKind,
    VariableScope,
};
pub use llmcc_core::{
    build_llmcc_graph, build_llmcc_ir, print_llmcc_graph, print_llmcc_ir, CompileCtxt,
    ProjectGraph, ProjectQuery,
};
pub use token::LangRust;

use llmcc_core::context::CompileUnit;
use llmcc_core::lang_def::ParallelSymbolCollect;
use llmcc_core::symbol::Scope;

impl ParallelSymbolCollect for LangRust {
    const PARALLEL_SYMBOL_COLLECTION: bool = true;
    type SymbolBatch = crate::collect::SymbolBatch;

    fn collect_symbol_batch<'tcx>(unit: CompileUnit<'tcx>) -> Self::SymbolBatch {
        collect_symbols_batch(unit)
    }

    fn apply_symbol_batch<'tcx>(
        unit: CompileUnit<'tcx>,
        globals: &'tcx Scope<'tcx>,
        batch: Self::SymbolBatch,
    ) {
        let _ = apply_symbol_batch(unit, globals, batch);
    }
}
