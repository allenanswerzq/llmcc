use std::collections::HashMap;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice::{Iter, IterMut};
use std::time::{Duration, Instant};

use llmcc_core::context::CompileUnit;
use llmcc_core::ir::{HirId, HirIdent, HirNode};
use llmcc_core::symbol::{Scope, Symbol, SymbolKind, SymbolKindMap};
use llmcc_descriptor::{
    CallDescriptor, ClassDescriptor, EnumDescriptor, FunctionDescriptor, ImplDescriptor,
    ImportDescriptor, StructDescriptor, TypeExpr, VariableDescriptor,
};

#[derive(Debug, Clone)]
pub struct SymbolSpec {
    /// HIR node that declared the symbol.
    pub owner: HirId,
    /// Unqualified symbol name as it appears in source.
    pub name: String,
    /// Fully-qualified path for the symbol.
    pub fqn: String,
    /// Kind of symbol (function, struct, trait, etc.).
    pub kind: SymbolKind,
    /// Index of the compile unit that produced this symbol.
    pub unit_index: usize,
    /// Whether the symbol should be visible from the global scope.
    pub is_global: bool,
}

#[derive(Debug, Clone)]
pub struct ScopeSpec {
    /// Owning HIR node for the scope; None represents the root scope.
    pub owner: Option<HirId>,
    /// Index of the symbol associated with this scope, if any.
    pub owner_symbol: Option<usize>,
    /// Symbols captured directly within this scope.
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
pub type ImplCollection = DescriptorCollection<ImplDescriptor>;
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
    /// HIR owner for this scope.
    owner: Option<HirId>,
    /// Index into `symbols` for the scope's representative symbol.
    owner_symbol: Option<usize>,
    /// Indices of symbols declared inside this scope.
    symbols: Vec<usize>,
    /// Map of local identifiers to their symbol indices in this scope.
    locals: HashMap<String, usize>,
    /// Cache of symbol indices grouped by kind for faster lookups.
    locals_by_kind: SymbolKindMap<Vec<usize>>,
}

impl ScopeInfo {
    fn new(owner: Option<HirId>) -> Self {
        Self {
            owner,
            owner_symbol: None,
            symbols: Vec::new(),
            locals: HashMap::new(),
            locals_by_kind: SymbolKindMap::new(),
        }
    }

    fn record_symbol(&mut self, name: &str, symbol_idx: usize, kind: SymbolKind) {
        self.locals.insert(name.to_string(), symbol_idx);
        self.symbols.push(symbol_idx);
        self.locals_by_kind
            .ensure_kind(kind)
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(symbol_idx);
    }
}

#[derive(Debug)]
pub struct CollectorCore<'tcx> {
    /// Source compile unit currently being processed.
    unit: CompileUnit<'tcx>,
    /// Scope bookkeeping indexed by scope identifier.
    scope_infos: Vec<ScopeInfo>,
    /// Lookup table from HIR owner to scope index.
    scope_lookup: HashMap<HirId, usize>,
    /// Stack of active scope indices while traversing the tree.
    scope_stack: Vec<usize>,
    /// All symbols discovered so far, in creation order.
    symbols: Vec<SymbolSpec>,
}

impl<'tcx> CollectorCore<'tcx> {
    pub fn new(unit: CompileUnit<'tcx>) -> Self {
        Self {
            unit,
            scope_infos: vec![ScopeInfo::new(None)],
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
        self.scope_infos.push(ScopeInfo::new(Some(owner)));
        self.scope_lookup.insert(owner, idx);
        idx
    }

    pub fn set_scope_owner_symbol(&mut self, scope_idx: usize, owner_symbol: Option<usize>) {
        self.scope_infos[scope_idx].owner_symbol = owner_symbol;
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
            if let Some(sym_idx) = self.scope_infos[scope_idx].owner_symbol {
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
            if let Some(sym_idx) = info.owner_symbol {
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
            if let Some(sym_idx) = self.scope_infos[scope_idx].owner_symbol {
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

    pub fn insert_symbol(
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
        self.scope_infos[current_scope].record_symbol(&name, idx, kind);

        if is_global {
            self.scope_infos[0].record_symbol(&name, idx, kind);
        }

        idx
    }

    pub fn upsert_symbol(
        &mut self,
        owner: HirId,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
    ) -> (usize, String) {
        if let Some(existing_idx) = self.find_symbol_with(name, kind) {
            let symbol = self.symbols.get_mut(existing_idx).unwrap();
            symbol.is_global |= is_global;
            let fqn = self.symbols[existing_idx].fqn.clone();
            return (existing_idx, fqn);
        } else {
            let fqn = self.scoped_qualified_name(name);
            let idx = self.insert_symbol(owner, name.to_string(), fqn.clone(), kind, is_global);
            (idx, fqn)
        }
    }

    pub fn upsert_expr_symbol(
        &mut self,
        owner: HirId,
        expr: &TypeExpr,
        kind: SymbolKind,
        is_global: bool,
    ) -> Option<usize> {
        match expr {
            TypeExpr::Path { qualifier, .. } => {
                let parts: Vec<String> = qualifier
                    .segments()
                    .iter()
                    .filter(|part| !part.is_empty())
                    .cloned()
                    .collect();

                if parts.is_empty() {
                    return None;
                }

                let mut normalized = parts.clone();
                while matches!(
                    normalized.first().map(String::as_str),
                    Some("crate" | "self" | "super")
                ) {
                    normalized.remove(0);
                }

                if normalized.is_empty() {
                    normalized = parts;
                }

                let name = normalized.last().cloned().unwrap();

                if normalized.len() == 1 {
                    let (idx, _) = self.upsert_symbol(owner, &name, kind, is_global);
                    Some(idx)
                } else {
                    let candidate_fqn = normalized.join("::");

                    if let Some(idx) = self.find_symbol_by_fqn(&candidate_fqn, kind) {
                        return Some(idx);
                    }

                    if let Some(idx) = self.find_symbol_with(&name, kind) {
                        return Some(idx);
                    }

                    let idx = self.insert_symbol(owner, name, candidate_fqn, kind, is_global);
                    Some(idx)
                }
            }
            TypeExpr::Reference { inner, .. } => {
                self.upsert_expr_symbol(owner, inner, kind, is_global)
            }
            TypeExpr::Tuple(items) => {
                for item in items {
                    if let Some(idx) = self.upsert_expr_symbol(owner, item, kind, is_global) {
                        return Some(idx);
                    }
                }
                None
            }
            _ => None,
        }
    }

    pub fn find_symbol_with(&self, name: &str, kind: SymbolKind) -> Option<usize> {
        for &scope_idx in self.scope_stack.iter().rev() {
            let scope = &self.scope_infos[scope_idx];
            if let Some(kind_bucket) = scope.locals_by_kind.kind_map(kind) {
                if let Some(indices) = kind_bucket.get(name) {
                    if let Some(&idx) = indices.last() {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    pub fn find_symbol_by_fqn(&self, fqn: &str, kind: SymbolKind) -> Option<usize> {
        self.symbols
            .iter()
            .enumerate()
            .rev()
            .find(|(_, symbol)| symbol.kind == kind && symbol.fqn == fqn)
            .map(|(idx, _)| idx)
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
                owner_symbol: info.owner_symbol,
                symbols: info.symbols,
            })
            .collect();

        CollectedSymbols {
            result,
            symbols: self.symbols,
            scopes,
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

        if let Some(sym_idx) = scope_spec.owner_symbol {
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
