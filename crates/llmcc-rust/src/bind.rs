use core::str;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_descriptor::{DescriptorTrait, TypeExpr};
use llmcc_resolver::{BinderCore, CollectedSymbols, CollectionResult};

use crate::describe::RustDescriptor;
use crate::token::{AstVisitorRust, LangRust};

/// `SymbolBinder` connects symbols with the items they reference so that later
/// stages (or LLM consumers) can reason about dependency relationships.
#[derive(Debug)]
struct SymbolBinder<'tcx, 'a> {
    core: BinderCore<'tcx, 'a>,
}

impl<'tcx, 'a> SymbolBinder<'tcx, 'a> {
    pub fn new(
        unit: CompileUnit<'tcx>,
        globals: &'tcx Scope<'tcx>,
        collection: &'a CollectionResult,
    ) -> Self {
        Self {
            core: BinderCore::new(unit, globals, collection),
        }
    }

    fn unit(&self) -> CompileUnit<'tcx> {
        self.core.unit()
    }

    fn collection(&self) -> &'a CollectionResult {
        self.core.collection()
    }

    fn scopes(&self) -> &ScopeStack<'tcx> {
        self.core.scopes()
    }

    fn scopes_mut(&mut self) -> &mut ScopeStack<'tcx> {
        self.core.scopes_mut()
    }

    fn interner(&self) -> &llmcc_core::interner::InternPool {
        self.core.interner()
    }

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.core.current_symbol()
    }
}

impl<'tcx> AstVisitorRust<'tcx> for SymbolBinder<'tcx, '_> {
    type ScopedSymbol = &'tcx Symbol;

    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit()
    }

    fn visit_children_scope(&mut self, node: &HirNode<'tcx>, symbol: Option<Self::ScopedSymbol>) {
        let depth = self.scopes().depth();
        if let Some(symbol) = symbol {
            if let Some(parent) = self.current_symbol() {
                parent.add_dependency(symbol);
            }
        }

        // NOTE: scope should already be created during symbol collection, here we just
        // follow the tree structure again
        let scope = self.unit().opt_get_scope(node.hir_id());

        if let Some(scope) = scope {
            self.scopes_mut().push_with_symbol(scope, symbol);
            self.visit_children(node);
            self.scopes_mut().pop_until(depth);
        } else {
            self.visit_children(node);
        }
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Module);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Struct);
        self.visit_children_scope(&node, symbol);

        if let Some(desc) = self.collection().structs.find(node.hir_id())
            && let Some(struct_symbol) = symbol {
            for field in &desc.fields {
                if let Some(type_expr) = field.type_annotation.as_ref() {
                    for &type_symbol in &self.core.lookup_expr_symbols(type_expr) {
                        struct_symbol.add_dependency(type_symbol);
                    }
                }
            }
        } else {
            tracing::warn!("failed to build descriptor for struct: {}", node.hir_id());
        }
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Enum);
        self.visit_children_scope(&node, symbol);

        if let Some(desc) = self.collection().enums.find(node.hir_id())
            && let Some(enum_symbol) = symbol {

            for variant in &desc.variants {
                for field in &variant.fields {
                    if let Some(type_expr) = field.type_annotation.as_ref() {
                        for &type_symbol in &self.core.lookup_expr_symbols(type_expr) {
                            enum_symbol.add_dependency(type_symbol);
                        }
                    }
                }
            }
        } else {
            tracing::warn!("failed to build descriptor for enum: {}", node.hir_id());
        }
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Function);
        self.visit_children_scope(&node, symbol);

        if let Some(descriptor) = self.collection().functions.find(node.hir_id())
            && let Some(func) = symbol {

            if let Some(return_type) = descriptor.return_type.as_ref() {
                for &type_symbol in &self.core.lookup_expr_symbols(return_type) {
                    func.add_dependency(type_symbol);
                }
            }

            for parameter in &descriptor.parameters {
                if let Some(type_expr) = parameter.type_hint.as_ref() {
                    for &type_symbol in &self.core.lookup_expr_symbols(type_expr) {
                        func.add_dependency(type_symbol);
                    }
                }
            }
        }

        let parent_symbol = self.current_symbol();
        if let Some(parent_symbol) = parent_symbol {
            // If this function is inside an impl block, it depends on the impl's target struct/enum
            // The current_symbol() when visiting impl children is the target struct/enum
            if matches!(parent_symbol.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                func.add_dependency(parent_symbol);
            }
        }

        if let (Some(parent_symbol), Some(func_symbol)) = (parent_symbol, symbol) {
            // When visiting `impl Foo { ... }`, `parent_symbol` refers to the synthetic impl symbol and
            // `func_symbol` is the method we just bound. We copy the method’s dependencies back onto the
            // impl so callers that link against the impl symbol (rather than the individual method) still
            // receive transitive edges.
            //
            // We also mirror those dependencies onto the owning struct/enum so that type-level queries see
            // the behaviour inherited from their inherent methods.
            if matches!(
                parent_symbol.kind(),
                SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Impl
            ) {
                self.core
                    .propagate_child_dependencies(parent_symbol, func_symbol);
            }
        }
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        if let Some(impl_descriptor) = self.collection().impls.find(node.hir_id()) {
            let impl_target = impl_descriptor.impl_target.clone();
            let mut symbol = self.core.lookup_symbol(impl_target, SymbolKind::Struct, None);
            if symbol.is_none() {
                symbol = self.core.lookup_symbol(impl_target.clone(), SymbolKind::Enum, None);
            }
            self.visit_children_scope(&node, symbol);

            // TODO: add dependencies from trait to impl target
        } else {
            tracing::warn!("failed to build descriptor for impl: {}", node.hir_id());
        }
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Trait);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);

        let parent = self.current_symbol();
        if let Some(descriptor) = self.collection().variables.find(node.hir_id())
            && let Some(parent_symbol) = parent {

            if let Some(type_expr) = descriptor.type_annotation.as_ref() {
                for &type_symbol in &self.core.lookup_expr_symbols(type_expr) {
                    parent_symbol.add_dependency(type_symbol);
                }
            }
        }
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        self.visit_const_item(node);
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);

        if let Some(descriptor) = self.collection().calls.find(node.hir_id()) {
            self.core.add_call_dependencies(&descriptor.target);
        }
    }

    fn visit_scoped_identifier(&mut self, node: HirNode<'tcx>) {
        self.visit_type_identifier(node);
    }

    fn visit_type_identifier(&mut self, node: HirNode<'tcx>) {
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collection: &CollectedSymbols,
) {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut binder = SymbolBinder::new(unit, globals, &collection.result);
    binder.visit_node(node);
}
