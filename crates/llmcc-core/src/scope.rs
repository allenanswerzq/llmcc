//! Scope management and symbol lookup for the code graph.
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::fmt;
use std::sync::atomic::Ordering;

use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId};
use crate::symbol::{NEXT_SCOPE_ID, ScopeId, SymId, SymKindSet, Symbol};

/// Represents a single level in the scope hierarchy.
pub struct Scope<'tcx> {
    /// Unique monotonic scope ID.
    id: ScopeId,
    /// Map of interned symbol names to vectors of symbols (allows for overloading/shadowing within same scope).
    /// Using DashMap for better concurrent read/write performance.
    symbols: DashMap<InternedStr, Vec<&'tcx Symbol>>,
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
    /// Optional interner for resolving symbol names during debugging.
    interner: Option<&'tcx InternPool>,
}

impl<'tcx> Scope<'tcx> {
    /// Creates a new scope owned by the given HIR node.
    pub fn new(owner: HirId) -> Self {
        Self::new_with(owner, None, None)
    }

    /// Creates a new scope owned by the given HIR node and associated with a symbol.
    pub fn new_with(
        owner: HirId,
        symbol: Option<&'tcx Symbol>,
        interner: Option<&'tcx InternPool>,
    ) -> Self {
        Self {
            id: ScopeId(NEXT_SCOPE_ID.fetch_add(1, Ordering::Relaxed)),
            symbols: DashMap::new(),
            owner,
            symbol: RwLock::new(symbol),
            parents: RwLock::new(Vec::new()),
            children: RwLock::new(Vec::new()),
            redirect: RwLock::new(None),
            interner,
        }
    }

    /// Merge existing scope into this scope.
    #[inline]
    pub fn merge_with(&self, other: &'tcx Scope<'tcx>, _arena: &'tcx Arena<'tcx>) {
        tracing::trace!(
            "merge: from scope {:?} to {:?}, {} symbol entries",
            other.id(),
            self.id(),
            other.symbols.len()
        );

        for entry in other.symbols.iter() {
            let name_key = *entry.key();
            let symbol_vec = entry.value().clone();
            self.symbols.entry(name_key).or_default().extend(symbol_vec);
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

    /// Find a parent scope with a symbol of the given kind.
    /// Walks up the parent chain (BFS) looking for a scope whose symbol matches.
    pub fn find_parent_by_kind(&self, kind: crate::symbol::SymKind) -> Option<&'tcx Symbol> {
        use std::collections::VecDeque;
        let mut queue = VecDeque::new();
        queue.extend(self.parents());

        while let Some(parent) = queue.pop_front() {
            if let Some(sym) = parent.opt_symbol()
                && sym.kind() == kind
            {
                return Some(sym);
            }
            queue.extend(parent.parents());
        }
        None
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
        // Collect keys for deterministic iteration order
        let mut keys: Vec<_> = self.symbols.iter().map(|e| *e.key()).collect();
        keys.sort();
        for key in keys {
            if let Some(symbol_vec) = self.symbols.get(&key) {
                for symbol in symbol_vec.iter() {
                    visit(symbol);
                }
            }
        }
    }

    /// Inserts a symbol into this scope.
    pub fn insert(&self, symbol: &'tcx Symbol) -> SymId {
        self.symbols.entry(symbol.name).or_default().push(symbol);
        symbol.id
    }

    pub fn lookup_symbols(
        &self,
        name: InternedStr,
        options: LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        let symbols = self.symbols.get(&name)?.clone();

        let filtered: Vec<&'tcx Symbol> = symbols
            .iter()
            .filter(|symbol| {
                // O(1) bitset check instead of O(n) iteration
                if !options.kind_filters.is_empty() && !options.kind_filters.contains(symbol.kind())
                {
                    return false;
                }
                if let Some(unit) = options.unit_filters
                    && symbol.unit_index() != Some(unit)
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

impl<'tcx> fmt::Debug for ScopeStack<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let stack = self.stack.read();
        let depth = stack.len();

        // Create a vector of scope debug representations
        let scopes_debug: Vec<_> = stack
            .iter()
            .map(|scope| {
                let mut symbol_entries: Vec<String> = Vec::new();

                scope.for_each_symbol(|s| {
                    symbol_entries.push(s.format(Some(self.interner)));
                });

                (scope.id(), scope.owner, symbol_entries)
            })
            .collect();

        f.debug_struct("ScopeStack")
            .field("depth", &depth)
            .field("scopes", &scopes_debug)
            .finish()
    }
}

impl<'tcx> fmt::Debug for Scope<'tcx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let symbol_desc = self.opt_symbol().cloned();
        let mut symbol_entries: Vec<String> = Vec::new();

        self.for_each_symbol(|s| {
            symbol_entries.push(s.format(self.interner));
        });

        f.debug_struct("Scope")
            .field("id", &self.id)
            .field("owner", &self.owner)
            .field("symbol", &symbol_desc)
            .field("symbols", &symbol_entries)
            .finish()
    }
}

/// Manages a stack of nested scopes for symbol resolution and insertion.
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
    pub fn lookup_symbols(&self, name: &str, options: LookupOptions) -> Option<Vec<&'tcx Symbol>> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();
        tracing::trace!("stack: {:#?}", stack);

        let symbols = stack.iter().rev().find_map(|scope| {
            let result = scope.lookup_symbols(name_key, options);
            if let Some(ref syms) = result {
                tracing::trace!(
                    "found '{}' in scope {:?}, symbols: {:?}",
                    name,
                    scope.id(),
                    syms.iter()
                        .map(|s| (s.id(), s.unit_index()))
                        .collect::<Vec<_>>()
                );
            }
            result
        });
        tracing::trace!(
            "lookup_symbols: '{}' found {:?} in scope stack",
            name,
            symbols.as_ref().map(|syms| syms
                .iter()
                .map(|s| s.format(Some(self.interner)))
                .collect::<Vec<_>>())
        );
        symbols
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
    /// This is the only entry point to create a symbol
    pub fn lookup_or_insert(
        &self,
        name: &str,
        node: HirId,
        options: LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        let name_key = self.normalize_name(name, options.force)?;

        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }

        let scope = if options.global {
            tracing::trace!("lookup_or_insert: '{}' in global scope", name);
            stack.first().copied()?
        } else if options.parent && stack.len() >= 2 {
            tracing::trace!("lookup_or_insert: '{}' in parent scope", name);
            stack.get(stack.len() - 2).copied()?
        } else {
            tracing::trace!("lookup_or_insert: '{}' in current scope", name);
            stack.last().copied()?
        };

        // Pass through kind_filters to lookup to support kind-specific lookup/insert
        let lookup_options = if !options.kind_filters.is_empty() {
            LookupOptions::default().with_kind_set(options.kind_filters)
        } else {
            LookupOptions::default()
        };
        let symbols = scope.lookup_symbols(name_key, lookup_options);
        if let Some(mut symbols) = symbols {
            debug_assert!(!symbols.is_empty());

            if options.chained {
                tracing::debug!("chained symbol '{}' in scope {:?}", name, scope.id());
                // create new symbol chained to existing one
                let symbol = symbols.last().copied().unwrap();
                let new_symbol = Symbol::new(node, name_key);
                new_symbol.set_previous(symbol.id);
                let sym_id = new_symbol.id().0;
                let allocated = self.arena.alloc_with_id(sym_id, new_symbol);
                scope.insert(allocated);
                symbols.push(allocated);
                Some(symbols)
            } else {
                tracing::trace!("found existing symbol '{}' in scope {:?}", name, scope.id());
                Some(symbols)
            }
        } else {
            tracing::trace!("create new symbol '{}' in scope {:?}", name, scope.id());
            // not found, create new symbol
            let new_symbol = Symbol::new(node, name_key);
            let sym_id = new_symbol.id().0;
            let allocated = self.arena.alloc_with_id(sym_id, new_symbol);
            scope.insert(allocated);
            Some(vec![allocated])
        }
    }

    /// Lookup a qualified name like `crate::foo::bar` by resolving each part sequentially.
    /// Starts from the global (first) scope and follows the scope chain through the symbol hierarchy.
    pub fn lookup_qualified(
        &self,
        qualified_name: &[&str],
        options: LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        if qualified_name.is_empty() {
            return None;
        }

        let stack = self.stack.read();
        if stack.is_empty() {
            return None;
        }

        // search in forward order to find the current scope where names[0] is defined
        let name_key = self.interner.intern(qualified_name[0]);
        let mut current_scope = *stack.first()?;
        if options.shift_start {
            for i in 0..stack.len() {
                if stack[i].lookup_symbols(name_key, options).is_some() {
                    current_scope = stack[i];
                    break;
                }
            }
            tracing::trace!(
                "lookup_qualified: shifted start to scope {:?} for '{}'",
                current_scope.id(),
                qualified_name[0]
            );
        }

        if current_scope
            .lookup_symbols(name_key, options)
            .is_none()
        {
            tracing::trace!(
                "lookup_qualified: starting scope {:?} does not contain '{}' options: {:?}, stack {:#?}",
                current_scope.id(),
                qualified_name[0],
                options,
                stack
            );
            return None;
        }

        // Recursively try all symbol choices
        self.lookup_qualified_recursive(current_scope, qualified_name, 0, &options)
    }

    /// Helper for recursive qualified lookup that tries all symbol choices.
    fn lookup_qualified_recursive(
        &self,
        scope: &'tcx Scope<'tcx>,
        qualified_name: &[&str],
        index: usize,
        options: &LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        if index >= qualified_name.len() {
            return None;
        }

        tracing::trace!(
            "lookup_qualified_recursive: scope {:?}, part '{}'",
            scope.id(),
            qualified_name[index]
        );
        let part = qualified_name[index];
        let name_key = self.interner.intern(part);
        let symbols = scope.lookup_symbols(name_key, *options)?;
        tracing::trace!(
            "lookup_qualified_recursive: found {:?} symbols for '{}' in scope {:?}",
            symbols
                .iter()
                .map(|s| s.format(Some(self.interner)))
                .collect::<Vec<_>>(),
            part,
            scope.id()
        );

        // If this is the last part, return the symbols
        if index == qualified_name.len() - 1 {
            return Some(symbols);
        }

        // Try each symbol as a potential next scope in the hierarchy
        let mut results = Vec::new();
        for symbol in symbols {
            if let Some(symbol_scope_id) = symbol.opt_scope() {
                // Get scope from DashMap by ID (O(1) lookup)
                if let Some(next_scope) = self.arena.get_scope(symbol_scope_id.0) {
                    debug_assert!(next_scope.id() == symbol_scope_id);
                    tracing::trace!(
                        "lookup_qualified_recursive: descending into scope {:?} for symbol '{}'",
                        next_scope.id(),
                        symbol.format(Some(self.interner))
                    );
                    // Recursively try to find the rest of the path
                    if let Some(result) = self.lookup_qualified_recursive(
                        next_scope,
                        qualified_name,
                        index + 1,
                        options,
                    ) {
                        results.extend(result);
                    }
                }
            }
        }

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct LookupOptions {
    pub global: bool,
    pub parent: bool,
    pub chained: bool,
    pub force: bool,
    pub shift_start: bool,
    pub kind_filters: SymKindSet,
    pub unit_filters: Option<usize>,
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

    pub fn with_shift_start(mut self, shift: bool) -> Self {
        self.shift_start = shift;
        self
    }

    pub fn with_kind_set(mut self, kind_set: SymKindSet) -> Self {
        self.kind_filters = kind_set;
        self
    }

    pub fn with_unit_filter(mut self, unit: usize) -> Self {
        self.unit_filters = Some(unit);
        self
    }
}
