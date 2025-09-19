use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicU32, Ordering};
use std::{collections::HashMap, marker::PhantomData};

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
    pub symbol_defs: RefCell<HashMap<SymId, &'tcx Symbol<'tcx>>>,
    pub symbol_map: RefCell<HashMap<String, SymId>>,
    pub owner: HirId,
}

impl<'tcx> Scope<'tcx> {
    pub fn format_compact(&self) -> String {
        let symbol_defs = self.symbol_defs.borrow();
        let symbol_map = self.symbol_map.borrow();

        format!("{}/{}", self.owner, symbol_defs.len(),)
    }

    pub fn new(owner: HirId) -> Self {
        Self {
            symbol_defs: RefCell::new(HashMap::new()),
            symbol_map: RefCell::new(HashMap::new()),
            owner,
        }
    }

    pub fn insert_symbol(&self, name: String, symbol: &'tcx Symbol<'tcx>) -> SymId {
        let sym_id = symbol.id;
        self.symbol_defs.borrow_mut().insert(sym_id, symbol);
        self.symbol_map.borrow_mut().insert(name, sym_id);
        sym_id
    }

    pub fn find_symbol_id(&self, name: &str) -> Option<SymId> {
        self.symbol_map.borrow().get(name).copied()
    }

    pub fn find_symbol(&self, name: &str) -> Option<SymId> {
        // Return the SymId instead, then use with_symbol for access
        self.symbol_map.borrow().get(name).copied()
    }

    pub fn get_symbol(&self, sym_id: SymId) -> Option<&'tcx Symbol<'tcx>> {
        // This still has the borrow issue. Better to use with_symbol pattern.
        self.symbol_defs.borrow().get(&sym_id).copied()
    }

    // Alternative: Use a closure-based API to work with symbols
    pub fn with_symbol<F, R>(&self, name: &str, f: F) -> Option<R>
    where
        F: FnOnce(&Symbol<'tcx>) -> R,
    {
        let sym_id = self.symbol_map.borrow().get(name).copied()?;
        let symbol_defs = self.symbol_defs.borrow();
        symbol_defs.get(&sym_id).map(|symbol| f(symbol))
    }

    pub fn with_symbol_by_id<F, R>(&self, sym_id: SymId, f: F) -> Option<R>
    where
        F: FnOnce(&Symbol<'tcx>) -> R,
    {
        let symbol_defs = self.symbol_defs.borrow();
        symbol_defs.get(&sym_id).map(|symbol| f(symbol))
    }
}

#[derive(Debug)]
pub struct ScopeStack<'tcx> {
    pub arena: &'tcx Arena<'tcx>,
    pub scopes: Vec<&'tcx Scope<'tcx>>,
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

    pub fn push_scope(&mut self, scope: &'tcx Scope<'tcx>) {
        self.scopes.push(scope);
    }

    pub fn pop_scope(&mut self) -> Option<&'tcx Scope<'tcx>> {
        self.scopes.pop()
    }

    pub fn pop_until(&mut self, depth: usize) {
        while self.depth() > depth {
            self.pop_scope();
        }
    }

    pub fn top(&self) -> Option<&'tcx Scope<'tcx>> {
        self.scopes.last().map(|s| *s)
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
            if let Some(sym_id) = scope.find_symbol(name) {
                return scope.get_symbol(sym_id);
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

        let symbol = Symbol::new(id, ident.name.clone());
        let symbol = self.arena.alloc(symbol);
        self.insert_symbol(ident.name.clone(), symbol);
        self.find(ident).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Symbol<'tcx> {
    pub id: SymId,
    pub owner: HirId,
    pub name: String,
    pub fqn_name: RefCell<String>,
    // Which node defined this symbol
    pub defined: Cell<Option<HirId>>,
    pub type_of: Cell<Option<SymId>>,
    pub field_of: Cell<Option<SymId>>,
    pub base_symbol: Cell<Option<SymId>>,
    pub overloads: RefCell<Vec<SymId>>,
    pub nested_types: RefCell<Vec<SymId>>,
    ph: PhantomData<&'tcx ()>,
}

impl<'tcx> Symbol<'tcx> {
    pub fn new(owner: HirId, name: String) -> Self {
        let id = NEXT_SYMBOL_ID.load(Ordering::SeqCst);
        let sym_id = SymId(id);
        NEXT_SYMBOL_ID.store(id + 1, Ordering::SeqCst);

        Symbol {
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
            ph: PhantomData,
        }
    }

    pub fn format_compact(&self) -> String {
        let mut info = Vec::new();

        if let Some(defined) = self.defined.get() {
            info.push(format!("#{},", defined));
        }

        if let Some(type_of) = self.type_of.get() {
            info.push(format!("@{},", type_of));
        }

        if let Some(field_of) = self.field_of.get() {
            info.push(format!("${},", field_of));
        }

        if let Some(base_symbol) = self.base_symbol.get() {
            info.push(format!("&{},", base_symbol));
        }

        let overloads_count = self.overloads.borrow().len();
        if overloads_count > 0 {
            info.push(format!("+{},", overloads_count));
        }

        let nested_count = self.nested_types.borrow().len();
        if nested_count > 0 {
            info.push(format!("*{}", nested_count));
        }

        let info_str = if info.is_empty() {
            String::new()
        } else {
            format!(" {}", info.join(" "))
        };

        format!("{}->{} \"{}\"{}", self.id, self.owner, self.name, info_str)
    }
}
