use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::ir::{Arena, HirId, HirIdent};

static NEXT_SYMBOL_ID: AtomicU32 = AtomicU32::new(1);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SymId(pub u32);

impl std::fmt::Display for SymId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct Scope<'tcx> {
    definitions: RefCell<HashMap<SymId, &'tcx Symbol<'tcx>>>,
    lookup: RefCell<HashMap<String, SymId>>,
    owner: HirId,
}

impl<'tcx> Scope<'tcx> {
    pub fn new(owner: HirId) -> Self {
        Self {
            definitions: RefCell::new(HashMap::new()),
            lookup: RefCell::new(HashMap::new()),
            owner,
        }
    }

    pub fn owner(&self) -> HirId {
        self.owner
    }

    pub fn insert(&self, name: &str, symbol: &'tcx Symbol<'tcx>) -> SymId {
        let sym_id = symbol.id;
        self.definitions.borrow_mut().insert(sym_id, symbol);
        self.lookup.borrow_mut().insert(name.to_string(), sym_id);
        sym_id
    }

    pub fn get_id(&self, name: &str) -> Option<SymId> {
        self.lookup.borrow().get(name).copied()
    }

    pub fn get_symbol(&self, sym_id: SymId) -> Option<&'tcx Symbol<'tcx>> {
        self.definitions.borrow().get(&sym_id).copied()
    }

    pub fn with_symbol<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Symbol<'tcx>) -> R,
    {
        let id = self.get_id(name)?;
        self.with_symbol_by_id(id, f)
    }

    pub fn with_symbol_by_id<F, R>(&self, id: SymId, f: F) -> Option<R>
    where
        F: FnOnce(&Symbol<'tcx>) -> R,
    {
        let defs = self.definitions.borrow();
        defs.get(&id).map(|symbol| f(*symbol))
    }

    pub fn format_compact(&self) -> String {
        let defs = self.definitions.borrow();
        format!("{}/{}", self.owner, defs.len())
    }
}

#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    arena: &'tcx Arena<'tcx>,
    stack: Vec<&'tcx Scope<'tcx>>,
}

impl<'tcx> ScopeStack<'tcx> {
    pub fn new(arena: &'tcx Arena<'tcx>) -> Self {
        Self {
            arena,
            stack: Vec::new(),
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn push(&mut self, scope: &'tcx Scope<'tcx>) {
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
        name: &str,
        symbol: &'tcx Symbol<'tcx>,
        global: bool,
    ) -> Result<SymId, &'static str> {
        let scope = self.scope_for_insertion(global)?;
        Ok(scope.insert(name, symbol))
    }

    pub fn find_symbol_id(&self, name: &str) -> Option<SymId> {
        self.stack.iter().rev().find_map(|scope| scope.get_id(name))
    }

    pub fn find_symbol(&self, name: &str) -> Option<&'tcx Symbol<'tcx>> {
        self.stack
            .iter()
            .rev()
            .find_map(|scope| scope.get_id(name))
            .and_then(|id| self.find_symbol_by_id(id))
    }

    pub fn find_symbol_by_id(&self, id: SymId) -> Option<&'tcx Symbol<'tcx>> {
        self.stack
            .iter()
            .rev()
            .find_map(|scope| scope.get_symbol(id))
    }

    pub fn find_or_insert(
        &mut self,
        owner: HirId,
        ident: &HirIdent<'tcx>,
        global: bool,
    ) -> &'tcx Symbol<'tcx> {
        if let Some(symbol) = self.find_ident(ident) {
            return symbol;
        }

        let symbol = self.create_symbol(owner, ident);
        self.insert_symbol(&ident.name, symbol, global)
            .expect("failed to insert symbol");
        self.find_ident(ident)
            .expect("symbol should be present after insertion")
    }

    pub fn find_or_insert_local(
        &mut self,
        owner: HirId,
        ident: &HirIdent<'tcx>,
    ) -> &'tcx Symbol<'tcx> {
        self.find_or_insert(owner, ident, false)
    }

    pub fn find_or_insert_global(
        &mut self,
        owner: HirId,
        ident: &HirIdent<'tcx>,
    ) -> &'tcx Symbol<'tcx> {
        self.find_or_insert(owner, ident, true)
    }

    fn create_symbol(&self, owner: HirId, ident: &HirIdent<'tcx>) -> &'tcx Symbol<'tcx> {
        let symbol = Symbol::new(owner, ident.name.clone());
        self.arena.alloc(symbol)
    }

    pub fn find_ident(&self, ident: &HirIdent<'tcx>) -> Option<&'tcx Symbol<'tcx>> {
        self.find_symbol(&ident.name)
    }
}

#[derive(Debug, Clone)]
pub struct Symbol<'tcx> {
    pub id: SymId,
    pub owner: HirId,
    pub name: String,
    pub fqn_name: RefCell<String>,
    pub defined: Cell<Option<HirId>>,
    pub type_of: Cell<Option<SymId>>,
    pub field_of: Cell<Option<SymId>>,
    pub base_symbol: Cell<Option<SymId>>,
    pub overloads: RefCell<Vec<SymId>>,
    pub nested_types: RefCell<Vec<SymId>>,
    _marker: PhantomData<&'tcx ()>,
}

impl<'tcx> Symbol<'tcx> {
    pub fn new(owner: HirId, name: String) -> Self {
        let next = NEXT_SYMBOL_ID.fetch_add(1, Ordering::SeqCst);
        let sym_id = SymId(next);

        Self {
            id: sym_id,
            owner,
            name: name.clone(),
            fqn_name: RefCell::new(name),
            defined: Cell::new(None),
            type_of: Cell::new(None),
            field_of: Cell::new(None),
            base_symbol: Cell::new(None),
            overloads: RefCell::new(Vec::new()),
            nested_types: RefCell::new(Vec::new()),
            _marker: PhantomData,
        }
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
        if let Some(base_symbol) = self.base_symbol.get() {
            info.push(format!("&{}", base_symbol));
        }

        let overloads = self.overloads.borrow().len();
        if overloads > 0 {
            info.push(format!("+{}", overloads));
        }

        let nested = self.nested_types.borrow().len();
        if nested > 0 {
            info.push(format!("*{}", nested));
        }

        let meta = if info.is_empty() {
            String::new()
        } else {
            format!(" ({})", info.join(" "))
        };

        format!("{}->{} \"{}\"{}", self.id, self.owner, self.name, meta)
    }
}
