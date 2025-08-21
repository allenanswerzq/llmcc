use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};
use std::{collections::HashMap, marker::PhantomData};

use crate::ir::{Arena, HirId, HirIdent};

static NEXT_SYMBOL_ID: AtomicU32 = AtomicU32::new(0);

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug, Default)]
pub struct SymId(pub u32);

#[derive(Debug)]
pub struct Scope<'tcx> {
    pub symbol_defs: HashMap<SymId, &'tcx Symbol<'tcx>>,
    pub symbol_map: HashMap<String, SymId>,
    pub owner: HirId,
}

impl<'tcx> Scope<'tcx> {
    pub fn new(owner: HirId) -> Self {
        Self {
            symbol_defs: HashMap::new(),
            symbol_map: HashMap::new(),
            owner,
        }
    }

    pub fn insert_symbol(&mut self, name: String, symbol: &'tcx Symbol<'tcx>) -> SymId {
        let sym_id = symbol.id;
        self.symbol_defs.insert(sym_id, symbol);
        self.symbol_map.insert(name, sym_id);
        sym_id
    }

    pub fn find_symbol_id(&self, name: &str) -> Option<SymId> {
        self.symbol_map.get(name).copied()
    }

    pub fn find_symbol(&self, name: &str) -> Option<&'tcx Symbol<'tcx>> {
        self.symbol_map
            .get(name)
            .and_then(|sym_id| self.symbol_defs.get(sym_id))
            .map(|s| &**s)
    }

    pub fn get_symbol(&self, sym_id: SymId) -> Option<&Symbol<'tcx>> {
        self.symbol_defs.get(&sym_id).map(|s| &**s)
    }
}

#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    pub arena: &'tcx Arena<'tcx>,
    pub scopes: Vec<Scope<'tcx>>,
}

impl<'tcx> ScopeStack<'tcx> {
    pub fn new(arena: &'tcx Arena<'tcx>) -> Self {
        Self {
            arena,
            scopes: Vec::new(),
        }
    }

    pub fn depth(&self) -> usize {
        self.scopes.len()
    }

    pub fn push_scope(&mut self, owner: HirId) {
        self.scopes.push(Scope::new(owner));
    }

    pub fn pop_scope(&mut self) -> Option<Scope<'tcx>> {
        self.scopes.pop()
    }

    pub fn pop_until(&mut self, depth: usize) {
        while self.depth() > depth {
            self.pop_scope();
        }
    }

    pub fn top(&self) -> Option<&Scope<'tcx>> {
        self.scopes.last()
    }

    pub fn top_mut(&mut self) -> Option<&mut Scope<'tcx>> {
        self.scopes.last_mut()
    }

    pub fn insert_symbol(
        &mut self,
        name: String,
        symbol: &'tcx Symbol<'tcx>,
    ) -> Result<SymId, &'static str> {
        if let Some(scope) = self.scopes.last_mut() {
            Ok(scope.insert_symbol(name, symbol))
        } else {
            Err("No scope available to insert symbol")
        }
    }

    pub fn find_symbol_id(&self, name: &str) -> Option<SymId> {
        for scope in self.scopes.iter().rev() {
            if let Some(sym_id) = scope.find_symbol_id(name) {
                return Some(sym_id);
            }
        }
        None
    }

    pub fn find(&self, ident: &HirIdent<'tcx>) -> Option<&'tcx Symbol<'tcx>> {
        let name = &ident.name;
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.find_symbol(name) {
                return Some(symbol);
            }
        }
        None
    }

    pub fn find_by_id(&self, sym_id: SymId) -> Option<&Symbol<'tcx>> {
        for scope in self.scopes.iter().rev() {
            if let Some(symbol) = scope.get_symbol(sym_id) {
                return Some(symbol);
            }
        }
        None
    }

    pub fn find_or_add(&mut self, id: HirId, ident: &HirIdent<'tcx>) -> &'tcx Symbol<'tcx> {
        if self.find(ident).is_some() {
            return self.find(ident).unwrap();
        }

        let symbol = Symbol::new(self.arena, id, ident.name.clone());
        self.insert_symbol(ident.name.clone(), symbol);
        self.find(ident).unwrap()
    }
}

#[derive(Debug)]
pub struct Symbol<'tcx> {
    pub id: SymId,
    pub owner: HirId,
    pub name: String,
    pub mangled_name: RefCell<String>,
    pub defined: Cell<Option<HirId>>,
    pub type_of: Cell<Option<SymId>>,
    pub field_of: Cell<Option<SymId>>,
    pub base_symbol: Cell<Option<SymId>>,
    pub overloads: RefCell<Vec<SymId>>,
    pub nested_types: RefCell<Vec<SymId>>,
    ph: PhantomData<&'tcx ()>,
}

impl<'tcx> Symbol<'tcx> {
    pub fn new(arena: &'tcx Arena<'tcx>, owner: HirId, name: String) -> &'tcx Symbol<'tcx> {
        let id = NEXT_SYMBOL_ID.load(Ordering::SeqCst);
        let sym_id = SymId(id);
        NEXT_SYMBOL_ID.store(id + 1, Ordering::SeqCst);

        let symbol = Symbol {
            id: sym_id,
            owner,
            name: name.clone(),
            mangled_name: RefCell::new(name),
            defined: Cell::new(None),
            type_of: Cell::new(None),
            field_of: Cell::new(None),
            base_symbol: Cell::new(None),
            overloads: RefCell::new(Vec::new()),
            nested_types: RefCell::new(Vec::new()),
            ph: PhantomData,
        };

        arena.alloc(symbol)
    }
}
