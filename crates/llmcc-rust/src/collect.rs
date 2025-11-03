use std::collections::HashMap;
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode};
use llmcc_core::symbol::{Scope, Symbol, SymbolKind};
use llmcc_descriptor::DescriptorTrait;

use crate::describe::{
    CallDescriptor, ClassDescriptor, EnumDescriptor, FunctionDescriptor, RustDescriptor,
    StructDescriptor, VariableDescriptor, Visibility,
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

#[derive(Debug)]
pub struct SymbolSpec {
    pub owner: HirId,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub unit_index: usize,
    pub is_global: bool,
}

#[derive(Debug)]
pub struct ScopeSpec {
    pub owner: Option<HirId>,
    pub symbol_index: Option<usize>,
    pub symbols: Vec<usize>,
}

#[derive(Debug)]
pub struct CollectedSymbols {
    pub result: CollectionResult,
    pub symbols: Vec<SymbolSpec>,
    pub scopes: Vec<ScopeSpec>,
}

#[derive(Debug)]
pub struct SymbolBatch {
    pub collected: CollectedSymbols,
    pub total_time: Duration,
    pub visit_time: Duration,
}

#[derive(Debug)]
struct ScopeInfo {
    owner: Option<HirId>,
    symbol_index: Option<usize>,
    symbols: Vec<usize>,
    locals: HashMap<String, usize>,
}

#[derive(Debug)]
struct DeclCollector<'tcx> {
    unit: CompileUnit<'tcx>,
    scope_infos: Vec<ScopeInfo>,
    scope_lookup: HashMap<HirId, usize>,
    scope_stack: Vec<usize>,
    symbols: Vec<SymbolSpec>,
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

#[allow(clippy::needless_lifetimes)]
impl<'tcx> DeclCollector<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            scope_infos: vec![ScopeInfo {
                owner: None,
                symbol_index: None,
                symbols: Vec::new(),
                locals: HashMap::new(),
            }],
            scope_lookup: HashMap::new(),
            scope_stack: vec![0],
            symbols: Vec::new(),
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

    fn current_scope_index(&self) -> usize {
        *self
            .scope_stack
            .last()
            .expect("scope stack should never be empty")
    }

    fn ensure_scope(&mut self, owner: HirId) -> usize {
        if let Some(&idx) = self.scope_lookup.get(&owner) {
            return idx;
        }

        let idx = self.scope_infos.len();
        self.scope_infos.push(ScopeInfo {
            owner: Some(owner),
            symbol_index: None,
            symbols: Vec::new(),
            locals: HashMap::new(),
        });
        self.scope_lookup.insert(owner, idx);
        idx
    }

    fn parent_symbol(&self) -> Option<&SymbolSpec> {
        for &scope_idx in self.scope_stack.iter().rev() {
            if let Some(sym_idx) = self.scope_infos[scope_idx].symbol_index {
                if let Some(symbol) = self.symbols.get(sym_idx) {
                    return Some(symbol);
                }
            }
        }
        None
    }

    fn current_function_name(&self) -> Option<&str> {
        for &scope_idx in self.scope_stack.iter().rev() {
            let info = &self.scope_infos[scope_idx];
            if let Some(sym_idx) = info.symbol_index {
                if let Some(symbol) = self.symbols.get(sym_idx) {
                    if symbol.kind == SymbolKind::Function {
                        return Some(symbol.name.as_str());
                    }
                }
            }
        }
        None
    }

    fn scoped_qualified_name(&self, _node: &HirNode<'tcx>, name: &str) -> String {
        let mut prefix = None;
        for &scope_idx in self.scope_stack.iter().rev() {
            if let Some(sym_idx) = self.scope_infos[scope_idx].symbol_index {
                if let Some(symbol) = self.symbols.get(sym_idx) {
                    if !symbol.fqn.is_empty() {
                        prefix = Some(symbol.fqn.as_str());
                        break;
                    }
                }
            }
        }

        if let Some(prefix) = prefix {
            format!("{}::{}", prefix, name)
        } else {
            let unit_prefix = format!("unit{}", self.unit.index);
            format!("{}::{}", unit_prefix, name)
        }
    }

    fn upsert_symbol_internal(
        &mut self,
        node: &HirNode<'tcx>,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
        fqn_hint: Option<&str>,
    ) -> (usize, String) {
        let owner = node.hir_id();

        if let Some(fqn) = fqn_hint {
            if let Some(idx) = self.find_symbol_by_fqn(fqn) {
                self.symbol_update(idx, owner, is_global);
                return (idx, fqn.to_string());
            }
        } else if let Some(existing_idx) = self.find_symbol_local(name) {
            let existing_kind = self.symbols[existing_idx].kind;
            if existing_kind != SymbolKind::Unknown && existing_kind != kind {
                let fqn = self.scoped_qualified_name(node, name);
                let idx = self.insert_symbol(owner, name.to_string(), fqn.clone(), kind, is_global);
                return (idx, fqn);
            } else {
                let fqn = self.symbols[existing_idx].fqn.clone();
                self.symbols[existing_idx].is_global |= is_global;
                return (existing_idx, fqn);
            }
        }

        let fqn = fqn_hint
            .map(|value| value.to_string())
            .unwrap_or_else(|| self.scoped_qualified_name(node, name));
        let idx = self.insert_symbol(owner, name.to_string(), fqn.clone(), kind, is_global);
        (idx, fqn)
    }

    fn upsert_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
    ) -> (usize, String) {
        self.upsert_symbol_internal(node, name, kind, is_global, None)
    }

    fn upsert_symbol_with_fqn(
        &mut self,
        node: &HirNode<'tcx>,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
        fqn: &str,
    ) -> (usize, String) {
        self.upsert_symbol_internal(node, name, kind, is_global, Some(fqn))
    }

    fn symbol_update(&mut self, idx: usize, owner: HirId, is_global: bool) {
        let symbol = &mut self.symbols[idx];
        symbol.owner = owner;
        symbol.unit_index = self.unit.index;
        symbol.is_global |= is_global;
    }

    fn ident_from_field(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        let ident_node = node.opt_child_by_field(self.unit, field_id)?;
        ident_node.as_ident()
    }

    fn insert_symbol(
        &mut self,
        owner: HirId,
        name: String,
        fqn: String,
        kind: SymbolKind,
        is_global: bool,
    ) -> usize {
        let idx = self.symbols.len();
        self.symbols.push(SymbolSpec {
            owner,
            name: name.clone(),
            fqn,
            kind,
            unit_index: self.unit.index,
            is_global,
        });

        let current_scope = self.current_scope_index();
        self.scope_infos[current_scope]
            .locals
            .insert(name.clone(), idx);
        self.scope_infos[current_scope].symbols.push(idx);

        if is_global {
            self.scope_infos[0].locals.insert(name, idx);
            self.scope_infos[0].symbols.push(idx);
        }

        idx
    }

    fn find_symbol_local(&self, name: &str) -> Option<usize> {
        if self.scope_stack.len() <= 1 {
            return None;
        }

        for &scope_idx in self.scope_stack[1..].iter().rev() {
            if let Some(&sym_idx) = self.scope_infos[scope_idx].locals.get(name) {
                return Some(sym_idx);
            }
        }

        None
    }

    fn visit_children_new_scope(&mut self, node: &HirNode<'tcx>, scoped_symbol: Option<usize>) {
        let owner = node.hir_id();
        let scope_idx = self.ensure_scope(owner);
        if let Some(sym_idx) = scoped_symbol {
            self.scope_infos[scope_idx].symbol_index = Some(sym_idx);
        }

        self.scope_stack.push(scope_idx);
        self.visit_children(node);
        self.scope_stack.pop();
    }

    fn find_symbol_by_fqn(&self, fqn: &str) -> Option<usize> {
        self.symbols
            .iter()
            .enumerate()
            .rev()
            .find(|(_, symbol)| symbol.fqn == fqn)
            .map(|(idx, _)| idx)
    }

    fn find_symbol_in_scopes(&self, name: &str, kinds: &[SymbolKind]) -> Option<usize> {
        for &scope_idx in self.scope_stack.iter().rev() {
            for &symbol_idx in self.scope_infos[scope_idx].symbols.iter().rev() {
                let symbol = &self.symbols[symbol_idx];
                if symbol.name == name && kinds.contains(&symbol.kind) {
                    return Some(symbol_idx);
                }
            }
        }
        None
    }

    fn finish(self) -> CollectedSymbols {
        let scopes = self
            .scope_infos
            .into_iter()
            .map(|info| ScopeSpec {
                owner: info.owner,
                symbol_index: info.symbol_index,
                symbols: info.symbols,
            })
            .collect();

        CollectedSymbols {
            result: CollectionResult {
                functions: self.functions,
                function_map: self.function_map,
                variables: self.variables,
                variable_map: self.variable_map,
                calls: self.calls,
                call_map: self.call_map,
                structs: self.structs,
                struct_map: self.struct_map,
                impls: self.impls,
                impl_map: self.impl_map,
                enums: self.enums,
                enum_map: self.enum_map,
            },
            symbols: self.symbols,
            scopes,
        }
    }
}

impl<'tcx> AstVisitorRust<'tcx> for DeclCollector<'tcx> {
    fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    fn visit_source_file(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node, None);
    }

    fn visit_function_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_function(self.unit, &node) {
            let is_global = matches!(desc.visibility, Visibility::Public);
            let (sym_idx, fqn) =
                self.upsert_symbol(&node, &desc.name, SymbolKind::Function, is_global);
            desc.fqn = Some(fqn.clone());
            let idx = self.functions.len();
            self.functions.push(desc);
            self.function_map.insert(node.hir_id(), idx);
            self.visit_children_new_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!("build function error");
        }
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        if let Some(mut var) = RustDescriptor::build_variable(self.unit, &node) {
            let (_, fqn) = self.upsert_symbol(&node, &var.name, SymbolKind::Variable, false);
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
        self.visit_children_new_scope(&node, None);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        if let Some(ident) = self.ident_from_field(&node, LangRust::field_pattern) {
            let _ = self.upsert_symbol(&node, &ident.name, SymbolKind::Variable, false);
        }
        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let sym_idx = self
            .ident_from_field(&node, LangRust::field_name)
            .map(|ident| {
                let (sym_idx, _fqn) =
                    self.upsert_symbol(&node, &ident.name, SymbolKind::Module, true);
                sym_idx
            });
        self.visit_children_new_scope(&node, sym_idx);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_class(self.unit, &node) {
            let fqn_hint = desc
                .impl_target_fqn
                .clone()
                .unwrap_or_else(|| desc.name.clone());
            let impl_name = desc.name.clone();
            let (sym_idx, fqn) =
                self.upsert_symbol_with_fqn(&node, &impl_name, SymbolKind::Impl, false, &fqn_hint);
            desc.fqn = Some(fqn.clone());
            let target_kinds = [SymbolKind::Struct, SymbolKind::Enum, SymbolKind::Trait];
            let scope_symbol = self
                .find_symbol_in_scopes(&impl_name, &target_kinds)
                .or_else(|| self.find_symbol_by_fqn(&fqn_hint))
                .or(Some(sym_idx));
            let idx = self.impls.len();
            self.impls.push(desc);
            self.impl_map.insert(node.hir_id(), idx);
            self.visit_children_new_scope(&node, scope_symbol);
        } else {
            tracing::warn!("failed to build impl descriptor for: {:?}", node);
        }
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        self.visit_struct_item(node);
    }

    fn visit_function_signature_item(&mut self, node: HirNode<'tcx>) {
        self.visit_function_item(node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_call(self.unit, &node) {
            desc.enclosing = self.current_function_name().map(|name| name.to_string());
            let idx = self.calls.len();
            self.calls.push(desc);
            self.call_map.insert(node.hir_id(), idx);
        }
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut variable) = RustDescriptor::build_variable(self.unit, &node) {
            let is_global = matches!(variable.visibility, Visibility::Public);
            let (sym_idx, fqn) =
                self.upsert_symbol(&node, &variable.name, SymbolKind::Const, is_global);
            variable.fqn = Some(fqn);
            let idx = self.variables.len();
            self.variables.push(variable);
            self.variable_map.insert(node.hir_id(), idx);
            self.visit_children_new_scope(&node, Some(sym_idx));
            return;
        }
        self.visit_children(&node);
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        self.visit_const_item(node);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_struct(self.unit, &node) {
            let is_global = matches!(desc.visibility, Visibility::Public);
            let (sym_idx, fqn) =
                self.upsert_symbol(&node, &desc.name, SymbolKind::Struct, is_global);
            desc.fqn = Some(fqn.clone());
            let idx = self.structs.len();
            self.structs.push(desc);
            self.struct_map.insert(node.hir_id(), idx);
            self.visit_children_new_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!("failed to build struct descriptor for: {:?}", node);
        }
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        if let Some(mut desc) = RustDescriptor::build_enum(self.unit, &node) {
            let is_global = matches!(desc.visibility, Visibility::Public);
            let (sym_idx, fqn) = self.upsert_symbol(&node, &desc.name, SymbolKind::Enum, is_global);
            desc.fqn = Some(fqn.clone());
            let idx = self.enums.len();
            self.enums.push(desc);
            self.enum_map.insert(node.hir_id(), idx);
            self.visit_children_new_scope(&node, Some(sym_idx));
        } else {
            tracing::warn!("failed to build enum descriptor for: {:?}", node);
        }
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        let is_global = self
            .parent_symbol()
            .map(|symbol| symbol.is_global)
            .unwrap_or(false);
        if let Some(ident) = self.ident_from_field(&node, LangRust::field_name) {
            let _ = self.upsert_symbol(&node, &ident.name, SymbolKind::EnumVariant, is_global);
        }
        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

fn apply_collected_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    collected: &CollectedSymbols,
) {
    let interner = unit.interner();
    let mut created_symbols = Vec::with_capacity(collected.symbols.len());

    {
        let mut symbol_map = unit.cc.symbol_map.write();
        for spec in &collected.symbols {
            let key = interner.intern(&spec.name);
            let symbol = unit
                .cc
                .arena
                .alloc(Symbol::new(spec.owner, spec.name.clone(), key));
            symbol.set_kind(spec.kind);
            symbol.set_unit_index(spec.unit_index);
            symbol.set_fqn(spec.fqn.clone(), interner);
            symbol.set_is_global(spec.is_global);
            symbol_map.insert(symbol.id, symbol);
            created_symbols.push(symbol);
        }
    }

    for scope_spec in &collected.scopes {
        let target_scope = match scope_spec.owner {
            Some(owner) => unit.alloc_scope(owner),
            None => globals,
        };

        if let Some(sym_idx) = scope_spec.symbol_index {
            if let Some(symbol) = created_symbols.get(sym_idx) {
                target_scope.set_symbol(Some(symbol));
            }
        }

        for &sym_idx in &scope_spec.symbols {
            if let Some(symbol) = created_symbols.get(sym_idx) {
                target_scope.insert(symbol, interner);
            }
        }
    }
}

pub fn collect_symbols_batch(unit: CompileUnit<'_>) -> SymbolBatch {
    let collect_start = Instant::now();
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut decl_finder = DeclCollector::new(unit);

    let visit_start = Instant::now();
    decl_finder.visit_node(node);
    let visit_time = visit_start.elapsed();

    let collected = decl_finder.finish();
    let total_time = collect_start.elapsed();

    SymbolBatch {
        collected,
        total_time,
        visit_time,
    }
}

/// Applies a previously collected symbol batch into the current unit, wiring the
/// newly created symbols back into the global scope and tracing timing metrics.
pub fn apply_symbol_batch<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    batch: SymbolBatch,
) -> CollectionResult {
    // Destructure the batch so the intent (results and timings) is explicit.
    let SymbolBatch {
        collected,
        total_time,
        visit_time,
    } = batch;

    // Pre-compute interesting counts that will be reported in the trace log.
    let counts = (
        collected.result.functions.len(),
        collected.result.structs.len(),
        collected.result.impls.len(),
        collected.result.variables.len(),
        collected.result.enums.len(),
        collected.result.calls.len(),
    );

    // Materialize symbols and attach them to the appropriate scopes.
    apply_collected_symbols(unit, globals, &collected);

    // Emit a trace entry when collection took a noticeable amount of time. This
    // helps us diagnose slow files without flooding logs for trivial cases.
    if total_time.as_millis() > 10 {
        tracing::trace!(
            "[COLLECT][rust] File {:?}: total={:.2}ms, visit={:.2}ms, fns={}, structs={}, impls={}, vars={}, enums={}, calls={}",
            unit.file_path().unwrap_or("unknown"),
            total_time.as_secs_f64() * 1000.0,
            visit_time.as_secs_f64() * 1000.0,
            counts.0,
            counts.1,
            counts.2,
            counts.3,
            counts.4,
            counts.5,
        );
    }

    let CollectedSymbols { result, .. } = collected;
    result
}

pub fn collect_symbols<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
) -> CollectionResult {
    let batch = collect_symbols_batch(unit);
    apply_symbol_batch(unit, globals, batch)
}
