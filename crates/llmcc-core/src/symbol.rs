use parking_lot::RwLock;
use std::collections::HashMap;

use crate::graph_builder::BlockId;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirIdent};
use crate::trie::SymbolTrie;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_SYMBOL_ID: AtomicU32 = AtomicU32::new(1);

/// Reset the global symbol identifier counter back to 1.
///
/// This is primarily intended for deterministic test harnesses that execute many
/// isolated compilations within the same process.
pub fn reset_symbol_id_counter() {
    NEXT_SYMBOL_ID.store(1, Ordering::SeqCst);
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SymId(pub u32);

impl std::fmt::Display for SymId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SymbolKind {
    Unknown,
    Module,
    Struct,
    Enum,
    Function,
    Macro,
    Variable,
    Field,
    Const,
    Static,
    Trait,
    Impl,
    EnumVariant,
    InferredType,
}

/// Canonical representation of an item bound in a scope (functions, variables, types, etc.).
#[derive(Debug)]
pub struct Scope<'tcx> {
    /// Trie for fast symbol lookup by suffix
    trie: RwLock<SymbolTrie<'tcx>>,
    /// The HIR node that owns this scope
    owner: HirId,
    /// The symbol that introduced this scope, if any
    symbol: RwLock<Option<&'tcx Symbol>>,
}

impl<'tcx> Scope<'tcx> {
    pub fn new(owner: HirId) -> Self {
        Self {
            trie: RwLock::new(SymbolTrie::default()),
            owner,
            symbol: RwLock::new(None),
        }
    }

    pub fn owner(&self) -> HirId {
        self.owner
    }

    pub fn symbol(&self) -> Option<&'tcx Symbol> {
        *self.symbol.read()
    }

    pub fn set_symbol(&self, symbol: Option<&'tcx Symbol>) {
        *self.symbol.write() = symbol;
    }

    pub fn insert(&self, symbol: &'tcx Symbol, interner: &InternPool) -> SymId {
        let sym_id = symbol.id;
        self.trie.write().insert_symbol(symbol, interner);
        sym_id
    }

    pub fn get_id(&self, key: InternedStr) -> Option<SymId> {
        let hits = self.trie.read().lookup_symbol_suffix(&[key], None, None);
        hits.first().map(|symbol| symbol.id)
    }

    pub fn lookup_suffix_once(
        &self,
        suffix: &[InternedStr],
        kind_filter: Option<SymbolKind>,
        unit_filter: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        self.lookup_suffix_symbols(suffix, kind_filter, unit_filter)
            .into_iter()
            .next()
    }

    pub fn lookup_suffix_symbols(
        &self,
        suffix: &[InternedStr],
        kind_filter: Option<SymbolKind>,
        unit_filter: Option<usize>,
    ) -> Vec<&'tcx Symbol> {
        self.trie
            .read()
            .lookup_symbol_suffix(suffix, kind_filter, unit_filter)
    }

    pub fn format_compact(&self) -> String {
        let count = self.trie.read().total_symbols();
        format!("{}/{}", self.owner, count)
    }

    pub fn all_symbols(&self) -> Vec<&'tcx Symbol> {
        self.trie.read().symbols()
    }
}

#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    arena: &'tcx Arena<'tcx>,
    interner: &'tcx InternPool,
    stack: Vec<&'tcx Scope<'tcx>>,
    symbol_map: &'tcx RwLock<HashMap<SymId, &'tcx Symbol>>,
}

impl<'tcx> ScopeStack<'tcx> {
    pub fn new(
        arena: &'tcx Arena<'tcx>,
        interner: &'tcx InternPool,
        symbol_map: &'tcx RwLock<HashMap<SymId, &'tcx Symbol>>,
    ) -> Self {
        Self {
            arena,
            interner,
            stack: Vec::new(),
            symbol_map,
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn push(&mut self, scope: &'tcx Scope<'tcx>) {
        self.push_with_symbol(scope, None);
    }

    pub fn push_with_symbol(&mut self, scope: &'tcx Scope<'tcx>, symbol: Option<&'tcx Symbol>) {
        scope.set_symbol(symbol);
        self.stack.push(scope);
    }

    pub fn pop(&mut self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.pop()
    }

    pub fn pop_until(&mut self, depth: usize) {
        while self.depth() > depth {
            self.pop();
        }
    }

    pub fn top(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.last().copied()
    }

    pub fn scoped_symbol(&self) -> Option<&'tcx Symbol> {
        self.stack.iter().rev().find_map(|scope| scope.symbol())
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &'tcx Scope<'tcx>> + '_ {
        self.stack.iter().copied()
    }

    pub fn lookup_scoped_suffix_once(&self, suffix: &[InternedStr]) -> Option<&'tcx Symbol> {
        self.find_scoped_suffix_with_filters(suffix, None, None)
    }

    pub fn find_scoped_suffix_with_filters(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        for scope in self.iter().rev() {
            let symbols = scope.lookup_suffix_symbols(suffix, kind, file);
            if let Some(symbol) = select_symbol(symbols, kind, file) {
                return Some(symbol);
            }
        }
        None
    }

    fn scope_for_insertion(&mut self, global: bool) -> Result<&'tcx Scope<'tcx>, &'static str> {
        if global {
            self.stack.first().copied().ok_or("no global scope exists")
        } else {
            self.stack
                .last()
                .copied()
                .ok_or("no active scope available")
        }
    }

    pub fn insert_symbol(
        &mut self,
        symbol: &'tcx Symbol,
        global: bool,
    ) -> Result<SymId, &'static str> {
        let scope = self.scope_for_insertion(global)?;
        Ok(scope.insert(symbol, self.interner))
    }

    pub fn find_symbol_id(&self, name: &str) -> Option<SymId> {
        let key = self.interner.intern(name);
        self.iter().rev().find_map(|scope| scope.get_id(key))
    }

    fn find_symbol_local_by_key(&self, key: InternedStr) -> Option<&'tcx Symbol> {
        let scopes = if self.stack.len() > 1 {
            &self.stack[1..]
        } else {
            &self.stack[..]
        };

        scopes.iter().rev().find_map(|scope| {
            scope
                .lookup_suffix_symbols(&[key], None, None)
                .into_iter()
                .next()
        })
    }

    pub fn find_symbol_local(&self, name: &str) -> Option<&'tcx Symbol> {
        let key = self.interner.intern(name);
        self.find_symbol_local_by_key(key)
    }

    pub fn find_global_suffix_vec(&self, suffix: &[InternedStr]) -> Vec<&'tcx Symbol> {
        self.stack
            .first()
            .map(|scope| scope.lookup_suffix_symbols(suffix, None, None))
            .unwrap_or_default()
    }

    pub fn find_global_suffix(&self, suffix: &[InternedStr]) -> Option<&'tcx Symbol> {
        self.find_global_suffix_with_filters(suffix, None, None)
    }

    pub fn find_global_suffix_with_filters(
        &self,
        suffix: &[InternedStr],
        kind: Option<SymbolKind>,
        file: Option<usize>,
    ) -> Option<&'tcx Symbol> {
        let symbols = self.find_global_suffix_vec(suffix);
        select_symbol(symbols, kind, file)
    }

    /// Find a global symbol that matches the provided suffix but restrict results to a unit index.
    pub fn find_global_suffix_in_unit(
        &self,
        suffix: &[InternedStr],
        unit_index: usize,
    ) -> Option<&'tcx Symbol> {
        self.find_global_suffix_with_filters(suffix, None, Some(unit_index))
    }

    pub fn insert_with<F>(
        &mut self,
        owner: HirId,
        ident: &HirIdent<'tcx>,
        global: bool,
        init: F,
    ) -> &'tcx Symbol
    where
        F: FnOnce(&'tcx Symbol),
    {
        let key = self.interner.intern(&ident.name);

        let symbol = self.alloc_symbol(owner, ident, key);
        init(symbol);

        self.insert_symbol(symbol, false)
            .expect("failed to insert symbol into scope");
        if global {
            self.insert_symbol(symbol, true)
                .expect("failed to insert symbol into global scope");
        }

        symbol
    }

    fn alloc_symbol(&self, owner: HirId, ident: &HirIdent<'tcx>, key: InternedStr) -> &'tcx Symbol {
        let symbol = Symbol::new(owner, ident.name.clone(), key);
        let symbol = self.arena.alloc(symbol);
        self.symbol_map.write().insert(symbol.id, symbol);
        symbol
    }
}

/// Canonical representation of an item bound in a scope (functions, variables, types, etc.).
#[derive(Debug)]
pub struct Symbol {
    /// Monotonic identifier assigned when the symbol is created.
    pub id: SymId,
    /// Owning HIR node that introduces the symbol (e.g. function, struct, module).
    pub owner: RwLock<HirId>,
    /// Unqualified name exactly as written in source.
    pub name: String,
    /// Interned key for `name`, used for fast lookup.
    pub name_key: InternedStr,
    /// Fully qualified name cached as a string (updated as scopes are resolved).
    pub fqn_name: RwLock<String>,
    /// Interned key for the fully qualified name.
    pub fqn_key: RwLock<InternedStr>,
    /// All symbols that this symbols depends on, most general relation, could be
    /// another relation, like field_of, type_of, called_by, calls etc.
    /// we dont do very clear sepration becase we want llm models to do that, we
    /// only need to tell models some symbols having depends relations
    pub depends: RwLock<Vec<SymId>>,
    pub depended: RwLock<Vec<SymId>>,
    /// Optional backing type for this symbol (e.g. variable type, alias target).
    pub type_of: RwLock<Option<SymId>>,
    pub kind: RwLock<SymbolKind>,
    /// Which compile unit this symbol defined
    pub unit_index: RwLock<Option<usize>>,
    /// Optional block id associated with this symbol (for graph building)
    pub block_id: RwLock<Option<BlockId>>,
    /// Whether the symbol is globally visible/exported.
    pub is_global: RwLock<bool>,
}

impl Clone for Symbol {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            owner: RwLock::new(*self.owner.read()),
            name: self.name.clone(),
            name_key: self.name_key,
            fqn_name: RwLock::new(self.fqn_name.read().clone()),
            fqn_key: RwLock::new(*self.fqn_key.read()),
            depends: RwLock::new(self.depends.read().clone()),
            depended: RwLock::new(self.depended.read().clone()),
            type_of: RwLock::new(*self.type_of.read()),
            kind: RwLock::new(*self.kind.read()),
            unit_index: RwLock::new(*self.unit_index.read()),
            block_id: RwLock::new(*self.block_id.read()),
            is_global: RwLock::new(*self.is_global.read()),
        }
    }
}

impl Symbol {
    pub fn new(owner: HirId, name: String, name_key: InternedStr) -> Self {
        let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::SeqCst);
        let sym_id = SymId(id);

        let fqn_key = name_key;

        Self {
            id: sym_id,
            owner: RwLock::new(owner),
            name: name.clone(),
            name_key,
            fqn_name: RwLock::new(name),
            fqn_key: RwLock::new(fqn_key),
            depends: RwLock::new(Vec::new()),
            depended: RwLock::new(Vec::new()),
            type_of: RwLock::new(None),
            kind: RwLock::new(SymbolKind::Unknown),
            unit_index: RwLock::new(None),
            block_id: RwLock::new(None),
            is_global: RwLock::new(false),
        }
    }

    pub fn owner(&self) -> HirId {
        *self.owner.read()
    }

    pub fn set_owner(&self, owner: HirId) {
        *self.owner.write() = owner;
    }

    pub fn format_compact(&self) -> String {
        let owner = *self.owner.read();
        format!("{}->{} \"{}\"", self.id, owner, self.name)
    }

    pub fn set_fqn(&self, fqn: String, interner: &InternPool) {
        let key = interner.intern(&fqn);
        *self.fqn_name.write() = fqn;
        *self.fqn_key.write() = key;
    }

    pub fn kind(&self) -> SymbolKind {
        *self.kind.read()
    }

    pub fn set_kind(&self, kind: SymbolKind) {
        *self.kind.write() = kind;
    }

    pub fn type_of(&self) -> Option<SymId> {
        *self.type_of.read()
    }

    pub fn set_type_of(&self, ty: Option<SymId>) {
        *self.type_of.write() = ty;
    }

    pub fn unit_index(&self) -> Option<usize> {
        *self.unit_index.read()
    }

    pub fn set_unit_index(&self, file: usize) {
        let mut unit_index = self.unit_index.write();
        if unit_index.is_none() {
            *unit_index = Some(file);
        }
    }

    pub fn is_global(&self) -> bool {
        *self.is_global.read()
    }

    pub fn set_is_global(&self, value: bool) {
        *self.is_global.write() = value;
    }

    pub fn add_depends_on(&self, sym_id: SymId) {
        if sym_id == self.id {
            return;
        }
        let mut deps = self.depends.write();
        if deps.contains(&sym_id) {
            return;
        }
        deps.push(sym_id);
    }

    pub fn add_depended_by(&self, sym_id: SymId) {
        if sym_id == self.id {
            return;
        }
        let mut deps = self.depended.write();
        if deps.contains(&sym_id) {
            return;
        }
        deps.push(sym_id);
    }

    pub fn add_dependency(&self, other: &Symbol) {
        if self.id == other.id {
            return;
        }
        if self.kind() == other.kind() {
            let self_fqn = self.fqn_name.read().clone();
            let other_fqn = other.fqn_name.read().clone();
            if !self_fqn.is_empty() && self_fqn == other_fqn {
                return;
            }
        }
        if other.depends.read().contains(&self.id) {
            return;
        }
        self.add_depends_on(other.id);
        other.add_depended_by(self.id);
    }

    pub fn block_id(&self) -> Option<BlockId> {
        *self.block_id.read()
    }

    pub fn set_block_id(&self, block_id: Option<BlockId>) {
        *self.block_id.write() = block_id;
    }
}

fn select_symbol(
    candidates: Vec<&Symbol>,
    kind: Option<SymbolKind>,
    file: Option<usize>,
) -> Option<&Symbol> {
    if candidates.is_empty() {
        return None;
    }

    if let Some(kind) = kind {
        let matches: Vec<&Symbol> = candidates
            .iter()
            .copied()
            .filter(|symbol| symbol.kind() == kind)
            .collect();

        if let Some(file) = file {
            if let Some(symbol) = matches
                .iter()
                .copied()
                .find(|symbol| symbol.unit_index() == Some(file))
            {
                return Some(symbol);
            }
        }

        if !matches.is_empty() {
            return Some(matches[0]);
        }
    }

    if let Some(file) = file {
        if let Some(symbol) = candidates
            .iter()
            .copied()
            .find(|candidate| candidate.unit_index() == Some(file))
        {
            return Some(symbol);
        }
    }

    candidates.into_iter().next()
}

#[derive(Debug, Clone)]
pub struct SymbolKindMap<T> {
    inner: HashMap<SymbolKind, HashMap<String, T>>,
}

impl<T> SymbolKindMap<T> {
    pub fn new() -> Self {
        Self {
            inner: HashMap::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.inner.values().map(HashMap::len).sum()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.values().all(HashMap::is_empty)
    }

    pub fn kind_len(&self, kind: SymbolKind) -> usize {
        self.inner.get(&kind).map(HashMap::len).unwrap_or(0)
    }

    pub fn contains(&self, kind: SymbolKind, name: &str) -> bool {
        self.inner
            .get(&kind)
            .map(|bucket| bucket.contains_key(name))
            .unwrap_or(false)
    }

    pub fn get(&self, kind: SymbolKind, name: &str) -> Option<&T> {
        self.inner.get(&kind).and_then(|bucket| bucket.get(name))
    }

    pub fn get_mut(&mut self, kind: SymbolKind, name: &str) -> Option<&mut T> {
        self.inner
            .get_mut(&kind)
            .and_then(|bucket| bucket.get_mut(name))
    }

    pub fn kind_map(&self, kind: SymbolKind) -> Option<&HashMap<String, T>> {
        self.inner.get(&kind)
    }

    pub fn kind_map_mut(&mut self, kind: SymbolKind) -> Option<&mut HashMap<String, T>> {
        self.inner.get_mut(&kind)
    }

    pub fn ensure_kind(&mut self, kind: SymbolKind) -> &mut HashMap<String, T> {
        self.inner.entry(kind).or_default()
    }

    pub fn insert(&mut self, kind: SymbolKind, name: impl Into<String>, value: T) -> Option<T> {
        self.ensure_kind(kind).insert(name.into(), value)
    }

    pub fn remove(&mut self, kind: SymbolKind, name: &str) -> Option<T> {
        let bucket = self.inner.get_mut(&kind)?;
        let removed = bucket.remove(name);
        if bucket.is_empty() {
            self.inner.remove(&kind);
        }
        removed
    }

    pub fn clear_kind(&mut self, kind: SymbolKind) -> Option<HashMap<String, T>> {
        self.inner.remove(&kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = SymbolKind> + '_ {
        self.inner.keys().copied()
    }

    pub fn inner(&self) -> &HashMap<SymbolKind, HashMap<String, T>> {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut HashMap<SymbolKind, HashMap<String, T>> {
        &mut self.inner
    }

    pub fn into_inner(self) -> HashMap<SymbolKind, HashMap<String, T>> {
        self.inner
    }
}

impl<T> Default for SymbolKindMap<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> IntoIterator for SymbolKindMap<T> {
    type Item = (SymbolKind, HashMap<String, T>);
    type IntoIter = std::collections::hash_map::IntoIter<SymbolKind, HashMap<String, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a SymbolKindMap<T> {
    type Item = (&'a SymbolKind, &'a HashMap<String, T>);
    type IntoIter = std::collections::hash_map::Iter<'a, SymbolKind, HashMap<String, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut SymbolKindMap<T> {
    type Item = (&'a SymbolKind, &'a mut HashMap<String, T>);
    type IntoIter = std::collections::hash_map::IterMut<'a, SymbolKind, HashMap<String, T>>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.iter_mut()
    }
}
