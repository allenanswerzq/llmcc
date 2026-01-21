use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::Symbol;
use llmcc_resolver::BinderScopes;

/// Infer the type of a Python AST node.
///
/// Python is dynamically typed, so this currently returns None for most nodes.
#[allow(dead_code)]
pub fn infer_type<'tcx>(
    _unit: &CompileUnit<'tcx>,
    _scopes: &BinderScopes<'tcx>,
    _node: &HirNode<'tcx>,
) -> Option<&'tcx Symbol> {
    None
}
