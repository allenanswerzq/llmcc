use std::sync::{Arc, LazyLock, RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaIdNode(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaIdSymbol(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArenaIdScope(pub usize);

#[derive(Debug, Default)]
pub struct AstArena<N, S, Sc> {
    pub nodes: Vec<N>,
    pub symbols: Vec<S>,
    pub scopes: Vec<Sc>,
}

pub type AstArenaShare<N, S, Sc> = Arc<RwLock<AstArena<N, S, Sc>>>;

impl<N, S, Sc> AstArena<N, S, Sc> {
    pub fn new() -> AstArenaShare<N, S, Sc> {
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

use crate::{AstKindNode, AstScope, AstSymbol};

pub static AST_ARENA: LazyLock<AstArenaShare<AstKindNode, AstSymbol, AstScope>> =
    LazyLock::new(|| AstArena::new());

pub fn ast_arena() -> RwLockReadGuard<'static, AstArena<AstKindNode, AstSymbol, AstScope>> {
    AST_ARENA.read().unwrap()
}

pub fn ast_arena_mut() -> RwLockWriteGuard<'static, AstArena<AstKindNode, AstSymbol, AstScope>> {
    AST_ARENA.write().unwrap()
}
