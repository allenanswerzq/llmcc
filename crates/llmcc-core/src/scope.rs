//! Scope management and symbol lookup for the code graph.
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::Ordering;

use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId, HirNode};
use crate::symbol::{NEXT_SCOPE_ID, ScopeId, SymId, SymKind, Symbol};

/// Represents a single level in the scope hierarchy.
pub struct Scope<'tcx> {
    /// Unique monotonic scope ID.
    id: ScopeId,
    /// Map of interned symbol names to vectors of symbols (allows for overloading/shadowing within same scope).
    symbols: RwLock<HashMap<InternedStr, Vec<&'tcx Symbol>>>,
    /// The HIR node that owns/introduces this scope.
    owner: HirId,
    /// The symbol that introduced this scope (e.g., function symbol).
    symbol: RwLock<Option<&'tcx Symbol>>,
    /// Parent scopes for inheritance (lexical chaining).
    parents: RwLock<Vec<&'tcx Scope<'tcx>>>,
    /// Child scopes nested within this scope.
    #[allow(dead_code)]
    children: RwLock<Vec<&'tcx Scope<'tcx>>>,
    /// If this scope was merged into another, points to the target scope ID.
    /// Used to redirect lookups from merged scopes to their parent scope.
    redirect: RwLock<Option<ScopeId>>,
}

impl<'tcx> Scope<'tcx> {
    /// Creates a new scope owned by the given HIR node.
    pub fn new(owner: HirId) -> Self {
        Self::new_with(owner, None)
    }

    /// Creates a new scope owned by the given HIR node and associated with a symbol.
    pub fn new_with(owner: HirId, symbol: Option<&'tcx Symbol>) -> Self {
        Self {
            id: ScopeId(NEXT_SCOPE_ID.fetch_add(1, Ordering::SeqCst)),
            symbols: RwLock::new(HashMap::new()),
            owner,
            symbol: RwLock::new(symbol),
            parents: RwLock::new(Vec::new()),
            children: RwLock::new(Vec::new()),
            redirect: RwLock::new(None),
        }
    }

    /// Creates a new scope from an existing scope, copying its structure.
    pub fn new_from<'src>(other: &Scope<'src>, arena: &'tcx Arena<'tcx>) -> Self {
        let symbol_ref = other.symbol.read().map(|s| arena.alloc(s.clone()));

        let new_scope = Self {
            id: other.id,
            symbols: RwLock::new(HashMap::new()),
            owner: other.owner,
            symbol: RwLock::new(symbol_ref),
            parents: RwLock::new(Vec::new()),
            children: RwLock::new(Vec::new()),
            redirect: RwLock::new(None),
        };

        // Deep copy symbols
        other.for_each_symbol(|source_symbol| {
            let allocated = arena.alloc(source_symbol.clone());
            new_scope.insert(allocated);
        });

        new_scope
    }

    /// Merge existing scope into this scope.
    #[inline]
    pub fn merge_with(&self, other: &'tcx Scope<'tcx>, _arena: &'tcx Arena<'tcx>) {
        let other_symbols = other.symbols.read().clone();
        let mut self_symbols = self.symbols.write();

        for (name_key, symbol_vec) in other_symbols {
            self_symbols.entry(name_key)
                .or_insert_with(Vec::new)
                .extend(symbol_vec);
        }
    }

    #[inline]
    pub fn add_parent(&self, parent: &'tcx Scope<'tcx>) {
        self.parents.write().push(parent);
    }

    #[inline]
    pub fn owner(&self) -> HirId {
        self.owner
    }

    #[inline]
    pub fn set_symbol(&self, symbol: &'tcx Symbol) {
        *self.symbol.write() = Some(symbol);
    }

    #[inline]
    pub fn opt_symbol(&self) -> Option<&'tcx Symbol> {
        *self.symbol.read()
    }

    #[inline]
    pub fn id(&self) -> ScopeId {
        self.id
    }

    /// If this scope was redirected (merged into another), get the target scope ID.
    pub fn get_redirect(&self) -> Option<ScopeId> {
        *self.redirect.read()
    }

    /// Set the redirect for this scope (used when merging scopes).
    pub fn set_redirect(&self, target: ScopeId) {
        *self.redirect.write() = Some(target);
    }

    #[inline]
    pub fn parents(&self) -> Vec<&'tcx Scope<'tcx>> {
        self.parents.read().clone()
    }

    /// Invokes a closure for each symbol in this scope.
    /// Iterates in deterministic order by sorting keys.
    pub fn for_each_symbol<F>(&self, mut visit: F)
    where
        F: FnMut(&'tcx Symbol),
    {
        let symbols = self.symbols.read();
        // Sort keys for deterministic iteration order
        let mut keys: Vec<_> = symbols.keys().copied().collect();
        keys.sort();
        for key in keys {
            if let Some(symbol_vec) = symbols.get(&key) {
                for symbol in symbol_vec {
                    visit(symbol);
                }
            }
        }
    }

    /// Inserts a symbol into this scope.
    pub fn insert(&self, symbol: &'tcx Symbol) -> SymId {
        self.symbols
            .write()
            .entry(symbol.name)
            .or_default()
            .push(symbol);
        symbol.id
    }

    /// Looks up all symbols with the given name in this specific scope.
    pub fn lookup_symbols(&self, name: InternedStr) -> Option<Vec<&'tcx Symbol>> {
        self.symbols.read().get(&name).cloned()
    }

    /// Looks up symbols with optional kind and unit filters within this scope.
    pub fn lookup_symbols_with(
        &self,
        name: InternedStr,
        kind_filter: Option<SymKind>,
        unit_filter: Option<usize>,
    ) -> Option<Vec<&'tcx Symbol>> {
        let symbols = self.lookup_symbols(name)?;

        let filtered: Vec<&'tcx Symbol> = symbols
            .into_iter()
            .filter(|symbol| {
                let kind_match = kind_filter.is_none_or(|k| k == symbol.kind());
                let unit_match = unit_filter.is_none_or(|u| Some(u) == symbol.unit_index());
                kind_match && unit_match
            })
            .collect();

        (!filtered.is_empty()).then_some(filtered)
    }
}

impl<'tcx> fmt::Debug for Scope<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol_desc = self.opt_symbol().cloned();
        // Collect symbols for debug printing without holding the lock too long
        let mut symbol_entries = Vec::new();
        self.for_each_symbol(|s| symbol_entries.push(s.clone()));

        f.debug_struct("Scope")
            .field("id", &self.id)
            .field("owner", &self.owner)
            .field("symbol", &symbol_desc)
            .field("symbols", &symbol_entries)
            .finish()
    }
}

/// Manages a stack of nested scopes for symbol resolution and insertion.
#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    arena: &'tcx Arena<'tcx>,
    interner: &'tcx InternPool,
    /// Stack of nested scopes (Index 0 = Global, Index End = Current).
    stack: RwLock<Vec<&'tcx Scope<'tcx>>>,
}

impl<'tcx> Clone for ScopeStack<'tcx> {
    fn clone(&self) -> Self {
        Self {
            arena: self.arena,
            interner: self.interner,
            stack: RwLock::new(self.stack.read().clone()),
        }
    }
}

impl<'tcx> ScopeStack<'tcx> {
    pub fn new(arena: &'tcx Arena<'tcx>, interner: &'tcx InternPool) -> Self {
        Self {
            arena,
            interner,
            stack: RwLock::new(Vec::new()),
        }
    }

    #[inline]
    pub fn depth(&self) -> usize {
        self.stack.read().len()
    }

    #[inline]
    pub fn push(&self, scope: &'tcx Scope<'tcx>) {
        self.stack.write().push(scope);
    }

    pub fn iter(&self) -> Vec<&'tcx Scope<'tcx>> {
        self.stack.read().clone()
    }

    pub fn first(&self) -> &'tcx Scope<'tcx> {
        self.stack.read().first().copied().unwrap()
    }

    /// Recursively pushes a scope and all its base (parent) scopes onto the stack.
    pub fn push_recursive(&self, scope: &'tcx Scope<'tcx>) {
        let mut candidates = vec![scope];
        let mut linear_chain = Vec::new();
        let mut visited = HashSet::new();

        // Traverse parents graph to build a linear stack
        while let Some(current) = candidates.pop() {
            if !visited.insert(current.id()) {
                continue;
            }
            linear_chain.push(current);

            let parents = current.parents.read();
            // Push parents in reverse order so the primary parent is processed last (LIFO)
            for base in parents.iter().rev() {
                if !visited.contains(&base.id()) {
                    candidates.push(base);
                }
            }
        }

        // Apply to stack (reverse the chain so `scope` is at the top)
        let mut stack = self.stack.write();
        for s in linear_chain.iter().rev() {
            stack.push(s);
        }
    }

    #[inline]
    pub fn pop(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.write().pop()
    }

    pub fn pop_until(&self, depth: usize) {
        let mut stack = self.stack.write();
        while stack.len() > depth {
            stack.pop();
        }
    }

    #[inline]
    pub fn top(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.read().last().copied()
    }

    /// Normalize name helper.
    fn normalize_name(&self, name: &str, force: bool) -> Option<InternedStr> {
        let name_to_intern = if !name.is_empty() {
            name
        } else if force {
            "___anonymous___"
        } else {
            return None;
        };
        Some(self.interner.intern(name_to_intern))
    }

    pub fn lookup_symbol(&self, name: &str) -> Option<&'tcx Symbol> {
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();

        stack.iter().rev().find_map(|scope| {
            scope
                .lookup_symbols(name_key)
                .and_then(|matches| matches.last().copied())
        })
    }

    /// Look up a symbol only in the global (first) scope.
    /// Used for crate-root paths like ::f or ::g::h.
    pub fn lookup_global_symbol(&self, name: &str) -> Option<&'tcx Symbol> {
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();
        let global_scope = stack.first()?;
        global_scope
            .lookup_symbols(name_key)
            .and_then(|matches| matches.last().copied())
    }

    pub fn lookup_symbol_with(
        &self,
        name: &str,
        kind_filters: Option<Vec<SymKind>>,
        unit_filters: Option<Vec<usize>>,
    ) -> Option<&'tcx Symbol> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();

        stack.iter().rev().find_map(|scope| {
            let matches = scope.lookup_symbols(name_key)?;
            matches.iter().rev().find_map(|symbol| {
                if let Some(kinds) = &kind_filters
                    && !kinds.iter().any(|kind| symbol.kind() == *kind)
                {
                    return None;
                }
                if let Some(units) = &unit_filters
                    && !units.iter().any(|unit| symbol.unit_index() == Some(*unit))
                {
                    return None;
                }
                Some(*symbol)
            })
        })
    }

    /// Find existing symbol or insert new one in the current scope.
    pub fn lookup_or_insert(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.handle_lookup_or_insert(name, node.id(), LookupOptions::current())
    }

    /// Find or insert with chaining (shadowing support).
    pub fn lookup_or_insert_chained(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.handle_lookup_or_insert(name, node.id(), LookupOptions::chained())
    }

    pub fn lookup_or_insert_parent(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.handle_lookup_or_insert(name, node.id(), LookupOptions::parent())
    }

    pub fn lookup_or_insert_global(&self, name: &str, node: &HirNode) -> Option<&'tcx Symbol> {
        self.handle_lookup_or_insert(name, node.id(), LookupOptions::global())
    }

    /// Full control API.
    pub fn lookup_or_insert_with(
        &self,
        name: &str,
        node: &HirNode,
        options: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        self.handle_lookup_or_insert(name, node.id(), options)
    }

    /// Internal implementation for lookup/insert logic.
    fn handle_lookup_or_insert(
        &self,
        name: &str,
        node: HirId,
        options: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let name_key = self.normalize_name(name, options.force)?;

        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }

        let scope = if options.global {
            stack.first().copied()
        } else if options.parent && stack.len() >= 2 {
            stack.get(stack.len() - 2).copied()
        } else {
            stack.last().copied()
        };

        let existing_symbols = scope.and_then(|s| s.lookup_symbols(name_key));
        let latest_existing = existing_symbols.as_ref().and_then(|v| v.last().copied());
        if !options.top
            && let Some(existing) = latest_existing
        {
            return Some(existing);
        }

        let new_symbol = Symbol::new(node, name_key);
        if options.top
            && let Some(prev) = latest_existing
        {
            new_symbol.set_previous(prev.id);
        }

        let allocated = self.arena.alloc(new_symbol);
        if let Some(s) = scope {
            s.insert(allocated);
        }

        Some(allocated)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LookupOptions {
    pub global: bool,
    pub parent: bool,
    /// If true, always creates a new symbol, chaining it to any existing one (shadowing).
    /// If false, returns the existing symbol if found.
    pub top: bool,
    pub force: bool,
}

impl LookupOptions {
    pub fn current() -> Self {
        Self {
            global: false,
            parent: false,
            top: false,
            force: false,
        }
    }

    pub fn global() -> Self {
        Self {
            global: true,
            parent: false,
            top: false,
            force: false,
        }
    }

    pub fn parent() -> Self {
        Self {
            global: false,
            parent: true,
            top: false,
            force: false,
        }
    }

    pub fn chained() -> Self {
        Self {
            global: false,
            parent: false,
            top: true,
            force: false,
        }
    }

    pub fn anonymous() -> Self {
        Self {
            global: false,
            parent: false,
            top: false,
            force: true,
        }
    }

    pub fn with_global(mut self, global: bool) -> Self {
        self.global = global;
        self
    }

    pub fn with_parent(mut self, parent: bool) -> Self {
        self.parent = parent;
        self
    }

    pub fn with_top(mut self, top: bool) -> Self {
        self.top = top;
        self
    }

    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }
}
