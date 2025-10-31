use std::collections::HashMap;
use std::sync::RwLock;

use crate::graph_builder::BlockId;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirIdent};
use crate::trie::SymbolTrie;
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT_SYMBOL_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SymId(pub u32);

impl std::fmt::Display for SymId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Unknown,
    Module,
    Struct,
    Enum,
    Function,
    Variable,
    Const,
    Static,
    Trait,
    Impl,
    EnumVariant,
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
        *self.symbol.read().unwrap()
    }

    pub fn set_symbol(&self, symbol: Option<&'tcx Symbol>) {
        *self.symbol.write().unwrap() = symbol;
    }

    pub fn insert(&self, symbol: &'tcx Symbol, interner: &InternPool) -> SymId {
        let sym_id = symbol.id;
        self.trie.write().unwrap().insert_symbol(symbol, interner);
        sym_id
    }

    pub fn get_id(&self, key: InternedStr) -> Option<SymId> {
        let hits = self.trie.read().unwrap().lookup_symbol_suffix(&[key]);
        hits.first().map(|symbol| symbol.id)
    }

    pub fn lookup_suffix_once(&self, suffix: &[InternedStr]) -> Option<&'tcx Symbol> {
        self.trie
            .read()
            .unwrap()
            .lookup_symbol_suffix(suffix)
            .into_iter()
            .next()
    }

    pub fn lookup_suffix_symbols(&self, suffix: &[InternedStr]) -> Vec<&'tcx Symbol> {
        self.trie.read().unwrap().lookup_symbol_suffix(suffix)
    }

    pub fn format_compact(&self) -> String {
        let count = self.trie.read().unwrap().total_symbols();
        format!("{}/{}", self.owner, count)
    }

    pub fn all_symbols(&self) -> Vec<&'tcx Symbol> {
        self.trie.read().unwrap().symbols()
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
            let symbols = scope.trie.read().unwrap().lookup_symbol_suffix(suffix);
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
                .trie
                .read()
                .unwrap()
                .lookup_symbol_suffix(&[key])
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
            .map(|scope| scope.trie.read().unwrap().lookup_symbol_suffix(suffix))
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
        self.symbol_map.write().unwrap().insert(symbol.id, symbol);
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
    pub kind: RwLock<SymbolKind>,
    /// Which compile unit this symbol defined
    pub unit_index: RwLock<Option<usize>>,
    /// Optional block id associated with this symbol (for graph building)
    pub block_id: RwLock<Option<BlockId>>,
}

impl Clone for Symbol {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            owner: RwLock::new(*self.owner.read().unwrap()),
            name: self.name.clone(),
            name_key: self.name_key,
            fqn_name: RwLock::new(self.fqn_name.read().unwrap().clone()),
            fqn_key: RwLock::new(*self.fqn_key.read().unwrap()),
            depends: RwLock::new(self.depends.read().unwrap().clone()),
            depended: RwLock::new(self.depended.read().unwrap().clone()),
            kind: RwLock::new(*self.kind.read().unwrap()),
            unit_index: RwLock::new(*self.unit_index.read().unwrap()),
            block_id: RwLock::new(*self.block_id.read().unwrap()),
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
            kind: RwLock::new(SymbolKind::Unknown),
            unit_index: RwLock::new(None),
            block_id: RwLock::new(None),
        }
    }

    pub fn owner(&self) -> HirId {
        *self.owner.read().unwrap()
    }

    pub fn set_owner(&self, owner: HirId) {
        *self.owner.write().unwrap() = owner;
    }

    pub fn format_compact(&self) -> String {
        let owner = *self.owner.read().unwrap();
        format!("{}->{} \"{}\"", self.id, owner, self.name)
    }

    pub fn set_fqn(&self, fqn: String, interner: &InternPool) {
        let key = interner.intern(&fqn);
        *self.fqn_name.write().unwrap() = fqn;
        *self.fqn_key.write().unwrap() = key;
    }

    pub fn kind(&self) -> SymbolKind {
        *self.kind.read().unwrap()
    }

    pub fn set_kind(&self, kind: SymbolKind) {
        *self.kind.write().unwrap() = kind;
    }

    pub fn unit_index(&self) -> Option<usize> {
        *self.unit_index.read().unwrap()
    }

    pub fn set_unit_index(&self, file: usize) {
        let mut unit_index = self.unit_index.write().unwrap();
        if unit_index.is_none() {
            *unit_index = Some(file);
        }
    }

    pub fn add_depends_on(&self, sym_id: SymId) {
        if sym_id == self.id {
            return;
        }
        let mut deps = self.depends.write().unwrap();
        if deps.contains(&sym_id) {
            return;
        }
        deps.push(sym_id);
    }

    pub fn add_depended_by(&self, sym_id: SymId) {
        if sym_id == self.id {
            return;
        }
        let mut deps = self.depended.write().unwrap();
        if deps.contains(&sym_id) {
            return;
        }
        deps.push(sym_id);
    }

    pub fn add_dependency(&self, other: &Symbol) {
        self.add_depends_on(other.id);
        other.add_depended_by(self.id);
    }

    pub fn block_id(&self) -> Option<BlockId> {
        *self.block_id.read().unwrap()
    }

    pub fn set_block_id(&self, block_id: Option<BlockId>) {
        *self.block_id.write().unwrap() = block_id;
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
