use crate::file::File;
use crate::ir::{Arena, HirId, HirKind, HirNode};
use crate::symbol::{Scope, ScopeStack, SymId, Symbol};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Deref;

#[derive(Debug, Copy, Clone)]
pub struct Context<'tcx> {
    pub gcx: &'tcx GlobalCtxt<'tcx>,
}

impl<'tcx> Context<'tcx> {
    pub fn hir_node(self, id: HirId) -> HirNode<'tcx> {
        self.gcx.hir_map.borrow().get(&id).unwrap().node.clone()
    }
}

impl<'tcx> Deref for Context<'tcx> {
    type Target = GlobalCtxt<'tcx>;

    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        self.gcx
    }
}

#[derive(Debug, Clone)]
pub struct ParentedNode<'tcx> {
    pub parent: HirId,
    pub node: HirNode<'tcx>,
}

impl<'tcx> ParentedNode<'tcx> {
    pub fn new(parent: HirId, node: HirNode<'tcx>) -> Self {
        Self { parent, node }
    }
}

#[derive(Debug)]
pub struct GlobalCtxt<'tcx> {
    pub arena: Arena<'tcx>,
    pub file: File,
    pub hir_map: RefCell<HashMap<HirId, ParentedNode<'tcx>>>,
}

impl<'tcx> GlobalCtxt<'tcx> {
    pub fn from_source(source: &[u8]) -> Self {
        Self {
            arena: Arena::default(),
            file: File::new_source(source.to_vec()),
            hir_map: RefCell::new(HashMap::new()),
        }
    }

    pub fn create_context(&'tcx self) -> Context<'tcx> {
        Context { gcx: self }
    }
}
