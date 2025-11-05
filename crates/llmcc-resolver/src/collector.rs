use std::collections::HashMap;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice::{Iter, IterMut};
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode};
use llmcc_core::symbol::{Scope, Symbol, SymbolKind};
use llmcc_descriptor::{
    CallDescriptor, ClassDescriptor, EnumDescriptor, FunctionDescriptor, ImportDescriptor,
    StructDescriptor, TypeExpr, VariableDescriptor,
};

#[derive(Debug, Clone)]
pub struct SymbolSpec {
    pub owner: HirId,
    pub name: String,
    pub fqn: String,
    pub kind: SymbolKind,
    pub unit_index: usize,
    pub is_global: bool,
}

#[derive(Debug, Clone)]
pub struct ScopeSpec {
    pub owner: Option<HirId>,
    pub symbol_index: Option<usize>,
    pub symbols: Vec<usize>,
}

#[derive(Debug)]
pub struct DescriptorCollection<T> {
    descriptors: Vec<T>,
    index_by_hir: HashMap<HirId, usize>,
}

impl<T> DescriptorCollection<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub fn is_empty(&self) -> bool {
        self.descriptors.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, T> {
        self.descriptors.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<'_, T> {
        self.descriptors.iter_mut()
    }

    pub fn add(&mut self, hir_id: HirId, descriptor: T) -> usize {
        let index = self.descriptors.len();
        self.descriptors.push(descriptor);
        self.index_by_hir.insert(hir_id, index);
        index
    }

    pub fn insert_index(&mut self, hir_id: HirId, index: usize) {
        self.index_by_hir.insert(hir_id, index);
    }

    pub fn get_index(&self, hir_id: HirId) -> Option<usize> {
        self.index_by_hir.get(&hir_id).copied()
    }

    pub fn find(&self, hir_id: HirId) -> Option<&T> {
        self.get_index(hir_id)
            .and_then(|idx| self.descriptors.get(idx))
    }

    pub fn find_mut(&mut self, hir_id: HirId) -> Option<&mut T> {
        self.get_index(hir_id)
            .and_then(move |idx| self.descriptors.get_mut(idx))
    }

    pub fn map(&self) -> &HashMap<HirId, usize> {
        &self.index_by_hir
    }

    pub fn map_mut(&mut self) -> &mut HashMap<HirId, usize> {
        &mut self.index_by_hir
    }

    pub fn into_vec(self) -> Vec<T> {
        self.descriptors
    }
}

impl<T> Default for DescriptorCollection<T> {
    fn default() -> Self {
        Self {
            descriptors: Vec::new(),
            index_by_hir: HashMap::new(),
        }
    }
}

impl<T> Deref for DescriptorCollection<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.descriptors
    }
}

impl<T> DerefMut for DescriptorCollection<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.descriptors
    }
}

impl<T> Index<usize> for DescriptorCollection<T> {
    type Output = T;

    fn index(&self, index: usize) -> &Self::Output {
        &self.descriptors[index]
    }
}

impl<T> IndexMut<usize> for DescriptorCollection<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.descriptors[index]
    }
}

impl<T> IntoIterator for DescriptorCollection<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.descriptors.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a DescriptorCollection<T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.descriptors.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut DescriptorCollection<T> {
    type Item = &'a mut T;
    type IntoIter = IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.descriptors.iter_mut()
    }
}

impl<T: Clone> Clone for DescriptorCollection<T> {
    fn clone(&self) -> Self {
        Self {
            descriptors: self.descriptors.clone(),
            index_by_hir: self.index_by_hir.clone(),
        }
    }
}

pub type FunctionCollection = DescriptorCollection<FunctionDescriptor>;
pub type ClassCollection = DescriptorCollection<ClassDescriptor>;
pub type StructCollection = DescriptorCollection<StructDescriptor>;
pub type ImplCollection = DescriptorCollection<ClassDescriptor>;
pub type EnumCollection = DescriptorCollection<EnumDescriptor>;
pub type VariableCollection = DescriptorCollection<VariableDescriptor>;
pub type ImportCollection = DescriptorCollection<ImportDescriptor>;
pub type CallCollection = DescriptorCollection<CallDescriptor>;

#[derive(Debug, Default)]
pub struct CollectionResult {
    pub functions: FunctionCollection,
    pub classes: ClassCollection,
    pub structs: StructCollection,
    pub impls: ImplCollection,
    pub enums: EnumCollection,
    pub variables: VariableCollection,
    pub imports: ImportCollection,
    pub calls: CallCollection,
}

#[derive(Debug)]
pub struct CollectedSymbols {
    pub result: CollectionResult,
    pub symbols: Vec<SymbolSpec>,
    pub scopes: Vec<ScopeSpec>,
}

impl Deref for CollectedSymbols {
    type Target = CollectionResult;

    fn deref(&self) -> &Self::Target {
        &self.result
    }
}

impl DerefMut for CollectedSymbols {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.result
    }
}

#[derive(Debug)]
struct ScopeInfo {
    owner: Option<HirId>,
    symbol_index: Option<usize>,
    symbols: Vec<usize>,
    locals: HashMap<String, usize>,
}

#[derive(Debug)]
pub struct CollectorCore<'tcx> {
    unit: CompileUnit<'tcx>,
    scope_infos: Vec<ScopeInfo>,
    scope_lookup: HashMap<HirId, usize>,
    scope_stack: Vec<usize>,
    symbols: Vec<SymbolSpec>,
}

impl<'tcx> CollectorCore<'tcx> {
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
        }
    }

    #[inline]
    pub fn unit(&self) -> CompileUnit<'tcx> {
        self.unit
    }

    #[inline]
    pub fn unit_index(&self) -> usize {
        self.unit.index
    }

    pub fn current_scope_index(&self) -> usize {
        *self
            .scope_stack
            .last()
            .expect("scope stack should never be empty")
    }

    pub fn scope_depth(&self) -> usize {
        self.scope_stack.len()
    }

    pub fn ensure_scope(&mut self, owner: HirId) -> usize {
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

    pub fn set_scope_symbol(&mut self, scope_idx: usize, symbol_index: Option<usize>) {
        self.scope_infos[scope_idx].symbol_index = symbol_index;
    }

    pub fn push_scope(&mut self, scope_idx: usize) {
        self.scope_stack.push(scope_idx);
    }

    pub fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    pub fn pop_until(&mut self, depth: usize) {
        while self.scope_stack.len() > depth {
            self.scope_stack.pop();
        }
    }

    pub fn parent_symbol(&self) -> Option<&SymbolSpec> {
        for &scope_idx in self.scope_stack.iter().rev() {
            if let Some(sym_idx) = self.scope_infos[scope_idx].symbol_index {
                if let Some(symbol) = self.symbols.get(sym_idx) {
                    return Some(symbol);
                }
            }
        }
        None
    }

    pub fn current_function_name(&self) -> Option<&str> {
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

    pub fn ident_from_field(
        &self,
        node: &HirNode<'tcx>,
        field_id: u16,
    ) -> Option<&'tcx HirIdent<'tcx>> {
        let unit = self.unit();
        let ident_node = node.opt_child_by_field(unit, field_id)?;
        ident_node.as_ident()
    }

    pub fn scoped_qualified_name(&self, name: &str) -> String {
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
            name.to_string()
        }
    }

    pub fn upsert_symbol_from_type_expr(&mut self, owner: HirId, expr: &TypeExpr) {
        match expr {
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
                let _ = self.upsert_symbol_with_fqn(owner, &name, SymbolKind::Trait, true, &fqn);
            }
            TypeExpr::Reference { inner, .. } => {
                self.upsert_symbol_from_type_expr(owner, inner);
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    self.upsert_symbol_from_type_expr(owner, item);
                }
            }
            _ => {}
        }
    }

    pub fn upsert_symbol_internal(
        &mut self,
        owner: HirId,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
        fqn_hint: Option<&str>,
    ) -> (usize, String) {
        if let Some(fqn) = fqn_hint {
            if let Some(idx) = self.find_symbol_by_fqn(fqn) {
                self.symbol_update(idx, owner, is_global);
                return (idx, fqn.to_string());
            }
        } else if let Some(existing_idx) = self.find_symbol_local(name) {
            let existing_kind = self.symbols[existing_idx].kind;
            if existing_kind != SymbolKind::Unknown && existing_kind != kind {
                let fqn = self.scoped_qualified_name(name);
                let idx = self.insert_symbol(owner, name.to_string(), fqn.clone(), kind, is_global);
                return (idx, fqn);
            } else {
                if let Some(symbol) = self.symbols.get_mut(existing_idx) {
                    symbol.is_global |= is_global;
                }
                let fqn = self.symbols[existing_idx].fqn.clone();
                return (existing_idx, fqn);
            }
        }

        let fqn = fqn_hint
            .map(|value| value.to_string())
            .unwrap_or_else(|| self.scoped_qualified_name(name));
        let idx = self.insert_symbol(owner, name.to_string(), fqn.clone(), kind, is_global);
        (idx, fqn)
    }

    pub fn upsert_symbol(
        &mut self,
        owner: HirId,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
    ) -> (usize, String) {
        self.upsert_symbol_internal(owner, name, kind, is_global, None)
    }

    pub fn upsert_symbol_with_fqn(
        &mut self,
        owner: HirId,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
        fqn: &str,
    ) -> (usize, String) {
        self.upsert_symbol_internal(owner, name, kind, is_global, Some(fqn))
    }

    pub fn find_symbol_by_fqn(&self, fqn: &str) -> Option<usize> {
        self.symbols
            .iter()
            .enumerate()
            .rev()
            .find(|(_, symbol)| symbol.fqn == fqn)
            .map(|(idx, _)| idx)
    }

    pub fn find_symbol_local(&self, name: &str) -> Option<usize> {
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

    pub fn find_symbol_in_scopes(&self, name: &str, kinds: &[SymbolKind]) -> Option<usize> {
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

    pub fn symbols(&self) -> &[SymbolSpec] {
        &self.symbols
    }

    pub fn symbols_mut(&mut self) -> &mut [SymbolSpec] {
        &mut self.symbols
    }

    pub fn finish(self, result: CollectionResult) -> CollectedSymbols {
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
            result,
            symbols: self.symbols,
            scopes,
        }
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
            unit_index: self.unit_index(),
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

    fn symbol_update(&mut self, idx: usize, owner: HirId, is_global: bool) {
        let unit_index = self.unit_index();
        if let Some(symbol) = self.symbols.get_mut(idx) {
            symbol.owner = owner;
            symbol.unit_index = unit_index;
            symbol.is_global |= is_global;
        }
    }
}

pub fn collect_symbols_batch<'tcx, C, MakeCollector, Visit, Finish>(
    unit: CompileUnit<'tcx>,
    make_collector: MakeCollector,
    visit: Visit,
    finish: Finish,
) -> (CollectedSymbols, Duration, Duration)
where
    MakeCollector: FnOnce(CompileUnit<'tcx>) -> C,
    Visit: FnOnce(&mut C, HirNode<'tcx>),
    Finish: FnOnce(C) -> CollectedSymbols,
{
    let collect_start = Instant::now();
    let root = unit.file_start_hir_id().unwrap();
    let node = unit.hir_node(root);
    let mut collector = make_collector(unit);

    let visit_start = Instant::now();
    visit(&mut collector, node);
    let visit_time = visit_start.elapsed();

    let collected = finish(collector);
    let total_time = collect_start.elapsed();

    (collected, total_time, visit_time)
}

pub fn apply_collected_symbols<'tcx>(
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

pub fn apply_symbol_batch<'tcx>(
    unit: CompileUnit<'tcx>,
    globals: &'tcx Scope<'tcx>,
    batch: (CollectedSymbols, Duration, Duration),
) -> (CollectionResult, Duration, Duration) {
    let (collected, total_time, visit_time) = batch;
    apply_collected_symbols(unit, globals, &collected);

    (collected.result, total_time, visit_time)
}
