use std::collections::HashMap;

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode};
use llmcc_core::symbol::{Scope, SymbolKind};
use llmcc_descriptor::DescriptorTrait;
use llmcc_resolver::{CollectedSymbols as ResolverCollectedSymbols, CollectorCore, SymbolSpec};

use crate::describe::{
    CallDescriptor, ClassDescriptor, EnumDescriptor, FunctionDescriptor, RustDescriptor,
    StructDescriptor, TypeExpr, VariableDescriptor, Visibility,
};
use crate::token::{AstVisitorRust, LangRust};

#[derive(Debug)]
pub struct CollectionResult {
    pub functions: Vec<FunctionDescriptor>,
    pub function_map: HashMap<HirId, usize>,
    pub variables: Vec<VariableDescriptor>,
    pub variable_map: HashMap<HirId, usize>,
    pub calls: Vec<CallDescriptor>,
    pub call_map: HashMap<HirId, usize>,
    pub structs: Vec<StructDescriptor>,
    pub struct_map: HashMap<HirId, usize>,
    pub impls: Vec<ClassDescriptor>,
    pub impl_map: HashMap<HirId, usize>,
    pub enums: Vec<EnumDescriptor>,
    pub enum_map: HashMap<HirId, usize>,
}

pub type CollectedSymbols = ResolverCollectedSymbols<CollectionResult>;
pub type SymbolBatch = llmcc_resolver::SymbolBatch<CollectionResult>;

#[derive(Debug)]
struct DeclCollector<'tcx> {
    core: CollectorCore<'tcx>,
    functions: Vec<FunctionDescriptor>,
    function_map: HashMap<HirId, usize>,
    variables: Vec<VariableDescriptor>,
    variable_map: HashMap<HirId, usize>,
    calls: Vec<CallDescriptor>,
    call_map: HashMap<HirId, usize>,
    structs: Vec<StructDescriptor>,
    struct_map: HashMap<HirId, usize>,
    impls: Vec<ClassDescriptor>,
    impl_map: HashMap<HirId, usize>,
    enums: Vec<EnumDescriptor>,
    enum_map: HashMap<HirId, usize>,
}

impl<'tcx> DeclCollector<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            core: CollectorCore::new(unit),
            functions: Vec::new(),
            function_map: HashMap::new(),
            variables: Vec::new(),
            variable_map: HashMap::new(),
            calls: Vec::new(),
            call_map: HashMap::new(),
            structs: Vec::new(),
            struct_map: HashMap::new(),
            impls: Vec::new(),
            impl_map: HashMap::new(),
            enums: Vec::new(),
            enum_map: HashMap::new(),
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

    fn ident_from_field(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        let unit = self.unit();
        let ident_node = node.opt_child_by_field(unit, field_id)?;
        ident_node.as_ident()
    }

    fn visibility_exports(visibility: &Visibility) -> bool {
        match visibility {
            Visibility::Public => true,
            Visibility::Restricted { scope } => scope == "crate",
            _ => false,
        }
    }

    fn ensure_base_type_symbol(&mut self, node: &HirNode<'tcx>, base: &TypeExpr) {
        match base {
            TypeExpr::Path { segments, .. } => {
                let segments: Vec<String> = segments
                    .iter()
                    .filter(|segment| !segment.is_empty())
                    .cloned()
                    .collect();
                if segments.is_empty() {
                    return;
                }

                let name = segments.last().cloned().unwrap();
                let fqn = segments.join("::");
                let _ = self.core.upsert_symbol_with_fqn(
                    node.hir_id(),
                    &name,
                    SymbolKind::Trait,
                    true,
                    &fqn,
                );
            }
            TypeExpr::Reference { inner, .. } => {
                self.ensure_base_type_symbol(node, inner);
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.ensure_base_type_symbol(node, item);
                }
            }
            _ => {}
        }
    }

    fn finish(self) -> CollectedSymbols {
        let DeclCollector {
            core,
            functions,
            function_map,
            variables,
            variable_map,
            calls,
            call_map,
            structs,
            struct_map,
            impls,
            impl_map,
            enums,
            enum_map,
        } = self;

        let result = CollectionResult {
            functions,
            function_map,
            variables,
            variable_map,
            calls,
            call_map,
            structs,
            struct_map,
            impls,
            impl_map,
            enums,
            enum_map,
        };

        core.finish(result)
    }
}

impl<'tcx> AstVisitorRust<'tcx> for DeclCollector<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit()
    }

    fn visit_children_new_scope(&mut self, node: &HirNode<'tcx>, scoped_symbol: Option<usize>) {
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
            let idx = self.functions.len();
            self.functions.push(desc);
            self.function_map.insert(node.hir_id(), idx);
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
            let idx = self.variables.len();
            self.variables.push(var);
            self.variable_map.insert(node.hir_id(), idx);
            self.visit_children(&node);
            return;
        }
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_scope(&node, None);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        if let Some(ident) = self.ident_from_field(&node, LangRust::field_pattern) {
            let _ =
                self.core
                    .upsert_symbol(node.hir_id(), &ident.name, SymbolKind::Variable, false);
        }
        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let sym_idx = self
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
                self.ensure_base_type_symbol(&node, base);
            }
            let target_kinds = [SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Trait];
            let scope_symbol = self
                .core
                .find_symbol_in_scopes(&impl_name, &target_kinds)
                .or_else(|| self.core.find_symbol_by_fqn(&fqn_hint))
                .or(Some(sym_idx));
            let idx = self.impls.len();
            self.impls.push(desc);
            self.impl_map.insert(node.hir_id(), idx);
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
            let idx = self.calls.len();
            self.calls.push(desc);
            self.call_map.insert(node.hir_id(), idx);
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
            let idx = self.variables.len();
            self.variables.push(variable);
            self.variable_map.insert(node.hir_id(), idx);
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
            let idx = self.structs.len();
            self.structs.push(desc);
            self.struct_map.insert(node.hir_id(), idx);
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
            let idx = self.enums.len();
            self.enums.push(desc);
            self.enum_map.insert(node.hir_id(), idx);
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
        if let Some(ident) = self.ident_from_field(&node, LangRust::field_name) {
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

pub fn collect_symbols_batch(unit: CompileUnit<'_>) -> SymbolBatch {
    llmcc_resolver::collect_symbols_batch(
        unit,
        DeclCollector::new,
        |collector, node| collector.visit_node(node),
        DeclCollector::finish,
    )
}

/// Applies a previously collected symbol batch into the current unit, wiring the
/// newly created symbols back into the global scope and tracing timing metrics.
pub fn apply_symbol_batch<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    batch: SymbolBatch,
) -> CollectionResult {
    let (result, total_time, visit_time) = llmcc_resolver::apply_symbol_batch(unit, globals, batch);

    if total_time.as_millis() > 10 {
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

    result
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
) -> CollectionResult {
    let batch = collect_symbols_batch(unit);
    apply_symbol_batch(unit, globals, batch)
}
