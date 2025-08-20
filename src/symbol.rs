use std::collections::HashMap;

use crate::{
    arena::{HirArena, NodeId, ScopeId, SymbolId},
    ir::HirIdPtr,
};

#[derive(Debug, Clone)]
pub struct Scope {
    // The symbol defines this scope
    pub owner: Option<SymbolId>,
    // Parent scope ID
    pub parent: Option<ScopeId>,
    // all symbols in this scope
    pub symbols: HashMap<String, SymbolId>,
    // The ast node owns this scope
    pub ast_node: Option<NodeId>,
}

impl Scope {
    pub fn new(arena: &mut HirArena, owner: Option<SymbolId>) -> ScopeId {
        let scope = Scope {
            owner,
            parent: None,
            symbols: HashMap::new(),
            ast_node: None,
        };
        arena.add_scope(scope)
    }

    pub fn add_symbol(&mut self, name: String, symbol_id: SymbolId) {
        self.symbols.insert(name, symbol_id);
    }
}

#[derive(Debug)]
pub struct ScopeStack {
    scopes: Vec<ScopeId>,
    current_scope: ScopeId,
}

impl ScopeStack {
    pub fn new(root_scope: ScopeId) -> Self {
        Self {
            scopes: vec![root_scope],
            current_scope: root_scope,
        }
    }

    pub fn scope_depth(&self) -> usize {
        self.scopes.len()
    }

    pub fn pop_until(&mut self, depth: usize) {
        while self.scope_depth() > depth {
            self.leave_scope();
        }
    }

    pub fn reset_stack(&mut self, root_scope: ScopeId) {
        self.current_scope = root_scope;
        self.scopes.clear();
        self.scopes.push(root_scope);
    }

    pub fn enter_scope(&mut self, arena: &mut HirArena, scope_id: ScopeId) {
        {
            if let Some(scope) = arena.get_scope_mut(scope_id) {
                scope.parent = Some(self.current_scope);
            }
        }

        self.scopes.push(scope_id);
        self.current_scope = scope_id;
    }

    pub fn leave_scope(&mut self) {
        if self.scopes.len() <= 1 {
            panic!("already at root scope");
        }

        self.scopes.pop();
        self.current_scope = self.scopes[self.scopes.len() - 1];
    }

    pub fn find_or_add(&mut self, arena: &mut HirArena, node: HirIdPtr) -> SymbolId {
        if let Some(id) = self.lookup(arena, &node) {
            id
        } else {
            let id = node.borrow().symbol;
            self.add_symbol(arena, id);
            id
        }
    }

    pub fn find(&mut self, arena: &mut HirArena, node: HirIdPtr) -> Option<SymbolId> {
        if let Some(id) = self.lookup(arena, &node) {
            Some(id)
        } else {
            None
        }
    }

    pub fn add_symbol(&mut self, arena: &mut HirArena, symbol_id: SymbolId) {
        let symbol_name = if let Some(symbol) = arena.get_symbol(symbol_id) {
            symbol.name.clone()
        } else {
            return;
        };

        if let Some(scope) = arena.get_scope_mut(self.current_scope) {
            scope.add_symbol(symbol_name, symbol_id);
        }
    }

    pub fn lookup(&self, arena: &mut HirArena, node: &HirIdPtr) -> Option<SymbolId> {
        let name = node.borrow().get_symbol_name(arena);
        let mut current_scope_id = self.current_scope;

        loop {
            if let Some(scope) = arena.get_scope(current_scope_id) {
                if let Some(&symbol_id) = scope.symbols.get(&name) {
                    return Some(symbol_id);
                }

                if let Some(parent_id) = scope.parent {
                    current_scope_id = parent_id;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        None
    }
}

#[derive(Debug, Clone)]
struct Field {
    value: u16,
}

#[derive(Debug, Clone, Default)]
struct Token {
    value: u16,
}

impl Token {
    fn new(id: u16) -> Self {
        Token { value: id }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Symbol {
    // the node owner this sybmol,
    pub owner: NodeId,
    // the id where this symbol is stored at arena,
    pub id: SymbolId,
    // Token identifier
    pub token_id: Token,
    // The name of the symbol
    pub name: String,
    // full mangled name, used for resolve symbols overloads etc
    pub mangled_name: String,
    // The typed name, for different funcion with same name etcc
    // pub typed_name: TypedName,
    // The point from the source code
    // pub origin: Point,
    // The ast node that defines this symbol
    pub defined: Option<NodeId>,
    // The type of this symbol, if any
    pub type_of: Option<SymbolId>,
    // The field this symbol belongs to, if any
    pub field_of: Option<SymbolId>,
    // The base this symbol derived from, if any
    pub base_symbol: Option<SymbolId>,
    // All overloads for this symbol, if exists
    pub overloads: Vec<SymbolId>,
    // The list of nested types inside this symbol
    pub nested_types: Vec<SymbolId>,
}

impl std::fmt::Display for Symbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "({:?}:{},{})",
            self.token_id, self.name, self.mangled_name
        )
    }
}

impl Symbol {
    pub fn new(arena: &mut HirArena, token_id: u16, name: String, owner: NodeId) -> SymbolId {
        let id = arena.get_next_symbol_id();
        let symbol = Symbol {
            token_id: Token::new(token_id),
            name,
            id,
            owner,
            ..Default::default()
        };
        arena.add_symbol(symbol)
    }
}
