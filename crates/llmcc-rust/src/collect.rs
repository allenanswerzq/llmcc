use std::collections::HashMap;
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode};
use llmcc_core::symbol::{Scope, Symbol, SymbolKind};

use crate::descriptor::function;
use crate::descriptor::{
    enumeration, structure, variable, CallDescriptor, EnumDescriptor, FunctionDescriptor,
    StructDescriptor, TypeExpr, VariableDescriptor, Visibility,
};
use crate::token::{AstVisitorRust, LangRust};

#[derive(Debug)]
pub struct CollectionResult {
    pub functions: Vec<FunctionDescriptor>,
    pub variables: Vec<VariableDescriptor>,
    pub calls: Vec<CallDescriptor>,
    pub structs: Vec<StructDescriptor>,
    pub enums: Vec<EnumDescriptor>,
}

#[derive(Debug)]
pub struct SymbolSpec {
    pub owner: HirId,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub unit_index: usize,
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
    variables: Vec<VariableDescriptor>,
    calls: Vec<CallDescriptor>,
    structs: Vec<StructDescriptor>,
    enums: Vec<EnumDescriptor>,
}

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
            variables: Vec::new(),
            calls: Vec::new(),
            structs: Vec::new(),
            enums: Vec::new(),
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
            if let Some(symbol_idx) = self.scope_infos[scope_idx].symbol_index {
                if let Some(symbol) = self.symbols.get(symbol_idx) {
                    return Some(symbol);
                }
            }
        }
        None
    }

    fn scoped_fqn(&self, _node: &HirNode<'tcx>, name: &str) -> String {
        if let Some(parent) = self.parent_symbol() {
            if parent.fqn.is_empty() {
                name.to_string()
            } else {
                format!("{}::{}", parent.fqn, name)
            }
        } else {
            name.to_string()
        }
    }

    fn create_new_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        global: bool,
        kind: SymbolKind,
    ) -> Option<(usize, &'tcx HirIdent<'tcx>, String)> {
        let ident_node = node.opt_child_by_field(self.unit, field_id)?;
        let ident = ident_node.as_ident()?;
        let name = ident.name.clone();
        let owner = node.hir_id();

        if let Some(existing_idx) = self.find_symbol_local(&name) {
            let existing_kind = self.symbols[existing_idx].kind;
            if Self::different_kind(existing_kind, kind) {
                let fqn = self.scoped_fqn(node, &name);
                let idx = self.insert_symbol(owner, name.clone(), fqn.clone(), kind, global);
                return Some((idx, ident, fqn));
            } else {
                let fqn = self.symbols[existing_idx].fqn.clone();
                return Some((existing_idx, ident, fqn));
            }
        }

        let fqn = self.scoped_fqn(node, &name);
        let idx = self.insert_symbol(owner, name.clone(), fqn.clone(), kind, global);
        Some((idx, ident, fqn))
    }

    fn different_kind(existing_kind: SymbolKind, new_kind: SymbolKind) -> bool {
        existing_kind != SymbolKind::Unknown && existing_kind != new_kind
    }

    fn insert_symbol(
        &mut self,
        owner: HirId,
        name: String,
        fqn: String,
        kind: SymbolKind,
        global: bool,
    ) -> usize {
        let idx = self.symbols.len();
        self.symbols.push(SymbolSpec {
            owner,
            name: name.clone(),
            fqn,
            kind,
            unit_index: self.unit.index,
        });

        let current_scope = self.current_scope_index();
        self.scope_infos[current_scope]
            .locals
            .insert(name.clone(), idx);
        self.scope_infos[current_scope].symbols.push(idx);

        if global {
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
            if let Some(&symbol_idx) = self.scope_infos[scope_idx].locals.get(name) {
                return Some(symbol_idx);
            }
        }

        None
    }

    fn has_public_visibility(&self, node: &HirNode<'tcx>) -> bool {
        let ts_node = node.inner_ts_node();
        let Some(name_node) = ts_node.child_by_field_name("name") else {
            return false;
        };
        let header = self
            .unit
            .file()
            .opt_get_text(ts_node.start_byte(), name_node.start_byte())
            .unwrap_or_default();

        !matches!(function::parse_visibility(&header), Visibility::Private)
    }

    fn should_register_globally(&self, node: &HirNode<'tcx>) -> bool {
        self.parent_symbol().is_none() || self.has_public_visibility(node)
    }

    fn enum_variant_should_register_globally(&self, _node: &HirNode<'tcx>) -> bool {
        let Some(enum_symbol) = self.parent_symbol() else {
            return false;
        };

        if enum_symbol.kind != SymbolKind::Enum {
            return false;
        }

        let parent_node = self.unit.hir_node(enum_symbol.owner);
        self.has_public_visibility(&parent_node)
    }

    fn visit_children_new_scope(&mut self, node: &HirNode<'tcx>, scoped_symbol: Option<usize>) {
        let owner = node.hir_id();
        let scope_idx = self.ensure_scope(owner);
        if let Some(symbol_idx) = scoped_symbol {
            self.scope_infos[scope_idx].symbol_index = Some(symbol_idx);
        }

        self.scope_stack.push(scope_idx);
        self.visit_children(node);
        self.scope_stack.pop();
    }

    fn visit_children(&mut self, node: &HirNode<'tcx>) {
        for id in node.children() {
            let child = self.unit.hir_node(*id);
            self.visit_node(child);
        }
    }

    fn find_symbol_by_fqn_in_unit(&self, fqn: &str) -> Option<usize> {
        self.symbols
            .iter()
            .enumerate()
            .rev()
            .find(|(_, symbol)| symbol.fqn == fqn && symbol.unit_index == self.unit.index)
            .map(|(idx, _)| idx)
    }

    fn find_symbol_by_fqn(&self, fqn: &str) -> Option<usize> {
        self.symbols
            .iter()
            .enumerate()
            .rev()
            .find(|(_, symbol)| symbol.fqn == fqn)
            .map(|(idx, _)| idx)
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
                variables: self.variables,
                calls: self.calls,
                structs: self.structs,
                enums: self.enums,
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
        let register_globally = self.should_register_globally(&node);
        if let Some((symbol_idx, _ident, fqn)) = self.create_new_symbol(
            &node,
            LangRust::field_name,
            register_globally,
            SymbolKind::Function,
        ) {
            if let Some(desc) = function::from_hir(self.unit, &node, fqn.clone()) {
                self.functions.push(desc);
            }
            self.visit_children_new_scope(&node, Some(symbol_idx));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>) {
        if let Some((_symbol_idx, ident, fqn)) =
            self.create_new_symbol(&node, LangRust::field_pattern, false, SymbolKind::Variable)
        {
            let var = variable::from_let(self.unit, &node, ident.name.clone(), fqn.clone());
            self.variables.push(var);
        }
        self.visit_children(&node);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>) {
        self.visit_children_new_scope(&node, None);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>) {
        let _ = self.create_new_symbol(&node, LangRust::field_pattern, false, SymbolKind::Variable);
        self.visit_children(&node);
    }

    fn visit_mod_item(&mut self, node: HirNode<'tcx>) {
        let symbol_idx = self
            .create_new_symbol(&node, LangRust::field_name, true, SymbolKind::Module)
            .map(|(symbol_idx, _ident, _)| symbol_idx);
        self.visit_children_new_scope(&node, symbol_idx);
    }

    fn visit_impl_item(&mut self, node: HirNode<'tcx>) {
        let symbol_idx = node
            .opt_child_by_field(self.unit, LangRust::field_type)
            .and_then(|type_node| {
                let segments = impl_type_segments(self.unit, &type_node)?;
                if segments.is_empty() {
                    return None;
                }

                let fqn = segments.join("::");
                let impl_name = segments
                    .last()
                    .cloned()
                    .unwrap_or_else(|| "impl".to_string());

                if let Some(idx) = self.find_symbol_by_fqn_in_unit(&fqn) {
                    let symbol = &mut self.symbols[idx];
                    symbol.owner = node.hir_id();
                    symbol.unit_index = self.unit.index;
                    return Some(idx);
                }

                if let Some(idx) = self.find_symbol_by_fqn(&fqn) {
                    let symbol = &mut self.symbols[idx];
                    symbol.owner = node.hir_id();
                    symbol.unit_index = self.unit.index;
                    return Some(idx);
                }

                if let Some(idx) = self.scope_infos[0].locals.get(&impl_name).copied() {
                    let symbol = &mut self.symbols[idx];
                    symbol.owner = node.hir_id();
                    symbol.unit_index = self.unit.index;
                    return Some(idx);
                }

                Some(self.insert_symbol(node.hir_id(), impl_name, fqn, SymbolKind::Impl, false))
            });
        self.visit_children_new_scope(&node, symbol_idx);
    }

    fn visit_trait_item(&mut self, node: HirNode<'tcx>) {
        self.visit_struct_item(node);
    }

    fn visit_function_signature_item(&mut self, node: HirNode<'tcx>) {
        self.visit_function_item(node);
    }

    fn visit_call_expression(&mut self, node: HirNode<'tcx>) {
        let enclosing = self.parent_symbol().map(|symbol| symbol.fqn.clone());
        let desc = crate::descriptor::call::from_call(self.unit, &node, enclosing);
        self.calls.push(desc);
        self.visit_children(&node);
    }

    fn visit_const_item(&mut self, node: HirNode<'tcx>) {
        let ts_kind = node.inner_ts_node().kind();
        let symbol_kind = if ts_kind == "static_item" {
            SymbolKind::Static
        } else {
            SymbolKind::Const
        };
        if let Some((symbol_idx, ident, fqn)) =
            self.create_new_symbol(&node, LangRust::field_name, true, symbol_kind)
        {
            let variable = match ts_kind {
                "const_item" => {
                    variable::from_const_item(self.unit, &node, ident.name.clone(), fqn.clone())
                }
                "static_item" => {
                    variable::from_static_item(self.unit, &node, ident.name.clone(), fqn.clone())
                }
                _ => return,
            };
            self.variables.push(variable);
            self.visit_children_new_scope(&node, Some(symbol_idx));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_static_item(&mut self, node: HirNode<'tcx>) {
        self.visit_const_item(node);
    }

    fn visit_struct_item(&mut self, node: HirNode<'tcx>) {
        let register_globally = self.should_register_globally(&node);
        if let Some((symbol_idx, _ident, fqn)) = self.create_new_symbol(
            &node,
            LangRust::field_name,
            register_globally,
            SymbolKind::Struct,
        ) {
            if let Some(desc) = structure::from_struct(self.unit, &node, fqn.clone()) {
                self.structs.push(desc);
            }
            self.visit_children_new_scope(&node, Some(symbol_idx));
        } else {
            eprintln!("Failed to create struct descriptor for: {:?}", node);
            self.visit_children(&node);
        }
    }

    fn visit_enum_item(&mut self, node: HirNode<'tcx>) {
        let register_globally = self.should_register_globally(&node);
        if let Some((symbol_idx, _ident, fqn)) = self.create_new_symbol(
            &node,
            LangRust::field_name,
            register_globally,
            SymbolKind::Enum,
        ) {
            if let Some(desc) = enumeration::from_enum(self.unit, &node, fqn.clone()) {
                self.enums.push(desc);
            }
            self.visit_children_new_scope(&node, Some(symbol_idx));
        } else {
            self.visit_children(&node);
        }
    }

    fn visit_enum_variant(&mut self, node: HirNode<'tcx>) {
        let register_globally = self.enum_variant_should_register_globally(&node);
        let _ = self.create_new_symbol(
            &node,
            LangRust::field_name,
            register_globally,
            SymbolKind::EnumVariant,
        );
        self.visit_children(&node);
    }

    fn visit_unknown(&mut self, node: HirNode<'tcx>) {
        self.visit_children(&node);
    }
}

fn impl_type_segments<'tcx>(
    unit: CompileUnit<'tcx>,
    type_node: &HirNode<'tcx>,
) -> Option<Vec<String>> {
    let ts_node = type_node.inner_ts_node();
    let expr = function::parse_type_expr(unit, ts_node);
    extract_path_segments(&expr)
}

fn extract_path_segments(expr: &TypeExpr) -> Option<Vec<String>> {
    match expr {
        TypeExpr::Path { segments, .. } => Some(segments.clone()),
        TypeExpr::Reference { inner, .. } => extract_path_segments(inner),
        TypeExpr::Tuple(items) if items.len() == 1 => extract_path_segments(&items[0]),
        _ => None,
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
            symbol_map.insert(symbol.id, symbol);
            created_symbols.push(symbol);
        }
    }

    for scope_spec in &collected.scopes {
        let target_scope = match scope_spec.owner {
            Some(owner) => unit.alloc_scope(owner),
            None => globals,
        };

        if let Some(symbol_idx) = scope_spec.symbol_index {
            if let Some(symbol) = created_symbols.get(symbol_idx) {
                target_scope.set_symbol(Some(symbol));
            }
        }

        for &symbol_idx in &scope_spec.symbols {
            if let Some(symbol) = created_symbols.get(symbol_idx) {
                target_scope.insert(symbol, interner);
            }
        }
    }
}

pub fn collect_symbols_batch<'tcx>(unit: CompileUnit<'tcx>) -> SymbolBatch {
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

pub fn apply_symbol_batch<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    batch: SymbolBatch,
) -> CollectionResult {
    let SymbolBatch {
        collected,
        total_time,
        visit_time,
    } = batch;

    let counts = (
        collected.result.functions.len(),
        collected.result.structs.len(),
        collected.result.variables.len(),
        collected.result.enums.len(),
        collected.result.calls.len(),
    );

    apply_collected_symbols(unit, globals, &collected);

    if total_time.as_millis() > 10 {
        tracing::trace!(
            "[COLLECT][rust] File {:?}: total={:.2}ms, visit={:.2}ms, fns={}, structs={}, vars={}, enums={}, calls={}",
            unit.file_path().unwrap_or("unknown"),
            total_time.as_secs_f64() * 1000.0,
            visit_time.as_secs_f64() * 1000.0,
            counts.0,
            counts.1,
            counts.2,
            counts.3,
            counts.4,
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
