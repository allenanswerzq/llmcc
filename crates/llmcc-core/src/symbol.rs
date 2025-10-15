use std::cell::{Cell, RefCell};

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

/// Canonical representation of an item bound in a scope (functions, variables, types, etc.).
#[derive(Debug)]
pub struct Scope<'tcx> {
    trie: RefCell<SymbolTrie<'tcx>>,
    owner: HirId,
    symbol: Cell<Option<&'tcx Symbol>>,
}

impl<'tcx> Scope<'tcx> {
    pub fn new(owner: HirId) -> Self {
        Self {
            trie: RefCell::new(SymbolTrie::default()),
            owner,
            symbol: Cell::new(None),
        }
    }

    pub fn owner(&self) -> HirId {
        self.owner
    }

    pub fn symbol(&self) -> Option<&'tcx Symbol> {
        self.symbol.get()
    }

    pub fn set_symbol(&self, symbol: Option<&'tcx Symbol>) {
        self.symbol.set(symbol);
    }

    pub fn insert(&self, _key: InternedStr, symbol: &'tcx Symbol, interner: &InternPool) -> SymId {
        let sym_id = symbol.id;
        self.trie.borrow_mut().insert_symbol(symbol, interner);
        sym_id
    }

    pub fn get_id(&self, key: InternedStr) -> Option<SymId> {
        let hits = self.trie.borrow().lookup_symbol_suffix(&[key]);
        hits.first().map(|symbol| symbol.id)
    }

    pub fn format_compact(&self) -> String {
        let count = self.trie.borrow().total_symbols();
        format!("{}/{}", self.owner, count)
    }
}

#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    arena: &'tcx Arena<'tcx>,
    interner: &'tcx InternPool,
    stack: Vec<&'tcx Scope<'tcx>>,
}

impl<'tcx> ScopeStack<'tcx> {
    pub fn new(arena: &'tcx Arena<'tcx>, interner: &'tcx InternPool) -> Self {
        Self {
            arena,
            interner,
            stack: Vec::new(),
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
        let popped = self.stack.pop();
        if let Some(scope) = popped {
            scope.set_symbol(None);
        }
        popped
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

    fn scope_for_insertion(&mut self, global: bool) -> Result<&'tcx Scope<'tcx>, &'static str> {
        if global {
            self.stack.get(0).copied().ok_or("no global scope exists")
        } else {
            self.stack
                .last()
                .copied()
                .ok_or("no active scope available")
        }
    }

    pub fn insert_symbol(
        &mut self,
        key: InternedStr,
        symbol: &'tcx Symbol,
        global: bool,
    ) -> Result<SymId, &'static str> {
        let scope = self.scope_for_insertion(global)?;
        Ok(scope.insert(key, symbol, self.interner))
    }

    pub fn find_symbol_id(&self, name: &str) -> Option<SymId> {
        let key = self.interner.intern(name);
        self.iter().rev().find_map(|scope| scope.get_id(key))
    }

    fn find_symbol_local_by_key(&self, key: InternedStr) -> Option<&'tcx Symbol> {
        self.iter().rev().find_map(|scope| {
            scope
                .trie
                .borrow()
                .lookup_symbol_suffix(&[key])
                .into_iter()
                .next()
        })
    }

    fn find_symbol_local(&self, name: &str) -> Option<&'tcx Symbol> {
        let key = self.interner.intern(name);
        self.find_symbol_local_by_key(key)
    }

    pub fn find_global_suffix(&self, suffix: &[InternedStr]) -> Vec<&'tcx Symbol> {
        self.stack
            .first()
            .map(|scope| scope.trie.borrow().lookup_symbol_suffix(suffix))
            .unwrap_or_default()
    }

    pub fn find_global_suffix_once(&self, suffix: &[InternedStr]) -> Option<&'tcx Symbol> {
        self.find_global_suffix(suffix).into_iter().next()
    }

    pub fn find_ident(&self, ident: &HirIdent<'tcx>) -> Option<&'tcx Symbol> {
        self.find_symbol_local(&ident.name)
    }

    pub fn find_or_insert(
        &mut self,
        owner: HirId,
        ident: &HirIdent<'tcx>,
        global: bool,
    ) -> &'tcx Symbol {
        self.find_or_insert_with(owner, ident, global, |_| {})
    }

    pub fn find_or_insert_with<F>(
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

        let symbol = if let Some(existing) = self.find_ident_local(ident) {
            init(existing);
            existing
        } else {
            let symbol = self.create_symbol(owner, ident, key);
            init(symbol);
            symbol
        };

        self.insert_symbol(key, symbol, false)
            .expect("failed to insert symbol into scope");
        if global {
            self.insert_symbol(key, symbol, true)
                .expect("failed to insert symbol into global scope");
        }

        self.find_symbol_local_by_key(key)
            .expect("symbol should be present after insertion")
    }

    pub fn find_or_insert_local(&mut self, owner: HirId, ident: &HirIdent<'tcx>) -> &'tcx Symbol {
        self.find_or_insert(owner, ident, false)
    }

    pub fn find_or_insert_global(&mut self, owner: HirId, ident: &HirIdent<'tcx>) -> &'tcx Symbol {
        self.find_or_insert(owner, ident, true)
    }

    fn create_symbol(
        &self,
        owner: HirId,
        ident: &HirIdent<'tcx>,
        key: InternedStr,
    ) -> &'tcx Symbol {
        let symbol = Symbol::new(owner, ident.name.clone(), key);
        self.arena.alloc(symbol)
    }

    fn find_ident_local(&self, ident: &HirIdent<'tcx>) -> Option<&'tcx Symbol> {
        self.find_symbol_local(&ident.name)
    }
}

/// Canonical representation of an item bound in a scope (functions, variables, types, etc.).
#[derive(Debug, Clone)]
pub struct Symbol {
    /// Monotonic identifier assigned when the symbol is created.
    pub id: SymId,
    /// Owning HIR node that introduces the symbol (e.g. function, struct, module).
    pub owner: Cell<HirId>,
    /// Unqualified name exactly as written in source.
    pub name: String,
    /// Interned key for `name`, used for fast lookup.
    pub name_key: InternedStr,
    /// Fully qualified name cached as a string (updated as scopes are resolved).
    pub fqn_name: RefCell<String>,
    /// Interned key for the fully qualified name.
    pub fqn_key: RefCell<InternedStr>,
    /// HIR node where the symbol definition appears (`None` until resolved).
    pub defined: Cell<Option<HirId>>,
    /// `SymId` of the type describing this symbol (e.g. variable type), if any.
    pub type_of: Cell<Option<SymId>>,
    /// If this symbol is a field, the `SymId` of the aggregate that owns it.
    pub field_of: Cell<Option<SymId>>,
}

impl Symbol {
    pub fn new(owner: HirId, name: String, name_key: InternedStr) -> Self {
        let id = NEXT_SYMBOL_ID.fetch_add(1, Ordering::SeqCst);
        let sym_id = SymId(id);

        let fqn_key = name_key;

        Self {
            id: sym_id,
            owner: Cell::new(owner),
            name: name.clone(),
            name_key,
            fqn_name: RefCell::new(name),
            fqn_key: RefCell::new(fqn_key),
            defined: Cell::new(None),
            type_of: Cell::new(None),
            field_of: Cell::new(None),
        }
    }

    pub fn owner(&self) -> HirId {
        self.owner.get()
    }

    pub fn set_owner(&self, owner: HirId) {
        self.owner.set(owner);
    }

    pub fn format_compact(&self) -> String {
        let mut info = Vec::new();

        if let Some(defined) = self.defined.get() {
            info.push(format!("#{}", defined));
        }
        if let Some(type_of) = self.type_of.get() {
            info.push(format!("@{}", type_of));
        }
        if let Some(field_of) = self.field_of.get() {
            info.push(format!("${}", field_of));
        }

        let meta = if info.is_empty() {
            String::new()
        } else {
            format!(" ({})", info.join(" "))
        };

        format!(
            "{}->{} \"{}\"{}",
            self.id,
            self.owner.get(),
            self.name,
            meta
        )
    }

    pub fn set_fqn(&self, fqn: String, interner: &InternPool) {
        let key = interner.intern(&fqn);
        *self.fqn_name.borrow_mut() = fqn;
        *self.fqn_key.borrow_mut() = key;
    }
}
