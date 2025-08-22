use std::collections::HashMap;

use crate::file::File;
use crate::ir::{Arena, HirId, HirKind, HirNode};
use crate::symbol::{Scope, ScopeStack, SymId, Symbol};

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
pub struct LangContext<'tcx> {
    pub arena: &'tcx Arena<'tcx>,
    pub hir_map: &'tcx HashMap<HirId, ParentedNode<'tcx>>,
    pub file: &'tcx File,
}

impl<'tcx> LangContext<'tcx> {
    pub fn hir_node(&self, id: HirId) -> HirNode<'tcx> {
        self.hir_map.get(&id).unwrap().node.clone()
    }
}

#[derive(Debug)]
pub struct Context<'tcx> {
    pub arena: Arena<'tcx>,
    pub hir_map: HashMap<HirId, ParentedNode<'tcx>>,
    pub file: File,
}

impl<'tcx> Context<'tcx> {
    pub fn from_source(source: &[u8]) -> Context {
        Context {
            arena: Arena::default(),
            hir_map: HashMap::new(),
            file: File::new_source(source.to_vec()),
        }
    }

    pub fn lang_ctx(&'tcx self) -> LangContext<'tcx> {
        LangContext {
            arena: &self.arena,
            hir_map: &self.hir_map,
            file: &self.file,
        }
    }
}
