use std::collections::HashMap;

use crate::file::File;
use crate::ir::{Arena, HirId, HirKind, HirNode};
use crate::symbol::{Scope, ScopeStack, SymId, Symbol};

#[derive(Debug)]
pub struct ParentedNode<'tcx> {
    parent: HirId,
    node: HirNode<'tcx>,
}

#[derive(Debug)]
pub struct TyCtxt<'tcx> {
    pub arena: Arena<'tcx>,
    pub hir_map: HashMap<HirId, ParentedNode<'tcx>>,
    pub file: File,
}

impl<'tcx> TyCtxt<'tcx> {
    pub fn from_source(source: &[u8]) -> TyCtxt {
        TyCtxt {
            arena: Arena::default(),
            hir_map: HashMap::new(),
            file: File::new_source(source.to_vec()),
        }
    }

    pub fn hir_node(&self, id: HirId) -> HirNode<'tcx> {
        self.hir_map.get(&id).unwrap().node.clone()
    }
}
