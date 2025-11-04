use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, ScopeStack, Symbol, SymbolKind};
use llmcc_descriptor::DescriptorTrait;
use llmcc_resolver::{BinderCore, CollectionResult};

use crate::describe::RustDescriptor;
use crate::token::{AstVisitorRust, LangRust};
/// `SymbolBinder` connects symbols with the items they reference so that later
/// stages (or LLM consumers) can reason about dependency relationships.
#[derive(Debug)]
struct SymbolBinder<'tcx, 'a> {
    core: BinderCore<'tcx, 'a, CollectionResult>,
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
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Module);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Struct);
        let struct_name = node
            .opt_child_by_field(self.unit(), LangRust::field_name)
            .and_then(|child| child.as_ident())
            .map(|ident| ident.name.clone())
            .unwrap_or_else(|| "<unknown>".to_string());
        if let Some(&descriptor_idx) = self.collection().struct_map.get(&node.hir_id()) {
            if let Some(struct_descriptor) = self.collection().structs.get(descriptor_idx) {
                tracing::trace!(
                    "[bind][struct] {} fields={}",
                    struct_name,
                    struct_descriptor.fields.len()
                );
                for field in &struct_descriptor.fields {
                    if let Some(type_expr) = field.type_annotation.as_ref() {
                        let mut symbols = Vec::new();
                        self.core.collect_type_expr_symbols(
                            type_expr,
                            &[SymbolKind::Struct, SymbolKind::Enum],
                            &mut symbols,
                        );
                        for &type_symbol in &symbols {
                            if type_symbol.unit_index() == Some(self.unit().index) {
                                tracing::trace!(
                                    "[bind][struct] {} depends on {:?}",
                                    struct_name,
                                    type_symbol.name.as_str()
                                );
                            }
                            self.core.add_symbol_dependency(Some(type_symbol));
                        }
                        if symbols.is_empty() {
                            tracing::trace!(
                                "[bind][struct] {} unresolved field type {:?}",
                                struct_name,
                                type_expr
                            );
                        }
                    }
                }
            } else {
                tracing::trace!("[bind][struct] {} descriptor missing", struct_name);
            }
        } else {
            tracing::trace!("[bind][struct] {} not in struct_map", struct_name);
        }

        self.visit_children_scope(&node, symbol);
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Enum);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Function);
        let parent_symbol = self.current_symbol();
        if let Some(func_symbol) = symbol {
            if let Some(&descriptor_idx) = self.collection().function_map.get(&node.hir_id()) {
                if let Some(return_type) = self.collection().functions[descriptor_idx]
                    .return_type
                    .as_ref()
                {
                    let mut symbols = Vec::new();
                    self.core.collect_type_expr_symbols(
                        return_type,
                        &[SymbolKind::Struct, SymbolKind::Enum],
                        &mut symbols,
                    );
                    for &return_type_sym in &symbols {
                        func_symbol.add_dependency(return_type_sym);
                    }
                }

                for parameter in &self.collection().functions[descriptor_idx].parameters {
                    if let Some(type_expr) = parameter.type_hint.as_ref() {
                        let mut symbols = Vec::new();
                        self.core.collect_type_expr_symbols(
                            type_expr,
                            &[SymbolKind::Struct, SymbolKind::Enum],
                            &mut symbols,
                        );
                        for &type_symbol in &symbols {
                            func_symbol.add_dependency(type_symbol);
                        }
                    }
                }
            }

            // If this function is inside an impl block, it depends on the impl's target struct/enum
            // The current_symbol() when visiting impl children is the target struct/enum
            if let Some(parent_symbol) = parent_symbol {
                let kind = parent_symbol.kind();
                if matches!(kind, SymbolKind::Struct | SymbolKind::Enum) {
                    func_symbol.add_dependency(parent_symbol);
                }
            }
        }

        self.visit_children_scope(&node, symbol);

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
        let impl_descriptor = self
            .collection()
            .impl_map
            .get(&node.hir_id())
            .and_then(|&idx| self.collection().impls.get(idx));

        // Use the descriptor’s fully-qualified name to locate the target type symbol. The
        // descriptor already normalized nested paths (e.g., `crate::foo::Bar`).
        let symbol = impl_descriptor.and_then(|descriptor| {
            descriptor.impl_target_fqn.as_ref().and_then(|fqn| {
                let segments: Vec<String> = fqn
                    .split("::")
                    .map(|segment| segment.trim().to_string())
                    .filter(|segment| !segment.is_empty())
                    .collect();
                if segments.is_empty() {
                    None
                } else {
                    self.core.lookup_segments_with_priority(
                        &segments,
                        &[SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Trait],
                        None,
                    )
                }
            })
        });

        // If we know both the descriptor and the target symbol, record trait → type dependencies.
        //    Example: `impl Display for Foo` should establish Display → Foo so trait queries reach Foo.
        if let (Some(descriptor), Some(target_symbol)) = (impl_descriptor, symbol) {
            // base_types is the traits being implemented
            for base in &descriptor.base_types {
                if let Some(segments) = base.path_segments().map(|segments| segments.to_vec()) {
                    if let Some(trait_symbol) = self
                        .core
                        .lookup_segments(&segments, Some(SymbolKind::Trait), None)
                        .or_else(|| self.core.lookup_segments(&segments, None, None))
                    {
                        trait_symbol.add_dependency(target_symbol);
                    }
                }
            }
        } else {
            tracing::warn!("failed to build descriptor for impl");
        }

        self.visit_children_scope(&node, symbol);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Trait);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        if let Some(&descriptor_idx) = self.collection().variable_map.get(&node.hir_id()) {
            if let Some(type_expr) = self.collection().variables[descriptor_idx]
                .type_annotation
                .as_ref()
            {
                let mut symbols = Vec::new();
                self.core.collect_type_expr_symbols(
                    type_expr,
                    &[SymbolKind::Struct, SymbolKind::Enum],
                    &mut symbols,
                );
                for &type_symbol in &symbols {
                    self.core.add_symbol_dependency(Some(type_symbol));
                }
            }
        }

        self.visit_children(&node);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        let symbol =
            self.core
                .find_symbol_from_field(&node, LangRust::field_name, SymbolKind::Const);
        self.visit_children_scope(&node, symbol);
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        self.core.add_symbol_dependency_by_field(
            &node,
            LangRust::field_name,
            SymbolKind::EnumVariant,
        );
        self.visit_children(&node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let call = self
            .collection()
            .call_map
            .get(&node.hir_id())
            .and_then(|&idx| self.collection().calls.get(idx));
        if let Some(descriptor) = call {
            self.core.add_call_target_dependencies(&descriptor.target);
        }
        self.visit_children(&node);
    }

    fn visit_scoped_identifier(&mut self, node: HirNode<'tcx>) {
        self.visit_type_identifier(node);
    }

    fn visit_type_identifier(&mut self, node: HirNode<'tcx>) {
        if let Some(symbol) = self.core.resolve_symbol_type_expr_with(
            &node,
            RustDescriptor::build_type_expr,
            &[SymbolKind::Struct, SymbolKind::Enum],
        ) {
            // Skip struct/enum dependencies if this identifier is followed by parentheses (bare constructor call).
            // This is detected by checking if the parent is a call_expression.
            if matches!(symbol.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                // Check parent - if it's a call_expression, this might be a bare constructor call
                if let Some(parent_id) = node.parent() {
                    let parent = self.unit().hir_node(parent_id);
                    if parent.kind_id() == LangRust::call_expression {
                        // Parent is a call expression - skip adding this dependency
                        // add_call_target_dependencies will handle it
                        self.visit_children(&node);
                        return;
                    }
                }
            }
            self.core.add_symbol_dependency(Some(symbol));
        }
        self.visit_children(&node);
    }

    fn visit_identifier(&mut self, node: HirNode<'tcx>) {
        let ident = node.as_ident().unwrap();
        let key = self.interner().intern(&ident.name);

        let symbol = if let Some(local) = self.scopes().find_symbol_local(&ident.name) {
            Some(local)
        } else {
            self.scopes()
                .find_scoped_suffix_with_filters(&[key], None, Some(self.unit().index))
                .or_else(|| {
                    self.scopes()
                        .find_scoped_suffix_with_filters(&[key], None, None)
                })
                .or_else(|| {
                    self.scopes().find_global_suffix_with_filters(
                        &[key],
                        None,
                        Some(self.unit().index),
                    )
                })
                .or_else(|| {
                    self.scopes()
                        .find_global_suffix_with_filters(&[key], None, None)
                })
        };

        if let Some(sym) = symbol {
            if !matches!(sym.kind(), SymbolKind::Struct | SymbolKind::Enum) {
                self.core.add_symbol_dependency(Some(sym));
            }
        }
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn bind_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collection: &CollectionResult,
) {
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut binder = SymbolBinder::new(unit, globals, collection);
    binder.visit_node(node);
}
