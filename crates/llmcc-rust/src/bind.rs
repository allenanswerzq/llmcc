use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_resolver::{BinderCore, CollectedSymbols, CollectionResult};

use crate::path::parse_rust_path;
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

    fn current_symbol(&self) -> Option<&'tcx Symbol> {
        self.core.scope_symbol()
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
        let symbol = self
            .core
            .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Module);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self
            .core
            .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Struct);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::EnumVariant);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self
            .core
            .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Enum);
        self.visit_children_scope(&node, symbol);

        let descriptor = self.collection().enums.find(node.hir_id());
        if let (Some(enum_symbol), Some(desc)) = (symbol, descriptor) {
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
                .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Function);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_type_parameter(&mut self, node: HirNode<'tcx>) {
        let child = node.opt_child_by_field(self.unit(), LangRust::field_default_type);
        if let Some(_child) = child {
            self.visit_children(&node);
        } else {
            let child = node.opt_child_by_field(self.unit(), LangRust::field_bounds);
            if let Some(child) = child {
                self.visit_children(&child);
            }
        }
    }

    fn visit_type_item(&mut self, node: HirNode<'tcx>) {
        self.visit_associated_type(node);
    }

    fn visit_associated_type(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::InferredType);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        if let Some(impl_descriptor) = self.collection().impls.find(node.hir_id()) {
            let symbols = self.core.lookup_expr_symbols(&impl_descriptor.target_ty);

            // Impl blocks can appear in files that do not define the target type
            // (e.g. `impl Person` inside `src/foo.rs` while `struct Person` lives
            // elsewhere). When that happens the collector may have only recorded a
            // placeholder symbol scoped to the current unit. Before we descend into
            // the block, try to resolve the canonical global symbol for the target
            // type so every impl shares the same owner symbol regardless of which
            // file declared it.
            let global_target_symbol =
                impl_descriptor
                    .target_ty
                    .path_segments()
                    .and_then(|segments| {
                        [SymbolKind::Struct, SymbolKind::Enum]
                            .into_iter()
                            .find_map(|kind| {
                                self.core
                                    .lookup_symbol_only_in_globals(segments, Some(kind), None)
                            })
                    });

            let enum_symbol = symbols
                .iter()
                .copied()
                .find(|symbol| symbol.kind() == SymbolKind::Enum);

            let struct_symbol = symbols
                .iter()
                .copied()
                .find(|symbol| symbol.kind() == SymbolKind::Struct);

            let target_symbol = global_target_symbol
                .or(enum_symbol)
                .or(struct_symbol)
                .or_else(|| symbols.into_iter().next());

            self.visit_children_scope(&node, target_symbol);

            if let (Some(target_symbol), Some(trait_ty)) =
                (target_symbol, impl_descriptor.trait_ty.as_ref())
            {
                for &trait_symbol in &self.core.lookup_expr_symbols(trait_ty) {
                    target_symbol.add_dependency(trait_symbol);
                }
            }
        } else {
            tracing::warn!("failed to build descriptor for impl: {}", node.hir_id());
        }
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self
            .core
            .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Trait)
            .or_else(|| {
                self.core
                    .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Struct)
            });
        self.visit_children_scope(&node, symbol);
    }

    fn visit_function_signature_item(&mut self, node: HirNode<'tcx>) {
        self.visit_function_item(node);
    }

    fn visit_macro_definition(&mut self, node: HirNode<'tcx>) {
        let symbol = self
            .core
            .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Macro);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let symbol = self
            .core
            .lookup_symbol_with(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        self.visit_const_item(node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);

        let parent = self.current_symbol();
        if let Some(descriptor) = self.collection().calls.find(node.hir_id()) {
            let mut symbols = Vec::new();
            self.core
                .lookup_call_symbols(&descriptor.target, &mut symbols);

            // Inline type binding for variables assigned from function calls (let x = func())
            // This handles Rust's type inference for assignments without explicit type annotations
            if !symbols.is_empty() {
                // Check if this call is inside a let declaration
                if let Some(parent_id) = node.parent() {
                    let parent_node = self.unit().hir_node(parent_id);
                    if parent_node.inner_ts_node().kind() == "let_declaration" {
                        // Extract the variable name from the pattern (e.g., `cfg` in `let cfg = ...`)
                        if let Some(pattern_node) =
                            parent_node.opt_child_by_field(self.unit(), LangRust::field_pattern)
                        {
                            if let Some(ident) = pattern_node.find_ident(self.unit()) {
                                let var_name = &ident.name;

                                // Use BinderCore APIs to bind variable type from function return type
                                self.core
                                    .bind_call_receivers_to_variable(var_name, &symbols);

                                // Create type alias in current scope for future lookups
                                if let Some(_variable) = self.core.lookup_symbol(
                                    &[var_name.clone()],
                                    Some(SymbolKind::Variable),
                                    None,
                                ) {
                                    if let Some(scope) = self.scopes().top() {
                                        for symbol in &symbols {
                                            if let Some(receiver) = self
                                                .core
                                                .lookup_return_receivers(symbol)
                                                .into_iter()
                                                .next()
                                            {
                                                self.core.bind_variable_type_alias(
                                                    scope, var_name, receiver,
                                                );
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(parent_symbol) = parent {
                for symbol in symbols {
                    parent_symbol.add_dependency(symbol);
                }
            }
        }
    }

    fn visit_macro_invocation(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);

        let parent = self.current_symbol();
        if let Some(descriptor) = self.collection().calls.find(node.hir_id()) {
            let mut symbols = Vec::new();
            self.core
                .lookup_call_symbols(&descriptor.target, &mut symbols);
            if let Some(parent_symbol) = parent {
                for symbol in symbols {
                    parent_symbol.add_dependency(symbol);
                }
            }
        }
    }

    fn visit_scoped_identifier(&mut self, node: HirNode<'tcx>) {
        self.core.resolve_identifier_with(node, parse_rust_path);
    }

    fn visit_type_identifier(&mut self, node: HirNode<'tcx>) {
        self.core.resolve_identifier_with(node, parse_rust_path);
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
        self.core.resolve_identifier_with(node, parse_rust_path);
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
