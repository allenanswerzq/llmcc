//! Scope management and symbol lookup for the code graph.
use dashmap::DashMap;
use parking_lot::RwLock;
use std::collections::{HashSet, VecDeque};
use std::fmt;

use crate::id::next_scope_id;
use crate::interner::{InternPool, InternedStr};
use crate::ir::{Arena, HirId};
use crate::symbol::{ScopeId, SymId, SymKindSet, Symbol};

/// Symbol table owned by one semantic scope.
///
/// A `Scope` stores symbols declared by one HIR owner and optional semantic
/// parent scopes used for member and hierarchy traversal. Lexical visibility is
/// handled by [`ScopeStack`], not by `Scope` itself.
pub struct Scope<'tcx> {
    id: ScopeId,
    symbols: DashMap<InternedStr, Vec<&'tcx Symbol>>,
    owner: HirId,
    symbol: RwLock<Option<&'tcx Symbol>>,
    /// Semantic parents used for hierarchy/member traversal. Lexical lookup is handled by ScopeStack.
    parents: RwLock<Vec<&'tcx Scope<'tcx>>>,
    redirect: RwLock<Option<ScopeId>>,
    interner: Option<&'tcx InternPool>,
}

impl<'tcx> Scope<'tcx> {
    /// Create an empty scope owned by the given HIR node.
    pub fn new(owner: HirId) -> Self {
        Self::new_with(owner, None, None)
    }

    /// Create a scope owned by the given HIR node and optional defining symbol.
    pub fn new_with(
        owner: HirId,
        symbol: Option<&'tcx Symbol>,
        interner: Option<&'tcx InternPool>,
    ) -> Self {
        Self::from_symbols(owner, symbol, interner, DashMap::new())
    }

    /// Create a scope with a specific symbol-map shard count.
    ///
    /// Use this for high-contention scopes like globals.
    pub fn new_with_shards(
        owner: HirId,
        symbol: Option<&'tcx Symbol>,
        interner: Option<&'tcx InternPool>,
        shard_count: usize,
    ) -> Self {
        Self::from_symbols(
            owner,
            symbol,
            interner,
            DashMap::with_hasher_and_shard_amount(std::hash::RandomState::new(), shard_count),
        )
    }

    fn from_symbols(
        owner: HirId,
        symbol: Option<&'tcx Symbol>,
        interner: Option<&'tcx InternPool>,
        symbols: DashMap<InternedStr, Vec<&'tcx Symbol>>,
    ) -> Self {
        Self {
            id: next_scope_id(),
            symbols,
            owner,
            symbol: RwLock::new(symbol),
            parents: RwLock::new(Vec::new()),
            redirect: RwLock::new(None),
            interner,
        }
    }

    /// Merge symbols from another scope into this scope.
    #[inline]
    pub fn merge_with(&self, other: &'tcx Scope<'tcx>) {
        for entry in other.symbols.iter() {
            let name_key = *entry.key();
            let symbol_vec = entry.value().clone();
            self.symbols.entry(name_key).or_default().extend(symbol_vec);
        }
    }

    #[inline]
    pub fn add_parent(&self, parent: &'tcx Scope<'tcx>) {
        if parent.id() == self.id() {
            return;
        }
        let mut parents = self.parents.write();
        if parents.iter().all(|scope| scope.id() != parent.id()) {
            parents.push(parent);
        }
    }

    #[inline]
    pub fn owner(&self) -> HirId {
        self.owner
    }

    #[inline]
    pub fn set_symbol(&self, symbol: &'tcx Symbol) {
        *self.symbol.write() = Some(symbol);
    }

    /// Return this scope's defining symbol, if one has been attached.
    #[inline]
    pub fn try_symbol(&self) -> Option<&'tcx Symbol> {
        *self.symbol.read()
    }

    /// Find a semantic parent scope introduced by a symbol of the given kind.
    pub fn try_parent_symbol(&self, kind: crate::symbol::SymKind) -> Option<&'tcx Symbol> {
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.extend(self.parents());

        while let Some(parent) = queue.pop_front() {
            if !visited.insert(parent.id()) {
                continue;
            }
            if let Some(sym) = parent.try_symbol()
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

    /// Return this scope's redirect target, if it was merged into another scope.
    pub fn try_redirect(&self) -> Option<ScopeId> {
        *self.redirect.read()
    }

    /// Set this scope's redirect target after merging scopes.
    pub fn set_redirect(&self, target: ScopeId) {
        *self.redirect.write() = Some(target);
    }

    /// Return semantic parent scopes used for hierarchy traversal.
    #[inline]
    pub fn parents(&self) -> Vec<&'tcx Scope<'tcx>> {
        self.parents.read().clone()
    }

    /// Invokes a closure for each symbol in deterministic name order.
    pub fn for_each_symbol<F>(&self, mut visit: F)
    where
        F: FnMut(&'tcx Symbol),
    {
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

    /// Insert a symbol into this scope and return its id.
    pub fn insert(&self, symbol: &'tcx Symbol) -> SymId {
        self.symbols.entry(symbol.name).or_default().push(symbol);
        symbol.id
    }

    /// Look up symbols by interned name within this scope only.
    pub fn try_lookup_symbols(
        &self,
        name: InternedStr,
        filter: SymbolFilter,
    ) -> Option<Vec<&'tcx Symbol>> {
        let guard = self.symbols.get(&name)?;
        let symbols = guard.value();

        if filter.is_empty() {
            return Some(symbols.clone());
        }

        let filtered: Vec<&'tcx Symbol> = symbols
            .iter()
            .copied()
            .filter(|symbol| filter.matches(symbol))
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
        let symbol_desc = self.try_symbol().cloned();
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

/// Lexical scope stack used for lookup and scoped symbol insertion.
///
/// The stack itself models lexical visibility from globals to the current
/// scope. Individual scopes may also carry semantic parents; [`push_recursive`]
/// exposes those parents beneath the requested scope for member-like lookup.
pub struct ScopeStack<'tcx> {
    arena: &'tcx Arena<'tcx>,
    interner: &'tcx InternPool,
    /// In normal resolver stacks, index 0 is the shared global scope and the last index is current.
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
    /// Create an empty scope stack backed by the shared HIR arena and interner.
    pub fn new(arena: &'tcx Arena<'tcx>, interner: &'tcx InternPool) -> Self {
        Self {
            arena,
            interner,
            stack: RwLock::new(Vec::new()),
        }
    }

    /// Number of scopes currently on the stack.
    #[inline]
    pub fn depth(&self) -> usize {
        self.stack.read().len()
    }

    /// Push one scope onto the top of the stack.
    #[inline]
    pub fn push(&self, scope: &'tcx Scope<'tcx>) {
        self.stack.write().push(scope);
    }

    /// Return the global scope at stack index 0.
    ///
    /// Panics if called before a global scope has been pushed.
    #[inline]
    pub fn globals(&self) -> &'tcx Scope<'tcx> {
        self.stack
            .read()
            .first()
            .copied()
            .expect("scope stack must contain a global scope")
    }

    /// Return the current lexical scope, if the stack is not empty.
    #[inline]
    pub fn try_current(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.read().last().copied()
    }

    /// Pushes a scope plus its semantic parents, with the requested scope left on top.
    pub fn push_recursive(&self, scope: &'tcx Scope<'tcx>) {
        let mut candidates = vec![scope];
        let mut linear_chain = Vec::new();
        let mut visited = HashSet::new();

        while let Some(current) = candidates.pop() {
            if !visited.insert(current.id()) {
                continue;
            }
            linear_chain.push(current);

            for parent in current.parents().into_iter().rev() {
                if !visited.contains(&parent.id()) {
                    candidates.push(parent);
                }
            }
        }

        let mut stack = self.stack.write();
        for scope in linear_chain.into_iter().rev() {
            stack.push(scope);
        }
    }

    /// Pop the current scope from the stack.
    #[inline]
    pub fn pop(&self) -> Option<&'tcx Scope<'tcx>> {
        self.stack.write().pop()
    }

    /// Pop scopes until the stack reaches `depth`.
    pub fn pop_until(&self, depth: usize) {
        let mut stack = self.stack.write();
        while stack.len() > depth {
            stack.pop();
        }
    }

    /// Looks up a name from innermost lexical scope to outermost scope.
    pub fn try_lookup_symbols(
        &self,
        name: &str,
        filter: SymbolFilter,
    ) -> Option<Vec<&'tcx Symbol>> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner.intern(name);
        let stack = self.stack.read();

        stack
            .iter()
            .rev()
            .find_map(|scope| scope.try_lookup_symbols(name_key, filter))
    }

    /// Find existing symbols in the target scope or create one there.
    ///
    /// Returns `None` when `name` is empty or when the target scope does not
    /// exist, such as inserting into the current scope on an empty stack.
    pub fn try_lookup_or_insert(
        &self,
        name: &str,
        node: HirId,
        options: InsertOptions,
    ) -> Option<Vec<&'tcx Symbol>> {
        if name.is_empty() {
            return None;
        }
        let name_key = self.interner.intern(name);

        let stack = self.stack.read();
        let scope = options.insert_scope.select(&stack)?;

        let symbols = scope.try_lookup_symbols(name_key, options.existing_filter());
        if let Some(symbols) = symbols {
            debug_assert!(!symbols.is_empty());
            Some(symbols)
        } else {
            let new_symbol = Symbol::new(node, name_key);
            let sym_id = new_symbol.id().0;
            let allocated = self.arena.alloc_with_id(sym_id, new_symbol);
            scope.insert(allocated);
            Some(vec![allocated])
        }
    }

    /// Resolves a qualified path by following each part's owned scope.
    ///
    /// By default the first part is resolved from the global scope. With lexical
    /// start enabled, visible scopes are tried from innermost to outermost. Kind
    /// filters apply only to the final path component.
    pub fn try_lookup_qualified(
        &self,
        qualified_name: &[&str],
        query: QualifiedLookup,
    ) -> Option<Vec<&'tcx Symbol>> {
        if qualified_name.is_empty() || qualified_name.iter().any(|part| part.is_empty()) {
            return None;
        }

        for scope in self.qualified_start_scopes(qualified_name[0], query) {
            if let Some(result) =
                self.try_lookup_qualified_recursive(scope, qualified_name, 0, &query)
            {
                return Some(result);
            }
        }

        None
    }

    fn qualified_start_scopes(
        &self,
        first_part: &str,
        query: QualifiedLookup,
    ) -> Vec<&'tcx Scope<'tcx>> {
        let stack = self.stack.read();
        if stack.is_empty() {
            return Vec::new();
        }

        if query.start == QualifiedStart::Global {
            return stack.first().copied().into_iter().collect();
        }

        let name_key = self.interner.intern(first_part);
        let mut visited = HashSet::new();
        stack
            .iter()
            .rev()
            .copied()
            .filter(|scope| {
                visited.insert(scope.id())
                    && scope
                        .try_lookup_symbols(name_key, SymbolFilter::any())
                        .is_some()
            })
            .collect()
    }

    fn try_lookup_qualified_recursive(
        &self,
        scope: &'tcx Scope<'tcx>,
        qualified_name: &[&str],
        index: usize,
        query: &QualifiedLookup,
    ) -> Option<Vec<&'tcx Symbol>> {
        if index >= qualified_name.len() {
            return None;
        }

        let part = qualified_name[index];
        let name_key = self.interner.intern(part);
        let filter = if index == qualified_name.len() - 1 {
            query.result_filter
        } else {
            SymbolFilter::any()
        };
        let symbols = scope.try_lookup_symbols(name_key, filter)?;

        if index == qualified_name.len() - 1 {
            return Some(symbols);
        }

        let mut results = Vec::new();
        for symbol in symbols {
            if let Some(symbol_scope_id) = symbol.opt_owned_scope()
                && let Some(next_scope) = self.arena.get_scope(symbol_scope_id.0)
            {
                debug_assert!(next_scope.id() == symbol_scope_id);
                if let Some(result) = self.try_lookup_qualified_recursive(
                    next_scope,
                    qualified_name,
                    index + 1,
                    query,
                ) {
                    results.extend(result);
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

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum InsertTarget {
    #[default]
    Current,
    Global,
}

impl InsertTarget {
    fn select<'tcx>(self, stack: &[&'tcx Scope<'tcx>]) -> Option<&'tcx Scope<'tcx>> {
        match self {
            Self::Global => stack.first().copied(),
            Self::Current => stack.last().copied(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
/// Predicate used by scope lookup to filter candidate symbols.
pub struct SymbolFilter {
    kinds: SymKindSet,
}

impl SymbolFilter {
    /// Match symbols of any kind.
    pub fn any() -> Self {
        Self::default()
    }

    /// Match symbols whose kind appears in `kinds`.
    pub fn kinds(kinds: SymKindSet) -> Self {
        Self { kinds }
    }

    fn is_empty(self) -> bool {
        self.kinds.is_empty()
    }

    fn matches(self, symbol: &Symbol) -> bool {
        self.kinds.is_empty() || self.kinds.contains(symbol.kind())
    }
}

#[derive(Debug, Clone, Copy, Default)]
/// Options controlling where lookup-or-insert writes and which existing symbols it reuses.
pub struct InsertOptions {
    insert_scope: InsertTarget,
    existing_filter: SymbolFilter,
}

impl InsertOptions {
    /// Insert into the current stack scope and reuse any existing symbol kind.
    pub fn current() -> Self {
        Self::default()
    }

    /// Insert into the global stack scope and reuse any existing symbol kind.
    pub fn global() -> Self {
        Self {
            insert_scope: InsertTarget::Global,
            ..Default::default()
        }
    }

    /// Restrict existing-symbol reuse to the provided kinds.
    pub fn with_existing_kinds(mut self, kinds: SymKindSet) -> Self {
        self.existing_filter = SymbolFilter::kinds(kinds);
        self
    }

    fn existing_filter(self) -> SymbolFilter {
        self.existing_filter
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
enum QualifiedStart {
    #[default]
    Global,
    Lexical,
}

#[derive(Debug, Clone, Copy, Default)]
/// Options for qualified path lookup.
pub struct QualifiedLookup {
    start: QualifiedStart,
    result_filter: SymbolFilter,
}

impl QualifiedLookup {
    /// Start qualified lookup from the global scope.
    pub fn global() -> Self {
        Self::default()
    }

    /// Start qualified lookup from visible lexical scopes, innermost first.
    pub fn lexical() -> Self {
        Self {
            start: QualifiedStart::Lexical,
            ..Default::default()
        }
    }

    /// Restrict the final path component to the provided symbol kinds.
    pub fn with_result_kinds(mut self, kinds: SymKindSet) -> Self {
        self.result_filter = SymbolFilter::kinds(kinds);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::SymKind;

    fn alloc_scope<'tcx>(
        arena: &'tcx Arena<'tcx>,
        interner: &'tcx InternPool,
        owner: usize,
    ) -> &'tcx Scope<'tcx> {
        let scope = Scope::new_with(HirId(owner), None, Some(interner));
        arena.alloc_with_id(scope.id().0, scope)
    }

    fn alloc_symbol<'tcx>(
        arena: &'tcx Arena<'tcx>,
        interner: &InternPool,
        name: &str,
        kind: SymKind,
        owner: usize,
    ) -> &'tcx Symbol {
        let symbol = Symbol::new(HirId(owner), interner.intern(name));
        symbol.set_kind(kind);
        arena.alloc_with_id(symbol.id().0, symbol)
    }

    fn insert_symbol<'tcx>(
        arena: &'tcx Arena<'tcx>,
        interner: &InternPool,
        scope: &Scope<'tcx>,
        name: &str,
        kind: SymKind,
        owner: usize,
    ) -> &'tcx Symbol {
        let symbol = alloc_symbol(arena, interner, name, kind, owner);
        scope.insert(symbol);
        symbol
    }

    fn symbol_ids(symbols: Vec<&Symbol>) -> Vec<SymId> {
        symbols.into_iter().map(Symbol::id).collect()
    }

    #[test]
    fn lexical_lookup_prefers_innermost_matching_scope() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let globals = alloc_scope(&arena, &interner, 0);
        let local = alloc_scope(&arena, &interner, 1);
        let stack = ScopeStack::new(&arena, &interner);

        let global_symbol =
            insert_symbol(&arena, &interner, globals, "value", SymKind::Variable, 10);
        let local_symbol = insert_symbol(&arena, &interner, local, "value", SymKind::Const, 11);

        stack.push(globals);
        stack.push(local);

        let symbols = stack
            .try_lookup_symbols("value", SymbolFilter::any())
            .expect("unfiltered lookup should find local symbol");
        assert_eq!(symbol_ids(symbols), vec![local_symbol.id()]);

        let symbols = stack
            .try_lookup_symbols(
                "value",
                SymbolFilter::kinds(SymKindSet::from_kind(SymKind::Variable)),
            )
            .expect("kind-filtered lookup should continue to outer scopes");
        assert_eq!(symbol_ids(symbols), vec![global_symbol.id()]);
    }

    #[test]
    fn lookup_or_insert_keeps_same_name_different_kinds_separate() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let globals = alloc_scope(&arena, &interner, 0);
        let stack = ScopeStack::new(&arena, &interner);
        stack.push(globals);

        let crate_symbol = stack
            .try_lookup_or_insert(
                "auth",
                HirId(1),
                InsertOptions::current().with_existing_kinds(SymKindSet::from_kind(SymKind::Crate)),
            )
            .expect("crate symbol should be inserted")
            .pop()
            .expect("insert should return symbol");
        crate_symbol.set_kind(SymKind::Crate);

        let file_symbol = stack
            .try_lookup_or_insert(
                "auth",
                HirId(2),
                InsertOptions::current().with_existing_kinds(SymKindSet::from_kind(SymKind::File)),
            )
            .expect("file symbol should be inserted")
            .pop()
            .expect("insert should return symbol");
        file_symbol.set_kind(SymKind::File);

        assert_ne!(crate_symbol.id(), file_symbol.id());
        let all_symbols = globals
            .try_lookup_symbols(interner.intern("auth"), SymbolFilter::any())
            .expect("scope should contain both symbols");
        assert_eq!(all_symbols.len(), 2);
    }

    #[test]
    fn semantic_parent_traversal_deduplicates_and_handles_cycles() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let file_scope = alloc_scope(&arena, &interner, 0);
        let module_scope = alloc_scope(&arena, &interner, 1);
        let crate_scope = alloc_scope(&arena, &interner, 2);
        let crate_symbol = alloc_symbol(&arena, &interner, "app", SymKind::Crate, 20);
        crate_scope.set_symbol(crate_symbol);

        file_scope.add_parent(file_scope);
        file_scope.add_parent(module_scope);
        file_scope.add_parent(module_scope);
        module_scope.add_parent(crate_scope);
        crate_scope.add_parent(file_scope);

        let parent_ids: Vec<_> = file_scope.parents().into_iter().map(Scope::id).collect();
        assert_eq!(parent_ids, vec![module_scope.id()]);
        assert_eq!(
            file_scope.try_parent_symbol(SymKind::Crate).map(Symbol::id),
            Some(crate_symbol.id())
        );
    }

    #[test]
    fn push_recursive_places_semantic_parents_under_requested_scope() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let file_scope = alloc_scope(&arena, &interner, 0);
        let module_scope = alloc_scope(&arena, &interner, 1);
        let crate_scope = alloc_scope(&arena, &interner, 2);
        let crate_symbol = insert_symbol(&arena, &interner, crate_scope, "app", SymKind::Crate, 10);
        file_scope.add_parent(module_scope);
        module_scope.add_parent(crate_scope);

        let stack = ScopeStack::new(&arena, &interner);
        stack.push_recursive(file_scope);

        assert_eq!(stack.depth(), 3);
        assert_eq!(stack.try_current().map(Scope::id), Some(file_scope.id()));
        let symbols = stack
            .try_lookup_symbols("app", SymbolFilter::any())
            .expect("recursive stack should expose semantic parents");
        assert_eq!(symbol_ids(symbols), vec![crate_symbol.id()]);
    }

    #[test]
    fn qualified_lookup_applies_kind_filter_only_to_final_component() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let globals = alloc_scope(&arena, &interner, 0);
        let module_scope = alloc_scope(&arena, &interner, 1);
        let nested_scope = alloc_scope(&arena, &interner, 2);
        let stack = ScopeStack::new(&arena, &interner);

        let module = insert_symbol(&arena, &interner, globals, "models", SymKind::Module, 10);
        module.set_owned_scope(module_scope.id());
        let nested = insert_symbol(&arena, &interner, module_scope, "v1", SymKind::Module, 11);
        nested.set_owned_scope(nested_scope.id());
        let user = insert_symbol(&arena, &interner, nested_scope, "User", SymKind::Struct, 12);

        stack.push(globals);
        let symbols = stack
            .try_lookup_qualified(
                &["models", "v1", "User"],
                QualifiedLookup::global().with_result_kinds(SymKindSet::from_kind(SymKind::Struct)),
            )
            .expect("module path should resolve to final struct");
        assert_eq!(symbol_ids(symbols), vec![user.id()]);
    }

    #[test]
    fn qualified_lookup_can_start_from_current_stack_scope() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let globals = alloc_scope(&arena, &interner, 0);
        let local = alloc_scope(&arena, &interner, 1);
        let local_namespace = alloc_scope(&arena, &interner, 2);
        let stack = ScopeStack::new(&arena, &interner);

        let namespace = insert_symbol(&arena, &interner, local, "local_ns", SymKind::Namespace, 10);
        namespace.set_owned_scope(local_namespace.id());
        let item = insert_symbol(
            &arena,
            &interner,
            local_namespace,
            "Item",
            SymKind::Struct,
            11,
        );

        stack.push(globals);
        stack.push(local);
        let symbols = stack
            .try_lookup_qualified(
                &["local_ns", "Item"],
                QualifiedLookup::lexical()
                    .with_result_kinds(SymKindSet::from_kind(SymKind::Struct)),
            )
            .expect("shifted qualified lookup should start from local scope");
        assert_eq!(symbol_ids(symbols), vec![item.id()]);
    }

    #[test]
    fn qualified_lookup_prefers_innermost_start_scope() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let globals = alloc_scope(&arena, &interner, 0);
        let local = alloc_scope(&arena, &interner, 1);
        let global_namespace = alloc_scope(&arena, &interner, 2);
        let local_namespace = alloc_scope(&arena, &interner, 3);
        let stack = ScopeStack::new(&arena, &interner);

        let global_ns = insert_symbol(&arena, &interner, globals, "ns", SymKind::Namespace, 10);
        global_ns.set_owned_scope(global_namespace.id());
        insert_symbol(
            &arena,
            &interner,
            global_namespace,
            "Item",
            SymKind::Struct,
            11,
        );

        let local_ns = insert_symbol(&arena, &interner, local, "ns", SymKind::Namespace, 12);
        local_ns.set_owned_scope(local_namespace.id());
        let local_item = insert_symbol(
            &arena,
            &interner,
            local_namespace,
            "Item",
            SymKind::Struct,
            13,
        );

        stack.push(globals);
        stack.push(local);
        let symbols = stack
            .try_lookup_qualified(
                &["ns", "Item"],
                QualifiedLookup::lexical()
                    .with_result_kinds(SymKindSet::from_kind(SymKind::Struct)),
            )
            .expect("relative qualified lookup should prefer the local namespace");
        assert_eq!(symbol_ids(symbols), vec![local_item.id()]);
    }

    #[test]
    fn qualified_lookup_falls_back_when_inner_start_does_not_complete_path() {
        let arena = Arena::new();
        let interner = InternPool::new();
        let globals = alloc_scope(&arena, &interner, 0);
        let local = alloc_scope(&arena, &interner, 1);
        let global_namespace = alloc_scope(&arena, &interner, 2);
        let stack = ScopeStack::new(&arena, &interner);

        let global_ns = insert_symbol(&arena, &interner, globals, "ns", SymKind::Namespace, 10);
        global_ns.set_owned_scope(global_namespace.id());
        let global_item = insert_symbol(
            &arena,
            &interner,
            global_namespace,
            "Item",
            SymKind::Struct,
            11,
        );
        insert_symbol(&arena, &interner, local, "ns", SymKind::Variable, 12);

        stack.push(globals);
        stack.push(local);
        let symbols = stack
            .try_lookup_qualified(
                &["ns", "Item"],
                QualifiedLookup::lexical()
                    .with_result_kinds(SymKindSet::from_kind(SymKind::Struct)),
            )
            .expect("outer namespace should be used when local segment has no owned scope");
        assert_eq!(symbol_ids(symbols), vec![global_item.id()]);
    }
}
