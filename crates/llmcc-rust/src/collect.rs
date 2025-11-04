use llmcc_core::context::CompileUnit;
use llmcc_core::ir::HirNode;
use llmcc_core::symbol::{Scope, SymbolKind};
use llmcc_descriptor::DescriptorTrait;
use llmcc_resolver::{
    collect_symbols_batch, CallCollection, CollectedSymbols, CollectionResult, CollectorCore,
    EnumCollection, FunctionCollection, ImplCollection, StructCollection, SymbolSpec,
    VariableCollection,
};

use crate::describe::{RustDescriptor, Visibility};
use crate::token::{AstVisitorRust, LangRust};

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

    fn parent_symbol(&self) -> Option<&SymbolSpec> {
        self.core.parent_symbol()
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

    fn finish(self) -> CollectedSymbols {
        let DeclCollector {
            core,
            functions,
            variables,
            calls,
            structs,
            impls,
            enums,
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
        scoped_symbol: Option<Self::ScopedSymbol>,
    ) {
        let owner = node.hir_id();
        let scope_idx = self.core.ensure_scope(owner);
        if let Some(sym_idx) = scoped_symbol {
            self.core.set_scope_symbol(scope_idx, Some(sym_idx));
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
                    .upsert_symbol(node.hir_id(), &desc.name, SymbolKind::Function, is_global);
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
            let (_, fqn) =
                self.core
                    .upsert_symbol(node.hir_id(), &var.name, SymbolKind::Variable, false);
            var.fqn = Some(fqn);
            self.variables.add(node.hir_id(), var);
            self.visit_children(&node);
            return;
        }
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        if let Some(ident) = self.core.ident_from_field(&node, LangRust::field_pattern) {
            let _ =
                self.core
                    .upsert_symbol(node.hir_id(), &ident.name, SymbolKind::Variable, false);
        }
        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let sym_idx = self
            .core
            .ident_from_field(&node, LangRust::field_name)
            .map(|ident| {
                let (sym_idx, _fqn) =
                    self.core
                        .upsert_symbol(node.hir_id(), &ident.name, SymbolKind::Module, true);
                sym_idx
            });
        self.visit_children_scope(&node, sym_idx);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_impl(self.unit(), &node) {
            let fqn_hint = desc
                .impl_target_fqn
                .clone()
                .unwrap_or_else(|| desc.name.clone());
            let impl_name = desc.name.clone();
            let (sym_idx, fqn) = self.core.upsert_symbol_with_fqn(
                node.hir_id(),
                &impl_name,
                SymbolKind::Impl,
                false,
                &fqn_hint,
            );
            desc.fqn = Some(fqn.clone());
            for base in &desc.base_types {
                self.core.upsert_symbol_from_type_expr(node.hir_id(), base);
            }
            let target_kinds = [SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Trait];
            let scope_symbol = self
                .core
                .find_symbol_in_scopes(&impl_name, &target_kinds)
                .or_else(|| self.core.find_symbol_by_fqn(&fqn_hint))
                .or(Some(sym_idx));
            self.impls.add(node.hir_id(), desc);
            self.visit_children_scope(&node, scope_symbol);
        } else {
            tracing::warn!(
                "failed to build impl descriptor for: {:?} next_hir={:?}",
                node,
                self.unit().hir_next()
            );
        }
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        self.visit_struct_item(node);
    }

    fn visit_function_signature_item(&mut self, node: HirNode<'tcx>) {
        self.visit_function_item(node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_call(self.unit(), &node) {
            desc.enclosing = self.current_function_name().map(|name| name.to_string());
            self.calls.add(node.hir_id(), desc);
        }
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut variable) = RustDescriptor::build_variable(self.unit(), &node) {
            let is_global = Self::visibility_exports(&variable.visibility);
            let (sym_idx, fqn) = self.core.upsert_symbol(
                node.hir_id(),
                &variable.name,
                SymbolKind::Const,
                is_global,
            );
            variable.fqn = Some(fqn);
            self.variables.add(node.hir_id(), variable);
            self.visit_children_scope(&node, Some(sym_idx));
            return;
        }
        self.visit_children(&node);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        self.visit_const_item(node);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_struct(self.unit(), &node) {
            let is_global = Self::visibility_exports(&desc.visibility);
            let (sym_idx, fqn) =
                self.core
                    .upsert_symbol(node.hir_id(), &desc.name, SymbolKind::Struct, is_global);
            desc.fqn = Some(fqn.clone());
            self.structs.add(node.hir_id(), desc);
            self.visit_children_scope(&node, Some(sym_idx));
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
                    .upsert_symbol(node.hir_id(), &desc.name, SymbolKind::Enum, is_global);
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
        let is_global = self
            .parent_symbol()
            .map(|symbol| symbol.is_global)
            .unwrap_or(false);
        if let Some(ident) = self.core.ident_from_field(&node, LangRust::field_name) {
            let _ = self.core.upsert_symbol(
                node.hir_id(),
                &ident.name,
                SymbolKind::EnumVariant,
                is_global,
            );
        }
        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    _globals: &'tcx Scope<'tcx>,
) -> CollectedSymbols {
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
