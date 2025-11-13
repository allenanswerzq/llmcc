use std::collections::HashMap;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice::{Iter, IterMut};
use std::time::{Duration, Instant};

use crate::type_expr::TypeExprResolver;
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
    /// Optional index pointing to the symbol that represents this symbol's type.
    pub type_of: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct ScopeSpec {
    /// Owning HIR node for the scope; None represents the root scope.
    pub owner: Option<HirId>,
    /// Index of the symbol associated with this scope, if any.
    pub owner_symbol: Option<usize>,
    /// Symbols captured directly within this scope.
    pub symbols: Vec<usize>,
    /// String paths that should map to symbols already declared elsewhere.
    pub aliases: Vec<ScopeAliasSpec>,
}

#[derive(Debug, Clone)]
pub struct ScopeAliasSpec {
    /// Segments that make up the alias path (e.g. `["Self"]`).
    pub parts: Vec<String>,
    /// Index into the collected symbol list for the aliased target.
    pub symbol_idx: usize,
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
    /// String aliases that should resolve to existing symbol indices.
    aliases: Vec<ScopeAliasSpec>,
}

impl ScopeInfo {
    fn new(owner: Option<HirId>) -> Self {
        Self {
            owner,
            owner_symbol: None,
            symbols: Vec::new(),
            locals: HashMap::new(),
            locals_by_kind: SymbolKindMap::new(),
            aliases: Vec::new(),
        }
    }

    fn record_symbol(&mut self, name: &str, symbol_idx: usize, kind: SymbolKind) {
        self.locals.insert(name.to_string(), symbol_idx);
        self.symbols.push(symbol_idx);
        self.locals_by_kind
            .ensure_kind(kind)
            .entry(name.to_string())
            .or_default()
            .push(symbol_idx);
    }

    fn add_alias(&mut self, name: &str, symbol_idx: usize, kind: SymbolKind) {
        self.locals.insert(name.to_string(), symbol_idx);
        let entries = self
            .locals_by_kind
            .ensure_kind(kind)
            .entry(name.to_string())
            .or_default();
        if !entries.contains(&symbol_idx) {
            entries.push(symbol_idx);
        }

        if !self.aliases.iter().any(|alias| {
            alias.symbol_idx == symbol_idx && alias.parts.len() == 1 && alias.parts[0] == name
        }) {
            self.aliases.push(ScopeAliasSpec {
                parts: vec![name.to_string()],
                symbol_idx,
            });
        }
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

    pub fn insert_field_symbol(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        kind: SymbolKind,
    ) -> Option<(usize, String)> {
        let is_global = self
            .parent_symbol()
            .map(|symbol| symbol.is_global)
            .unwrap_or(false);
        let ident = self.ident_from_field(node, field_id)?;
        Some(self.insert_symbol(node.hir_id(), &ident.name, kind, is_global))
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
        name: &str,
        kind: SymbolKind,
        is_global: bool,
    ) -> (usize, String) {
        if let Some(_idx) = self.lookup_from_scopes_with(name, kind) {
            tracing::warn!(
                "symbol '{}' of kind {:?} already exists in scope, source code probaly has duplicate declaration",
                name,
                kind
            );
        }

        let idx = self.symbols.len();
        let fqn = self.scoped_qualified_name(name);
        self.symbols.push(SymbolSpec {
            owner,
            name: name.to_owned(),
            fqn: fqn.clone(),
            kind,
            unit_index: self.unit_index(),
            is_global,
            type_of: None,
        });

        let current_scope = self.current_scope_index();
        self.scope_infos[current_scope].record_symbol(name, idx, kind);

        if is_global {
            self.scope_infos[0].record_symbol(name, idx, kind);
        }

        (idx, fqn)
    }

    pub fn register_symbol_globally(&mut self, name: &str, symbol_idx: usize, kind: SymbolKind) {
        self.scope_infos[0].record_symbol(name, symbol_idx, kind);
    }

    pub fn upsert_symbol(
        &mut self,
        owner: HirId,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
    ) -> (usize, String) {
        if let Some(idx) = self.lookup_from_scopes_with(name, kind) {
            if let Some(existing) = self.symbols.get_mut(idx) {
                existing.owner = owner;
                if is_global {
                    existing.is_global = true;
                }
                return (idx, existing.fqn.clone());
            }
        }

        self.insert_symbol(owner, name, kind, is_global)
    }

    pub fn upsert_symbol_with_fqn(
        &mut self,
        owner: HirId,
        name: &str,
        kind: SymbolKind,
        is_global: bool,
        fqn: &str,
    ) -> (usize, String) {
        if let Some((idx, _)) = self
            .symbols
            .iter()
            .enumerate()
            .find(|(_, spec)| spec.fqn == fqn && spec.kind == kind)
        {
            if let Some(existing) = self.symbols.get_mut(idx) {
                existing.owner = owner;
                if is_global {
                    existing.is_global = true;
                }
            }
            return (idx, fqn.to_string());
        }

        let (idx, _) = self.insert_symbol(owner, name, kind, is_global);
        if let Some(symbol) = self.symbols.get_mut(idx) {
            symbol.fqn = fqn.to_string();
        }
        (idx, fqn.to_string())
    }

    pub fn set_symbol_type_of(&mut self, symbol_idx: usize, type_idx: usize) {
        if let Some(symbol) = self.symbols.get_mut(symbol_idx) {
            symbol.type_of = Some(type_idx);
        }
    }

    pub fn add_scope_alias(&mut self, name: &str, symbol_idx: usize, kind: SymbolKind) {
        let scope_idx = self.current_scope_index();
        self.scope_infos[scope_idx].add_alias(name, symbol_idx, kind);
    }

    pub fn lookup_type_expr_symbol(
        &mut self,
        owner: HirId,
        expr: &TypeExpr,
        kind: SymbolKind,
        is_global: bool,
    ) -> Option<usize> {
        self.lookup_or_insert_expr_symbol(owner, expr, kind, is_global, false)
    }

    pub fn upsert_expr_symbol(
        &mut self,
        owner: HirId,
        expr: &TypeExpr,
        kind: SymbolKind,
        is_global: bool,
    ) -> Option<usize> {
        self.lookup_or_insert_expr_symbol(owner, expr, kind, is_global, true)
    }

    /// Resolve the canonical symbol for a language-agnostic `TypeExpr`, optionally inserting a
    /// placeholder.
    ///
    /// Front-ends normalize each language's type syntax into `TypeExpr` variants (paths,
    /// references, tuples, …). Path expressions try progressively broader matches (qualified
    /// path, canonical FQN, terminal identifier) before optionally creating a synthetic symbol
    /// when `upsert` is true. Reference and tuple expressions strip to their underlying path so
    /// constructs like Rust's `impl &Foo` or Python's `tuple[T]` reuse the same lookup rules as
    /// plain identifiers.
    fn lookup_or_insert_expr_symbol(
        &mut self,
        owner: HirId,
        expr: &TypeExpr,
        kind: SymbolKind,
        is_global: bool,
        upsert: bool,
    ) -> Option<usize> {
        let mut resolver = TypeExprResolver::new(self, owner, kind, is_global, upsert);
        resolver.resolve(expr)
    }

    pub fn lookup_from_scopes_with(&self, name: &str, kind: SymbolKind) -> Option<usize> {
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

    pub fn lookup_from_scopes_with_parts(
        &self,
        parts: &[&str],
        kind: SymbolKind,
        start_depth: Option<usize>,
    ) -> Option<usize> {
        let parts: Vec<_> = parts
            .iter()
            .copied()
            .filter(|part| !part.is_empty())
            .collect();
        if parts.is_empty() {
            return None;
        }

        let mut scope_cursor = start_depth.unwrap_or(self.scope_stack.len());
        scope_cursor = scope_cursor.min(self.scope_stack.len());
        if scope_cursor == 0 {
            return None;
        }

        let mut resolved_idx = None;
        let mut restrict_kind = true;

        for part in parts.iter().rev() {
            let mut found: Option<(usize, usize)> = None;

            while scope_cursor > 0 {
                scope_cursor -= 1;
                let scope_idx = self.scope_stack[scope_cursor];
                let scope = &self.scope_infos[scope_idx];

                if restrict_kind {
                    if let Some(kind_bucket) = scope.locals_by_kind.kind_map(kind) {
                        if let Some(indices) = kind_bucket.get(*part) {
                            if let Some(&idx) = indices.last() {
                                found = Some((scope_cursor, idx));
                                break;
                            }
                        }
                    }
                } else if let Some(&idx) = scope.locals.get(*part) {
                    found = Some((scope_cursor, idx));
                    break;
                }
            }

            let (matched_scope, symbol_idx) = found?;
            if resolved_idx.is_none() {
                resolved_idx = Some(symbol_idx);
            }
            scope_cursor = matched_scope;
            restrict_kind = false;
        }

        resolved_idx
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
                aliases: info.aliases,
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

    for (spec, symbol) in collected.symbols.iter().zip(created_symbols.iter()) {
        if let Some(type_idx) = spec.type_of {
            if let Some(target_symbol) = created_symbols.get(type_idx) {
                symbol.set_type_of(Some(target_symbol.id));
            }
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

            if let Some(owner_symbol) = created_symbols.get(sym_idx) {
                if matches!(
                    owner_symbol.kind(),
                    SymbolKind::Struct | SymbolKind::Enum | SymbolKind::Trait
                ) {
                    for &member_idx in &scope_spec.symbols {
                        if member_idx == sym_idx {
                            continue;
                        }
                        if let Some(member_symbol) = created_symbols.get(member_idx) {
                            if member_symbol.kind() == SymbolKind::Function {
                                owner_symbol.add_member(member_symbol.name_key, member_symbol.id);
                            }
                        }
                    }
                }
            }
        }

        for &sym_idx in &scope_spec.symbols {
            if let Some(symbol) = created_symbols.get(sym_idx) {
                target_scope.insert(symbol, interner);
            }
        }

        for alias in &scope_spec.aliases {
            if let Some(symbol) = created_symbols.get(alias.symbol_idx) {
                target_scope.insert_alias(&alias.parts, interner, symbol);
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
