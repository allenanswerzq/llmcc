use llmcc_core::context::CompileUnit;
use llmcc_core::scope::Scope;
use llmcc_core::symbol::{Symbol, SymbolKind};
use llmcc_core::ir::HirNode;
use llmcc_resolver::BinderCore;

use crate::token::AstVisitorRust;
use crate::token::LangRust;

/// Visitor for resolving symbol bindings and establishing relationships.
///
/// The BinderVisitor is the second pass after DeclVisitor. It:
/// 1. Navigates the pre-created symbol table from collection phase
/// 2. Resolves symbol references to their definitions
/// 3. Establishes symbol relationships (parent-child, type-of, etc.)
/// 4. Performs type inference where applicable
///
/// # Two-Phase Approach
/// - Phase 1 (DeclVisitor/CollectorCore): Create all symbols and scopes
/// - Phase 2 (BinderVisitor/BinderCore): Resolve and bind symbols
///
/// # Phase 1 Implementation (Starting)
/// This phase focuses on core container types:
/// - Source files (crate root)
/// - Modules (namespace organization)
/// - Functions (behavior)
/// - Structs (data layout)
///
/// Phase 2 expansion will add enum, trait, impl, type alias, const, static, and field handling.
#[derive(Debug)]
pub struct BinderVisitor<'tcx> {
    unit: CompileUnit<'tcx>,
}

impl<'tcx> BinderVisitor<'tcx> {
    /// Creates a new binder visitor for the compilation unit.
    fn new(unit: CompileUnit<'tcx>) -> Self {
        Self { unit }
    }

    /// Visit a scoped identifier to resolve symbol references.
    /// Maps identifiers to symbols across scope boundaries.
    fn visit_scoped_identifier(
        &self,
        node: HirNode<'tcx>,
        core: &BinderCore<'tcx>,
    ) -> Option<&'tcx Symbol> {
        // Try to resolve the path first
        if let Some(path_node) = node.opt_child_by_field(self.unit(), LangRust::field_path) {
            // Recursively resolve the path
            // This handles cases like `module::Type::METHOD`
        }

        // Then resolve the final name
        if let Some(name_node) = node.opt_child_by_field(self.unit(), LangRust::field_name) {
            if let Some(name_ident) = name_node.as_ident() {
                // Look up the name in the current scope hierarchy
                let scope = core.scope_top();
                let name_key = core.interner().intern(&name_ident.name);
                let symbols = scope.lookup_symbols(name_key);
                return symbols.last().copied();
            }
        }

        None
    }


    /// Type inference for expressions.
    /// Returns the inferred type symbol for an expression node.
    fn infer_type(
        &self,
        _node: HirNode<'tcx>,
        _core: &BinderCore<'tcx>,
    ) -> Option<&'tcx Symbol> {
        // TODO: Implement type inference
        // - Literals (i32, bool, etc.)
        // - Variable references
        // - Function returns
        // - Binary operations
        // - Pattern matching
        None
    }
}

impl<'tcx> AstVisitorRust<'tcx, BinderCore<'tcx>> for BinderVisitor<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut BinderCore<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Navigate the crate/module scope established during collection phase
        // Entry point for the binding phase
        if let Some(scope) = core.unit().opt_get_scope(node.id()) {
            core.push_scope(scope);
            self.visit_children(&node, core, scope, None);
            core.pop_scope();
        }
    }

    fn visit_mod_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut BinderCore<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Navigate module scope created during collection phase
        if let Some(scope) = core.unit().opt_get_scope(node.id()) {
            core.push_scope(scope);
            self.visit_children(&node, core, scope, None);
            core.pop_scope();
        }
    }

    fn visit_function_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut BinderCore<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Navigate function scope created during collection phase
        // TODO: Resolve return type if present
        // TODO: Resolve parameter types
        if let Some(scope) = core.unit().opt_get_scope(node.id()) {
            core.push_scope(scope);
            self.visit_children(&node, core, scope, None);
            core.pop_scope();
        }
    }

    fn visit_struct_item(
        &mut self,
        node: HirNode<'tcx>,
        core: &mut BinderCore<'tcx>,
        _namespace: &'tcx Scope<'tcx>,
        _parent: Option<&Symbol>,
    ) {
        // Navigate struct scope and resolve field types
        // TODO: Resolve types for struct fields
        if let Some(scope) = core.unit().opt_get_scope(node.id()) {
            core.push_scope(scope);
            self.visit_children(&node, core, scope, None);
            core.pop_scope();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_binder_visitor_creation() {
        // Basic test to verify BinderVisitor can be instantiated
        // Full tests require a CompileUnit which is integration-level
    }
}
