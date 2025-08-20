use crate::ir::HirKindNode;
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

make_id_type!(NodeId);
make_id_type!(SymbolId);
make_id_type!(ScopeId);

#[derive(Debug, Default)]
pub struct Arena<N, S, Sc> {
    pub nodes: Vec<N>,
    pub symbols: Vec<S>,
    pub scopes: Vec<Sc>,
}

impl<N: Clone, S, Sc> Arena<N, S, Sc> {
    pub fn new() -> Arena<N, S, Sc> {
        Self {
            nodes: vec![],
            symbols: vec![],
            scopes: vec![],
        }
    }

    pub fn add_node(&mut self, node: N) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    pub fn add_symbol(&mut self, symbol: S) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        self.symbols.push(symbol);
        id
    }

    pub fn add_scope(&mut self, scope: Sc) -> ScopeId {
        let id = ScopeId(self.scopes.len());
        self.scopes.push(scope);
        id
    }

    pub fn get_next_node_id(&self) -> NodeId {
        NodeId(self.nodes.len())
    }

    pub fn get_next_symbol_id(&self) -> SymbolId {
        SymbolId(self.symbols.len())
    }

    pub fn get_node(&self, id: NodeId) -> Option<&N> {
        self.nodes.get(id.0)
    }

    pub fn clone_node(&self, id: NodeId) -> Option<N> {
        self.nodes.get(id.0).cloned()
    }

    pub fn get_node_mut(&mut self, id: NodeId) -> Option<&mut N> {
        self.nodes.get_mut(id.0)
    }

    pub fn get_symbol(&self, id: SymbolId) -> Option<&S> {
        self.symbols.get(id.0)
    }

    pub fn get_symbol_mut(&mut self, id: SymbolId) -> Option<&mut S> {
        self.symbols.get_mut(id.0)
    }

    pub fn get_scope(&self, id: ScopeId) -> Option<&Sc> {
        self.scopes.get(id.0)
    }

    pub fn get_scope_mut(&mut self, id: ScopeId) -> Option<&mut Sc> {
        self.scopes.get_mut(id.0)
    }
}

pub type HirArena = Arena<HirKindNode, Symbol, Scope>;
