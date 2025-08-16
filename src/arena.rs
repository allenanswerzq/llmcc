use crate::ir::IrKindNode;
use crate::symbol::{Scope, Symbol};

macro_rules! make_id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $name(pub usize);

        impl From<$name> for usize {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }
    };
}

make_id_type!(ArenaIdNode);
make_id_type!(ArenaIdSymbol);
make_id_type!(ArenaIdScope);

#[derive(Debug, Default)]
pub struct Arena<N, S, Sc> {
    pub nodes: Vec<N>,
    pub symbols: Vec<S>,
    pub scopes: Vec<Sc>,
}

impl<N, S, Sc> Arena<N, S, Sc> {
    pub fn new() -> Arena<N, S, Sc> {
        Self {
            nodes: vec![],
            symbols: vec![],
            scopes: vec![],
        }
    }

    pub fn add_node(&mut self, node: N) -> ArenaIdNode {
        let id = ArenaIdNode(self.nodes.len());
        self.nodes.push(node);
        id
    }

    pub fn add_symbol(&mut self, symbol: S) -> ArenaIdSymbol {
        let id = ArenaIdSymbol(self.symbols.len());
        self.symbols.push(symbol);
        id
    }

    pub fn add_scope(&mut self, scope: Sc) -> ArenaIdScope {
        let id = ArenaIdScope(self.scopes.len());
        self.scopes.push(scope);
        id
    }

    pub fn get_next_node_id(&self) -> ArenaIdNode {
        ArenaIdNode(self.nodes.len())
    }

    pub fn get_node(&self, id: ArenaIdNode) -> Option<&N> {
        self.nodes.get(id.0)
    }

    pub fn get_node_mut(&mut self, id: ArenaIdNode) -> Option<&mut N> {
        self.nodes.get_mut(id.0)
    }

    pub fn get_symbol(&self, id: ArenaIdSymbol) -> Option<&S> {
        self.symbols.get(id.0)
    }

    pub fn get_symbol_mut(&mut self, id: ArenaIdSymbol) -> Option<&mut S> {
        self.symbols.get_mut(id.0)
    }

    pub fn get_scope(&self, id: ArenaIdScope) -> Option<&Sc> {
        self.scopes.get(id.0)
    }

    pub fn get_scope_mut(&mut self, id: ArenaIdScope) -> Option<&mut Sc> {
        self.scopes.get_mut(id.0)
    }
}

pub type IrArena = Arena<IrKindNode, Symbol, Scope>;
