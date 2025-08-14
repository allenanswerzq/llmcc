use crate::ir::IrKindNode;
use crate::symbol::{Scope, Symbol};

use std::sync::{Arc, LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaIdNode(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaIdSymbol(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaIdScope(pub usize);

#[derive(Debug, Default)]
pub struct Arena<N, S, Sc> {
    pub nodes: Vec<N>,
    pub symbols: Vec<S>,
    pub scopes: Vec<Sc>,
}

pub type IrArena<N, S, Sc> = Arc<RwLock<Arena<N, S, Sc>>>;

impl<N, S, Sc> Arena<N, S, Sc> {
    pub fn new() -> IrArena<N, S, Sc> {
        Arc::new(RwLock::new(Self {
            nodes: vec![],
            symbols: vec![],
            scopes: vec![],
        }))
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

pub static IR_ARENA: LazyLock<IrArena<IrKindNode, Symbol, Scope>> = LazyLock::new(|| Arena::new());

pub fn ir_arena() -> RwLockReadGuard<'static, Arena<IrKindNode, Symbol, Scope>> {
    IR_ARENA.read().unwrap()
}

pub fn ir_arena_mut() -> RwLockWriteGuard<'static, Arena<IrKindNode, Symbol, Scope>> {
    IR_ARENA.write().unwrap()
}
