//! Scope management and symbol lookup for the code graph.
use parking_lot::RwLock;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::sync::atomic::Ordering;

use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId};
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
        };

        // Deep copy symbols
        other.for_each_symbol(|source_symbol| {
            let allocated = arena.alloc(source_symbol.clone());
            new_scope.insert(allocated);
        });

        new_scope
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

    #[inline]
    pub fn parents(&self) -> Vec<&'tcx Scope<'tcx>> {
        self.parents.read().clone()
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

    /// Inserts a symbol into this scope using FQN as the key.
    /// This is used for global scope to avoid name collisions (e.g., multiple `new` functions).
    pub fn insert_with_fqn(&self, symbol: &'tcx Symbol) -> SymId {
        self.symbols
            .write()
            .entry(symbol.fqn())
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
        option: &LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        let symbols = self.lookup_symbols(name)?;

        let filtered: Vec<&'tcx Symbol> = symbols
            .into_iter()
            .filter(|symbol| {
                let kind_match = option
                    .kind_filters
                    .as_ref()
                    .is_none_or(|kinds| kinds.iter().any(|k| *k == symbol.kind()));
                let unit_match = option
                    .unit_filters
                    .as_ref()
                    .is_none_or(|units| symbol.unit_index().is_some_and(|u| units.contains(&u)));
                kind_match && unit_match
            })
            .collect();

        (!filtered.is_empty()).then_some(filtered)
    }

    /// Iterates over all symbols in this scope.
    pub fn for_each_symbol<F>(&self, mut f: F)
    where
        F: FnMut(&'tcx Symbol),
    {
        let symbols = self.symbols.read();
        for syms in symbols.values() {
            for sym in syms {
                f(sym);
            }
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

    pub fn lookup_symbols(
        &self,
        symbol: &'tcx Symbol,
        option: LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        let simple_key = symbol.name;
        let fqn_key = symbol.fqn();

        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }

        if option.global {
            // search global scope with fqn key
            return stack[0].lookup_symbols_with(fqn_key, &option);
        }

        // search local scope with simple name key
        stack[1..]
            .iter()
            .rev()
            .find_map(|scope| scope.lookup_symbols_with(simple_key, &option))
            .or_else(|| {
                // search global scope with fqn key
                stack[0].lookup_symbols_with(fqn_key, &option)
            })
    }

    pub fn lookup_symbol(
        &self,
        symbol: &'tcx Symbol,
        option: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let symbols = self.lookup_symbols(symbol, option)?;
        if symbols.len() == 1 {
            return Some(symbols[0]);
        } else {
            tracing::warn!(
                "ambiguous symbol lookup for '{}', found {} candidates",
                self.interner.resolve_owned(symbol.fqn())?,
                symbols.len()
            );
            return None;
        }
    }

    /// String-based symbol lookup (for identifiers without pre-set symbols)
    pub fn lookup_symbol_by_name(
        &self,
        name: &str,
        option: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }

        if option.global {
            let symbols = stack[0].lookup_symbols_with(name_key, &option)?;
            return symbols.into_iter().next();
        }

        // Search local scopes first, then global
        stack[1..]
            .iter()
            .rev()
            .find_map(|scope| scope.lookup_symbols_with(name_key, &option))
            .or_else(|| stack[0].lookup_symbols_with(name_key, &option))
            .and_then(|symbols| symbols.into_iter().next())
    }

    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: HirId,
        option: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        self.handle_lookup_or_insert(name, node, option)
    }

    pub fn lookup_or_insert_global(
        &self,
        name: &str,
        node: HirId,
        option: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let option = option.with_global(true);
        self.handle_lookup_or_insert(name, node, option)
    }

    pub fn lookup_or_insert_parent(
        &self,
        name: &str,
        node: HirId,
        option: LookupOptions,
    ) -> Option<&'tcx Symbol> {
        let option = option.with_parent(true);
        self.handle_lookup_or_insert(name, node, option)
    }

    fn normalize_name(&self, name: &str, force: bool) -> Option<InternedStr> {
        let name_to_intern = if !name.is_empty() {
            name
        } else if force {
            "___llmcc_anonymous"
        } else {
            return None;
        };
        Some(self.interner.intern(name_to_intern))
    }

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

        debug_assert!(
            !existing_symbols
                .as_ref()
                .map_or(false, |v| v.len() > 1 && !options.chain),
            "symbol {:?} already exists in scope {:?} and chaining is disabled",
            name_key,
            scope.map(|s| s.id())
        );

        let latest_existing = existing_symbols.as_ref().and_then(|v| v.last().copied());
        let new_symbol = Symbol::new(node, name_key);
        if let Some(existing) = latest_existing {
            if !options.chain {
                return Some(existing);
            } else {
                new_symbol.set_previous(existing.id);
            }
        }

        let allocated = self.arena.alloc(new_symbol);
        if let Some(s) = scope {
            s.insert(allocated);
        }

        Some(allocated)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LookupOptions {
    pub global: bool,
    pub parent: bool,
    pub chain: bool,
    pub force: bool,
    pub kind_filters: Option<Vec<SymKind>>,
    pub unit_filters: Option<Vec<usize>>,
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
            chain: true,
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

    pub fn with_chain(mut self, chain: bool) -> Self {
        self.chain = chain;
        self
    }

    pub fn with_force(mut self, force: bool) -> Self {
        self.force = force;
        self
    }

    pub fn with_kind_filters(mut self, kinds: Vec<SymKind>) -> Self {
        self.kind_filters = Some(kinds);
        self
    }
}
