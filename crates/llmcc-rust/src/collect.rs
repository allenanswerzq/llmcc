use crate::describe::{RustDescriptor, Visibility};
use crate::token::{AstVisitorRust, LangRust};
use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::SymbolKind;
use llmcc_descriptor::DescriptorTrait;
use llmcc_resolver::{
    CallCollection, CollectedSymbols, CollectionResult, CollectorCore, EnumCollection,
    FunctionCollection, ImplCollection, StructCollection, VariableCollection,
    collect_symbols_batch,
};

#[derive(Debug)]
struct DeclCollector<'tcx> {
    core: CollectorCore<'tcx>,
    functions: FunctionCollection,
    variables: VariableCollection,
    calls: CallCollection,
    structs: StructCollection,
    impls: ImplCollection,
    enums: EnumCollection,
}

impl<'tcx> DeclCollector<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            core: CollectorCore::new(unit),
            functions: FunctionCollection::default(),
            variables: VariableCollection::default(),
            calls: CallCollection::default(),
            structs: StructCollection::default(),
            impls: ImplCollection::default(),
            enums: EnumCollection::default(),
        }
    }

    fn unit(&self) -> CompileUnit<'tcx> {
        self.core.unit()
    }

    fn current_function_name(&self) -> Option<&str> {
        self.core.current_function_name()
    }

    fn visibility_exports(visibility: &Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Restricted { scope } => scope == "crate",
            _ => false,
        }
    }

    fn insert_self_aliases(&mut self, owner_symbol_idx: usize) {
        let owner_kind = {
            let symbols = self.core.symbols();
            let Some(spec) = symbols.get(owner_symbol_idx) else {
                return;
            };
            if !matches!(spec.kind, SymbolKind::Struct | SymbolKind::Enum) {
                return;
            }
            spec.kind
        };

        // Alias `Self` to the owner symbol so lookups resolve without duplicating entries.
        self.core
            .add_scope_alias("Self", owner_symbol_idx, owner_kind);

        // Alias `self` (value form) to the same symbol so implicit receivers resolve.
        self.core
            .add_scope_alias("self", owner_symbol_idx, SymbolKind::Variable);
    }

    fn visit_children_scope_with_self(&mut self, node: &HirNode<'tcx>, owner_symbol: usize) {
        let owner = node.hir_id();
        let scope_idx = self.core.ensure_scope(owner);
        self.core
            .set_scope_owner_symbol(scope_idx, Some(owner_symbol));

        self.core.push_scope(scope_idx);
        self.insert_self_aliases(owner_symbol);
        self.visit_children(node);
        self.core.pop_scope();
    }

    fn finish(self) -> CollectedSymbols {
        let DeclCollector {
            core,
            functions,
            variables,
            calls,
            structs,
            impls,
            enums,
            ..
        } = self;

        core.finish(CollectionResult {
            functions,
            variables,
            calls,
            structs,
            impls,
            enums,
            ..CollectionResult::default()
        })
    }
}

impl<'tcx> AstVisitorRust<'tcx> for DeclCollector<'tcx> {
    type ScopedSymbol = usize;

    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit()
    }

    fn visit_children_scope(
        &mut self,
        node: &HirNode<'tcx>,
        owner_symbol: Option<Self::ScopedSymbol>,
    ) {
        let owner = node.hir_id();
        let scope_idx = self.core.ensure_scope(owner);
        if let Some(sym_idx) = owner_symbol {
            self.core.set_scope_owner_symbol(scope_idx, Some(sym_idx));
        }

        self.core.push_scope(scope_idx);
        self.visit_children(node);
        self.core.pop_scope();
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_function(self.unit(), &node) {
            let is_global = Self::visibility_exports(&desc.visibility);
            let (sym_idx, fqn) =
                self.core
                    .insert_symbol(node.hir_id(), &desc.name, SymbolKind::Function, is_global);
            desc.fqn = Some(fqn.clone());
            self.functions.add(node.hir_id(), desc);
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!(
                "build function error {:?} next_hir={:?}",
                self.unit().hir_text(&node),
                self.unit().hir_next()
            );
        }
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        if let Some(mut var) = RustDescriptor::build_variable(self.unit(), &node) {
            let mut type_of = None;
            if let Some(ty) = &var.type_annotation {
                // Infer the type symbol if possible
                type_of =
                    self.core
                        .lookup_type_expr_symbol(node.hir_id(), ty, SymbolKind::InferredType, false);
            }

            let (name_sym, fqn) =
                self.core
                    .insert_symbol(node.hir_id(), &var.name, SymbolKind::Variable, false);

            if let Some(type_of) = type_of {
                self.core.set_symbol_type_of(name_sym, type_of);
            }

            var.fqn = Some(fqn);
            self.variables.add(node.hir_id(), var);
            self.visit_children(&node);
        } else {
            tracing::warn!(
                "build variable error {:?} next_hir={:?}",
                self.unit().hir_text(&node),
                self.unit().hir_next()
            );
        }
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        if let Some(param) = RustDescriptor::build_parameter(self.unit(), &node) {
            for name in param.names() {
                let _ =
                    self.core
                        .insert_symbol(node.hir_id(), name, SymbolKind::Variable, false);
            }
            self.visit_children(&node);
        } else {
            tracing::warn!(
                "build parameter error {:?} next_hir={:?}",
                self.unit().hir_text(&node),
                self.unit().hir_next()
            );
        }
    }

    fn visit_self_parameter(&mut self, node: HirNode<'tcx>) {
        let (sym_idx, _) =
            self.core
                .insert_symbol(node.hir_id(), "self", SymbolKind::Variable, false);

        let owner_symbol_idx = self
            .core
            .lookup_from_scopes_with("Self", SymbolKind::Struct)
            .or_else(|| self.core.lookup_from_scopes_with("Self", SymbolKind::Enum));

        if let Some(owner_idx) = owner_symbol_idx {
            self.core.set_symbol_type_of(sym_idx, owner_idx);
        }

        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        if let Some(module) = RustDescriptor::build_module(self.unit(), &node) {
            let is_global = Self::visibility_exports(&module.visibility);
            let (sym_idx, _fqn) =
                self.core
                    .insert_symbol(node.hir_id(), &module.name, SymbolKind::Module, is_global);
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!(
                "failed to build module descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_type_item(&mut self, node: HirNode<'tcx>) {
        self.visit_associated_type(node);
    }

    fn visit_associated_type(&mut self, node: HirNode<'tcx>) {
        if let Some((sym_idx, _)) =
            self.core
                .insert_field_symbol(&node, LangRust::field_name, SymbolKind::InferredType)
        {
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        if let Some(descriptor) = RustDescriptor::build_impl(self.unit(), &node) {
            // impl Foo {}
            let owner_symbol = match self.core.upsert_expr_symbol(
                node.hir_id(),
                &descriptor.target_ty,
                SymbolKind::Struct,
                false,
            ) {
                Some(symbol) => symbol,
                None => return,
            };

            // impl Bar for Foo {}
            if let Some(ty) = &descriptor.trait_ty {
                self.core
                    .lookup_type_expr_symbol(node.hir_id(), ty, SymbolKind::Trait, false);
            }

            self.impls.add(node.hir_id(), descriptor);
            self.visit_children_scope_with_self(&node, owner_symbol);
        } else {
            tracing::warn!(
                "failed to build impl descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        // todo!("support trait declration");
        self.visit_struct_item(node);
    }

    fn visit_function_signature_item(&mut self, node: HirNode<'tcx>) {
        self.visit_function_item(node);
    }

    fn visit_macro_definition(&mut self, node: HirNode<'tcx>) {
        if let Some(ident) = self.core.ident_from_field(&node, LangRust::field_name) {
            let (sym_idx, _fqn) =
                self.core
                    .insert_symbol(node.hir_id(), &ident.name, SymbolKind::Macro, true);
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_call(self.unit(), &node) {
            desc.enclosing = self.current_function_name().map(|name| name.to_string());
            self.calls.add(node.hir_id(), desc);
            self.visit_children(&node);
        } else {
            tracing::warn!(
                "failed to build call descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_macro_invocation(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_call(self.unit(), &node) {
            desc.enclosing = self.current_function_name().map(|name| name.to_string());
            self.calls.add(node.hir_id(), desc);
        }
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut variable) = RustDescriptor::build_variable(self.unit(), &node) {
            let is_global = Self::visibility_exports(&variable.visibility);
            let (sym_idx, fqn) = self.core.insert_symbol(
                node.hir_id(),
                &variable.name,
                SymbolKind::Const,
                is_global,
            );
            variable.fqn = Some(fqn);
            self.variables.add(node.hir_id(), variable);
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!(
                "failed to build const descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        self.visit_const_item(node);
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

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_struct(self.unit(), &node) {
            let is_global = Self::visibility_exports(&desc.visibility);
            let (sym_idx, fqn) =
                self.core
                    .insert_symbol(node.hir_id(), &desc.name, SymbolKind::Struct, is_global);
            desc.fqn = Some(fqn.clone());
            self.structs.add(node.hir_id(), desc);
            self.visit_children_scope_with_self(&node, sym_idx);
        } else {
            tracing::warn!(
                "failed to build struct descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_enum(self.unit(), &node) {
            let is_global = Self::visibility_exports(&desc.visibility);
            let (sym_idx, fqn) =
                self.core
                    .insert_symbol(node.hir_id(), &desc.name, SymbolKind::Enum, is_global);
            desc.fqn = Some(fqn.clone());
            self.enums.add(node.hir_id(), desc);
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!(
                "failed to build enum descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        let owner_symbol = self
            .core
            .insert_field_symbol(&node, LangRust::field_name, SymbolKind::EnumVariant)
            .map(|(idx, _)| idx);

        if let Some(sym_idx) = owner_symbol {
            self.visit_children_scope(&node, Some(sym_idx));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn collect_symbols(unit: CompileUnit<'_>) -> CollectedSymbols {
    let (collected, total_time, visit_time) = collect_symbols_batch(
        unit,
        DeclCollector::new,
        |collector, node| collector.visit_node(node),
        DeclCollector::finish,
    );

    if total_time.as_millis() > 10 {
        let result = &collected.result;
        tracing::trace!(
            "[COLLECT][rust] File {:?}: total={:.2}ms, visit={:.2}ms, fns={}, structs={}, impls={}, vars={}, enums={}, calls={}",
            unit.file_path().unwrap_or("unknown"),
            total_time.as_secs_f64() * 1000.0,
            visit_time.as_secs_f64() * 1000.0,
            result.functions.len(),
            result.structs.len(),
            result.impls.len(),
            result.variables.len(),
            result.enums.len(),
            result.calls.len(),
        );
    }

    collected
}
