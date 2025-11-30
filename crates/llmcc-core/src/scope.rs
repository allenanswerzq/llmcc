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
    fn lookup_symbol_inner(&self, name: InternedStr) -> Option<Vec<&'tcx Symbol>> {
        self.symbols.read().get(&name).cloned()
    }

    pub fn lookup_symbol_with(
        &self,
        name: InternedStr,
        kind_filters: Option<Vec<SymKind>>,
        unit_filters: Option<Vec<usize>>,
    ) -> Option<Vec<&'tcx Symbol>> {
        let matched_symbols = self.lookup_symbol_inner(name)?;

        let filtered: Vec<&'tcx Symbol> =
        matched_symbols
            .iter()
            .filter(|symbol| {
                if let Some(kinds) = &kind_filters
                    && !kinds.iter().any(|kind| symbol.kind() == *kind)
                {
                    return false;
                }
                if let Some(units) = &unit_filters
                    && !units.iter().any(|unit| symbol.unit_index() == Some(*unit))
                {
                    return false;
                }
                true
            })
            .copied()
            .collect();

        if filtered.is_empty() {
            None
        } else {
            Some(filtered)
        }
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

    #[inline]
    pub fn globals(&self) -> &'tcx Scope<'tcx> {
        self.stack.read().first().copied().unwrap()
    }

    #[inline]
    pub fn top(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.read().last().copied()
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

    /// This should be the only pure api to lookup symbols following the
    /// lexical scope backwards.
    pub fn lookup_symbol_with(
        &self,
        name: &str,
        kind_filters: Option<Vec<SymKind>>,
        unit_filters: Option<Vec<usize>>,
    ) -> Option<Vec<&'tcx Symbol>> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();

        stack.iter().rev().find_map(|scope| {
            scope.lookup_symbol_with(name_key, kind_filters.clone(), unit_filters.clone())
        })
    }

    /// Normalize name helper.
    fn normalize_name(&self, name: &str, force: bool) -> Option<InternedStr> {
        let name_to_intern = if !name.is_empty() {
            name
        } else if force {
            "___llmcc_anonymous___"
        } else {
            return None;
        };
        Some(self.interner.intern(name_to_intern))
    }

    /// Internal implementation for lookup/insert logic.
    pub fn lookup_or_insert(
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
            tracing::trace!("lookup global scope for symbol '{}'", name);
            stack.first().copied()?
        } else if options.parent && stack.len() >= 2 {
            tracing::trace!("lookup parent scope for symbol '{}'", name);
            stack.get(stack.len() - 2).copied()?
        } else {
            tracing::trace!("lookup current scope for symbol '{}'", name);
            stack.last().copied()?
        };

        let symbols = scope.lookup_symbol_with(name_key, None, None);
        if let Some(symbols) = symbols {
            debug_assert!(!symbols.is_empty());

            if symbols.len() > 1 {
                tracing::trace!(
                    "found mutpile {} symbols for name '{}' in scope {:?}",
                    symbols.len(),
                    name,
                    scope.id()
                );
            }

            let symbol = symbols.last().copied().unwrap();
            if options.chained {
                // create new symbol chained to existing one
                let new_symbol = Symbol::new(node, name_key);
                new_symbol.set_previous(symbol.id);
                let allocated = self.arena.alloc(new_symbol);
                scope.insert(allocated);
                return Some(allocated);
            } else {
                // return existing symbol
                return Some(symbol);
            }

        } else {
            // not name found, create new symbol
            let new_symbol = Symbol::new(node, name_key);
            let allocated = self.arena.alloc(new_symbol);
            scope.insert(allocated);
            return Some(allocated);
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LookupOptions {
    pub global: bool,
    pub parent: bool,
    pub chained: bool,
    pub force: bool,
}

impl LookupOptions {
    pub fn current() -> Self {
        Self::default()
    }

    pub fn global() -> Self {
        Self {
            global: true,
            ..Default::default()
        }
    }

    pub fn parent() -> Self {
        Self {
            parent: true,
            ..Default::default()
        }
    }

    pub fn chained() -> Self {
        Self {
            chained: true,
            ..Default::default()
        }
    }

    pub fn anonymous() -> Self {
        Self {
            force: true,
            ..Default::default()
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

    pub fn with_chained(mut self, chained: bool) -> Self {
        self.chained = chained;
        self
    }

    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }
}
