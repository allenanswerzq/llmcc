use crate::arena::{ArenaIdNode, ArenaIdScope, ArenaIdSymbol, IrArena};

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Scope {
    // The symbol defines this scope
    pub owner: ArenaIdSymbol,
    // Parent scope ID
    pub parent: Option<ArenaIdScope>,
    // all symbols in this scope
    pub symbols: HashMap<String, ArenaIdSymbol>,
    // The ast node owns this scope
    pub ast_node: Option<ArenaIdNode>,
}

impl Scope {
    pub fn new(arena: &mut IrArena, owner: ArenaIdSymbol) -> ArenaIdScope {
        let scope = Scope {
            owner,
            parent: None,
            symbols: HashMap::new(),
            ast_node: None,
        };
        arena.add_scope(scope)
    }

    pub fn add_symbol(&mut self, name: String, symbol_id: ArenaIdSymbol) {
        self.symbols.insert(name, symbol_id);
    }
}

#[derive(Debug)]
pub struct ScopeStack {
    scopes: Vec<ArenaIdScope>,
    current_scope: ArenaIdScope,
}

impl ScopeStack {
    fn new(root_scope: ArenaIdScope) -> Self {
        Self {
            scopes: vec![root_scope],
            current_scope: root_scope,
        }
    }

    fn reset_stack(&mut self, root_scope: ArenaIdScope) {
        self.current_scope = root_scope;
        self.scopes.clear();
        self.scopes.push(root_scope);
    }

    fn enter_scope(&mut self, arena: &mut IrArena, scope_id: ArenaIdScope) {
        // Set parent relationship
        {
            if let Some(scope) = arena.get_scope_mut(scope_id) {
                scope.parent = Some(self.current_scope);
            }
        }

        self.scopes.push(scope_id);
        self.current_scope = scope_id;
    }

    fn leave_scope(&mut self) {
        if self.scopes.len() <= 1 {
            panic!("already at root scope");
        }

        self.scopes.pop();
        self.current_scope = self.scopes[self.scopes.len() - 1];
    }

    fn add_symbol(&mut self, arena: &mut IrArena, symbol_id: ArenaIdSymbol) {
        // Get the symbol name
        let symbol_name = if let Some(symbol) = arena.get_symbol(symbol_id) {
            symbol.name.clone()
        } else {
            return; // Symbol not found
        };

        // Add to current scope
        if let Some(scope) = arena.get_scope_mut(self.current_scope) {
            scope.add_symbol(symbol_name, symbol_id);
        }
    }

    fn lookup(&self, arena: &mut IrArena, name: &str) -> Option<ArenaIdSymbol> {
        let mut current_scope_id = self.current_scope;

        loop {
            if let Some(scope) = arena.get_scope(current_scope_id) {
                if let Some(&symbol_id) = scope.symbols.get(name) {
                    return Some(symbol_id);
                }

                // Move to parent scope
                if let Some(parent_id) = scope.parent {
                    current_scope_id = parent_id;
                } else {
                    break; // No parent, we're at root
                }
            } else {
                break; // Scope not found
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
    pub defined: Option<ArenaIdNode>,
    // The type of this symbol, if any
    pub type_of: Option<ArenaIdSymbol>,
    // The field this symbol belongs to, if any
    pub field_of: Option<ArenaIdSymbol>,
    // The base this symbol derived from, if any
    pub base_symbol: Option<ArenaIdSymbol>,
    // All overloads for this symbol, if exists
    pub overloads: Vec<ArenaIdSymbol>,
    // The list of nested types inside this symbol
    pub nested_types: Vec<ArenaIdSymbol>,
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
    pub fn new(arena: &mut IrArena, token_id: u16, name: String) -> ArenaIdSymbol {
        let symbol = Symbol {
            token_id: Token::new(token_id),
            name,
            ..Default::default()
        };
        arena.add_symbol(symbol)
    }
}
