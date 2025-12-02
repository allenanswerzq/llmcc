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
            id: ScopeId(NEXT_SCOPE_ID.fetch_add(1, Ordering::SeqCst)),
            symbols: RwLock::new(HashMap::new()),
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
        let other_symbols = other.symbols.read().clone();
        let mut self_symbols = self.symbols.write();

        for (name_key, symbol_vec) in other_symbols {
            self_symbols.entry(name_key).or_default().extend(symbol_vec);
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

    pub fn lookup_symbols(
        &self,
        name: InternedStr,
        options: LookupOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        let symbols = self.symbols.read().get(&name).cloned()?;

        let filtered: Vec<&'tcx Symbol> = symbols
            .iter()
            .filter(|symbol| {
                if let Some(kinds) = &options.kind_filters
                    && !kinds.iter().any(|kind| symbol.kind() == *kind)
                {
                    return false;
                }
                if let Some(units) = &options.unit_filters
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

        stack
            .iter()
            .rev()
            .find_map(|scope| scope.lookup_symbols(name_key, options.clone()))
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

        let symbols = scope.lookup_symbols(name_key, LookupOptions::default());
        if let Some(mut symbols) = symbols {
            debug_assert!(!symbols.is_empty());

            if options.chained {
                tracing::debug!("chained symbol '{}' in scope {:?}", name, scope.id());
                // create new symbol chained to existing one
                let symbol = symbols.last().copied().unwrap();
                let new_symbol = Symbol::new(node, name_key);
                new_symbol.set_previous(symbol.id);
                let allocated = self.arena.alloc(new_symbol);
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
            let allocated = self.arena.alloc(new_symbol);
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
        let mut current_scope = *stack.first()?;
        if options.shift_start {
            for i in 0..stack.len() {
                if stack[i]
                    .lookup_symbols(self.interner.intern(qualified_name[0]), options.clone())
                    .is_some()
                {
                    current_scope = stack[i];
                    break;
                }
            }
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

        let part = qualified_name[index];
        let name_key = self.interner.intern(part);
        let symbols = scope.lookup_symbols(name_key, options.clone())?;

        // If this is the last part, return the symbols
        if index == qualified_name.len() - 1 {
            return Some(symbols);
        }

        // Try each symbol as a potential next scope in the hierarchy
        let mut results = Vec::new();
        for symbol in symbols {
            if let Some(symbol_scope_id) = symbol.opt_scope() {
                // ScopeId.0 is 0-indexed (directly usable for arena indexing)
                let scope_index = symbol_scope_id.0;
                let scopes_slice = self.arena.scope();
                if scope_index < scopes_slice.len() {
                    let next_scope = &scopes_slice[scope_index];
                    debug_assert!(next_scope.id() == symbol_scope_id);
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

#[derive(Debug, Clone, Default)]
pub struct LookupOptions {
    pub global: bool,
    pub parent: bool,
    pub chained: bool,
    pub force: bool,
    pub shift_start: bool,
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

    pub fn with_kind_filters(mut self, kind_filters: Vec<SymKind>) -> Self {
        self.kind_filters = Some(kind_filters);
        self
    }

    pub fn with_unit_filters(mut self, unit_filters: Vec<usize>) -> Self {
        self.unit_filters = Some(unit_filters);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interner::InternPool;
    use crate::ir::{Arena, HirId};
    use crate::symbol::{SymKind, Symbol, reset_scope_id_counter, reset_symbol_id_counter};
    use serial_test::serial;

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_empty_name() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let result = scope_stack.lookup_symbols("", LookupOptions::default());
        assert!(result.is_none());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_not_found() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let result = scope_stack.lookup_symbols("nonexistent", LookupOptions::default());
        assert!(result.is_none());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_single_match() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("test")));
        global.insert(sym);

        let result = scope_stack.lookup_symbols("test", LookupOptions::default());
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_multiple_matches() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("overload")));
        let sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("overload")));
        global.insert(sym1);
        global.insert(sym2);

        let result = scope_stack.lookup_symbols("overload", LookupOptions::default());
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 2);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_kind_filter() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let func_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("item")));
        func_sym.set_kind(SymKind::Function);
        let var_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("item")));
        var_sym.set_kind(SymKind::Variable);
        global.insert(func_sym);
        global.insert(var_sym);

        let result = scope_stack.lookup_symbols(
            "item",
            LookupOptions::default().with_kind_filters(vec![SymKind::Function]),
        );
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].kind(), SymKind::Function);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_kind_filter_no_match() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let var_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("item")));
        var_sym.set_kind(SymKind::Variable);
        global.insert(var_sym);

        let result = scope_stack.lookup_symbols(
            "item",
            LookupOptions::default().with_kind_filters(vec![SymKind::Function]),
        );
        assert!(result.is_none());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_unit_filter() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("unit_sym")));
        sym1.set_unit_index(0);
        let sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("unit_sym")));
        sym2.set_unit_index(1);
        global.insert(sym1);
        global.insert(sym2);

        let result = scope_stack.lookup_symbols(
            "unit_sym",
            LookupOptions::default().with_unit_filters(vec![1]),
        );
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].unit_index(), Some(1));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_with_nested_scopes() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let inner_scope = arena.alloc(Scope::new(HirId::new()));
        let global_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("shared")));
        let inner_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("shared")));
        global.insert(global_sym);
        inner_scope.insert(inner_sym);
        inner_scope.add_parent(global);
        scope_stack.push(inner_scope);

        let result = scope_stack.lookup_symbols("shared", LookupOptions::default());
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_symbol_falls_back_to_outer_scope() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let inner_scope = arena.alloc(Scope::new(HirId::new()));
        let global_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("outer_only")));
        global.insert(global_sym);
        inner_scope.add_parent(global);
        scope_stack.push(inner_scope);

        let result = scope_stack.lookup_symbols("outer_only", LookupOptions::default());
        assert!(result.is_some());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_or_insert_creates_new_symbol() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let node = HirId::new();
        let result = scope_stack.lookup_or_insert("new_symbol", node, LookupOptions::current());

        assert!(result.is_some());
        let syms = result.unwrap();
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, interner.intern("new_symbol"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_or_insert_returns_existing_symbol() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let existing_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("existing")));
        global.insert(existing_sym);

        let node = HirId::new();
        let result = scope_stack.lookup_or_insert("existing", node, LookupOptions::current());

        assert!(result.is_some());
        let syms = result.unwrap();
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].id, existing_sym.id);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_or_insert_chained() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let existing_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("chained")));
        global.insert(existing_sym);

        let node = HirId::new();
        let result = scope_stack.lookup_or_insert("chained", node, LookupOptions::chained());

        assert!(result.is_some());
        let syms = result.unwrap();
        assert_eq!(syms.len(), 2);
        let new_sym = syms[1];
        assert_ne!(new_sym.id, existing_sym.id);
        assert_eq!(new_sym.previous(), Some(existing_sym.id));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_or_insert_global() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let inner_scope = arena.alloc(Scope::new(HirId::new()));
        inner_scope.add_parent(global);
        scope_stack.push(inner_scope);

        let node = HirId::new();
        let result = scope_stack.lookup_or_insert("global_only", node, LookupOptions::global());

        assert!(result.is_some());
        let syms = result.unwrap();
        assert_eq!(syms.len(), 1);
        let sym = syms[0];
        let global_lookup =
            global.lookup_symbols(interner.intern("global_only"), LookupOptions::default());
        assert!(global_lookup.is_some());
        assert_eq!(global_lookup.unwrap()[0].id, sym.id);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_or_insert_parent() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let global = scope_stack.globals();
        let inner_scope = arena.alloc(Scope::new(HirId::new()));
        inner_scope.add_parent(global);
        scope_stack.push(inner_scope);

        let node = HirId::new();
        let result = scope_stack.lookup_or_insert("parent_target", node, LookupOptions::parent());

        assert!(result.is_some());
        let _syms = result.unwrap();
        let inner_lookup =
            inner_scope.lookup_symbols(interner.intern("parent_target"), LookupOptions::default());
        assert!(inner_lookup.is_none());
        let parent_lookup =
            global.lookup_symbols(interner.intern("parent_target"), LookupOptions::default());
        assert!(parent_lookup.is_some());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_or_insert_anonymous() {
        let arena = Arena::new();
        let _interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &_interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let node = HirId::new();
        let result = scope_stack.lookup_or_insert("", node, LookupOptions::anonymous());

        assert!(result.is_some());
        let syms = result.unwrap();
        assert_eq!(syms.len(), 1);
        // Anonymous name should be set
        assert_eq!(syms[0].name, _interner.intern("___llmcc_anonymous___"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_options() {
        let opts = LookupOptions::current();
        assert!(!opts.global && !opts.parent && !opts.chained && !opts.force);

        assert!(LookupOptions::global().global);
        assert!(LookupOptions::parent().parent);
        assert!(LookupOptions::chained().chained);
        assert!(LookupOptions::anonymous().force);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_options_builders() {
        let opts = LookupOptions::current()
            .with_global(true)
            .with_chained(true)
            .with_force(true);

        assert!(opts.global);
        assert!(opts.chained);
        assert!(opts.force);
        assert!(!opts.parent);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_merge() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let source_scope = arena.alloc(Scope::new(HirId::new()));
        let target_scope = arena.alloc(Scope::new(HirId::new()));

        let sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("merged1")));
        let sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("merged2")));
        source_scope.insert(sym1);
        source_scope.insert(sym2);
        target_scope.merge_with(source_scope, &arena);

        let lookup1 =
            target_scope.lookup_symbols(interner.intern("merged1"), LookupOptions::default());
        let lookup2 =
            target_scope.lookup_symbols(interner.intern("merged2"), LookupOptions::default());
        assert!(lookup1.is_some());
        assert!(lookup2.is_some());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_stack_depth() {
        let arena = Arena::new();
        let _interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &_interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        assert_eq!(scope_stack.depth(), 1);

        let inner1 = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(inner1);
        assert_eq!(scope_stack.depth(), 2);

        let inner2 = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(inner2);
        assert_eq!(scope_stack.depth(), 3);

        scope_stack.pop();
        assert_eq!(scope_stack.depth(), 2);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_stack_pop_until() {
        let arena = Arena::new();
        let _interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &_interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);
        let inner1 = arena.alloc(Scope::new(HirId::new()));
        let inner2 = arena.alloc(Scope::new(HirId::new()));
        let inner3 = arena.alloc(Scope::new(HirId::new()));

        scope_stack.push(inner1);
        scope_stack.push(inner2);
        scope_stack.push(inner3);

        assert_eq!(scope_stack.depth(), 4);
        scope_stack.pop_until(2);
        assert_eq!(scope_stack.depth(), 2);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_debug_format() {
        let arena = Arena::new();
        let _interner = InternPool::new();
        let global_scope = arena.alloc(Scope::new(HirId::new()));

        let debug_str = format!("{:?}", global_scope);
        assert!(debug_str.contains("Scope"));
        assert!(debug_str.contains("id"));
        assert!(debug_str.contains("owner"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_stack_debug_format_empty() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let debug_str = format!("{:?}", scope_stack);
        assert!(debug_str.contains("ScopeStack"));
        assert!(debug_str.contains("depth"));
        assert!(debug_str.contains("scopes"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_stack_debug_format_multiple_scopes() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let inner1 = arena.alloc(Scope::new(HirId::new()));
        let inner2 = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(inner1);
        scope_stack.push(inner2);

        let debug_str = format!("{:?}", scope_stack);
        assert!(debug_str.contains("ScopeStack"));
        assert!(debug_str.contains("depth: 3"));
        assert!(debug_str.contains("scopes"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_stack_debug_shows_correct_depth() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let debug_str = format!("{:?}", scope_stack);
        // Should show depth 1 for single global scope
        assert!(debug_str.contains("depth: 1"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_debug_contains_symbol_info() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        let sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("test_sym")));
        global_scope.insert(sym);

        let debug_str = format!("{:?}", global_scope);
        assert!(debug_str.contains("symbols"));
        assert!(debug_str.contains("Scope"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_debug_shows_symbol_names_with_interner() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope = arena.alloc(Scope::new_with(HirId::new(), None, Some(&interner)));
        let sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("function")));
        let sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("variable")));
        scope.insert(sym1);
        scope.insert(sym2);

        println!("debug output: {:#?}", scope);
        let debug_str = format!("{:?}", scope);
        assert!(debug_str.contains("function"));
        assert!(debug_str.contains("variable"));
        assert!(debug_str.contains("Scope"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_debug_without_interner() {
        let arena = Arena::new();
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        let interner = InternPool::new();
        let sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("test")));
        global_scope.insert(sym);

        let debug_str = format!("{:?}", global_scope);
        // Without interner, should show [id:kind] format
        assert!(debug_str.contains("["));
        assert!(debug_str.contains(":Unknown]"));
        assert!(debug_str.contains("Scope"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_scope_stack_debug_with_multiple_scopes_and_symbols() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);

        // Create global scope with symbols
        let global_scope = arena.alloc(Scope::new_with(HirId::new(), None, Some(&interner)));
        let global_sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("global_func")));
        let global_sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("global_var")));
        global_scope.insert(global_sym1);
        global_scope.insert(global_sym2);
        scope_stack.push(global_scope);

        // Create inner scope with symbols
        let inner_scope = arena.alloc(Scope::new_with(HirId::new(), None, Some(&interner)));
        let inner_sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("local_func")));
        let inner_sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("param")));
        inner_scope.insert(inner_sym1);
        inner_scope.insert(inner_sym2);
        inner_scope.add_parent(global_scope);
        scope_stack.push(inner_scope);

        // Pretty-print the scope stack
        println!("ScopeStack with multiple scopes and symbols:");
        println!("{:#?}", scope_stack);

        // Verify structure
        assert_eq!(scope_stack.depth(), 2);
        let debug_str = format!("{:?}", scope_stack);
        assert!(debug_str.contains("global_func"));
        assert!(debug_str.contains("global_var"));
        assert!(debug_str.contains("local_func"));
        assert!(debug_str.contains("param"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_qualified_single_part() {
        reset_scope_id_counter();
        reset_symbol_id_counter();
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("crate")));
        global_scope.insert(sym);

        // Lookup single part
        let result = scope_stack.lookup_qualified(&["crate"], LookupOptions::default());
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, interner.intern("crate"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_qualified_multi_part() {
        reset_scope_id_counter();
        reset_symbol_id_counter();
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);

        // Create global scope with "crate" symbol
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let crate_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("crate")));
        global_scope.insert(crate_sym);

        // Create scope for crate with two "foo" symbols
        let crate_scope = arena.alloc(Scope::new(HirId::new()));
        crate_sym.set_scope(crate_scope.id());

        let foo_sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("foo")));
        crate_scope.insert(foo_sym1);

        let foo_sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("foo")));
        crate_scope.insert(foo_sym2);

        // Create scope for first foo with "bar" symbol
        let foo_scope1 = arena.alloc(Scope::new(HirId::new()));
        foo_sym1.set_scope(foo_scope1.id());

        let bar_sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("bar")));
        bar_sym1.set_kind(SymKind::Function);
        foo_scope1.insert(bar_sym1);

        // Create scope for second foo with "bar" symbol
        let foo_scope2 = arena.alloc(Scope::new(HirId::new()));
        foo_sym2.set_scope(foo_scope2.id());

        let bar_sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("bar")));
        bar_sym2.set_kind(SymKind::Variable);
        foo_scope2.insert(bar_sym2);

        // Lookup qualified path: crate::foo::bar
        let result =
            scope_stack.lookup_qualified(&["crate", "foo", "bar"], LookupOptions::current());
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, interner.intern("bar"));
        assert_eq!(symbols[1].name, interner.intern("bar"));
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_qualified_not_found() {
        reset_scope_id_counter();
        reset_symbol_id_counter();
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        // Lookup non-existent qualified path
        let result = scope_stack.lookup_qualified(&["missing", "path"], LookupOptions::current());
        assert!(result.is_none());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_qualified_partial_path() {
        reset_scope_id_counter();
        reset_symbol_id_counter();
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);

        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let crate_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("crate")));
        global_scope.insert(crate_sym);

        let crate_scope = arena.alloc(Scope::new(HirId::new()));
        crate_sym.set_scope(crate_scope.id());

        // Path exists but second part doesn't
        let result = scope_stack.lookup_qualified(&["crate", "missing"], LookupOptions::current());
        assert!(result.is_none());
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_qualified_multiple_choices() {
        reset_scope_id_counter();
        reset_symbol_id_counter();
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);

        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        // Create two "crate" symbols (ambiguous)
        let crate_sym1 = arena.alloc(Symbol::new(HirId::new(), interner.intern("crate")));
        let crate_sym2 = arena.alloc(Symbol::new(HirId::new(), interner.intern("crate")));
        global_scope.insert(crate_sym1);
        global_scope.insert(crate_sym2);

        // First crate has a scope but no "foo"
        let crate_scope1 = arena.alloc(Scope::new(HirId::new()));
        crate_sym1.set_scope(crate_scope1.id());

        // Second crate has a scope with "foo"
        let crate_scope2 = arena.alloc(Scope::new(HirId::new()));
        crate_sym2.set_scope(crate_scope2.id());

        let foo_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("foo")));
        crate_scope2.insert(foo_sym);

        let foo_scope = arena.alloc(Scope::new(HirId::new()));
        foo_sym.set_scope(foo_scope.id());

        let bar_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("bar")));
        bar_sym.set_kind(SymKind::Variable);
        foo_scope.insert(bar_sym);

        // Should recursively try crate_sym1 first (fails at foo lookup)
        // Then try crate_sym2 (succeeds)
        let result =
            scope_stack.lookup_qualified(&["crate", "foo", "bar"], LookupOptions::current());
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, interner.intern("bar"));
        assert_eq!(symbols[0].kind(), SymKind::Variable);
    }

    #[test]
    #[serial(counter_tests)]
    fn test_lookup_qualified_with_stack_iteration() {
        reset_scope_id_counter();
        reset_symbol_id_counter();
        let arena = Arena::new();
        let interner = InternPool::new();
        let scope_stack = ScopeStack::new(&arena, &interner);

        // Create a stack with multiple scopes
        let global_scope = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(global_scope);

        let inner_scope1 = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(inner_scope1);

        let inner_scope2 = arena.alloc(Scope::new(HirId::new()));
        scope_stack.push(inner_scope2);

        // Add "foo" symbol in inner_scope1 (not in global or inner_scope2)
        let foo_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("foo")));
        foo_sym.set_kind(SymKind::Module);
        inner_scope1.insert(foo_sym);

        let foo_scope = arena.alloc(Scope::new(HirId::new()));
        foo_sym.set_scope(foo_scope.id());

        // Add "bar" in foo's scope
        let bar_sym = arena.alloc(Symbol::new(HirId::new(), interner.intern("bar")));
        bar_sym.set_kind(SymKind::Variable);
        foo_scope.insert(bar_sym);

        // Lookup should find "foo" in inner_scope1 (searching forward through stack)
        let result = scope_stack.lookup_qualified(
            &["foo", "bar"],
            LookupOptions::current().with_shift_start(true),
        );
        assert!(result.is_some());
        let symbols = result.unwrap();
        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, interner.intern("bar"));
        assert_eq!(symbols[0].kind(), SymKind::Variable);
    }
}
