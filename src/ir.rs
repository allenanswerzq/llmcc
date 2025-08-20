use strum_macros::{Display, EnumIter, EnumString, FromRepr};
use tree_sitter::Point;

use crate::declare_arena;

// Declare the arena with all HIR types
declare_arena!([
    hir_root: HirRoot<'tcx>,
    hir_text: HirText<'tcx>,
    hir_internal: HirInternal<'tcx>,
    hir_scope: HirScope<'tcx>,
    hir_file: HirFile<'tcx>,
    hir_ident: HirIdent<'tcx>,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum HirKind {
    Undefined,
    Error,
    File,
    Scope,
    Text,
    Internal,
    Comment,
    IdentUse,
    IdentTyUse,
    IdentFieldUse,
    IdentDef,
    IdentTypeDef,
    IdentFieldDef,
}

impl Default for HirKind {
    fn default() -> Self {
        HirKind::Undefined
    }
}

#[derive(Debug, Clone)]
pub enum HirNode<'hir> {
    Undefined,
    Root(&'hir HirRoot<'hir>),
    Text(&'hir HirText<'hir>),
    Internal(&'hir HirInternal<'hir>),
    Scope(&'hir HirScope<'hir>),
    File(&'hir HirFile<'hir>),
    Ident(&'hir HirIdent<'hir>),
}

impl<'hir> Default for HirNode<'hir> {
    fn default() -> Self {
        HirNode::Undefined
    }
}

impl<'hir> HirNode<'hir> {
    /// Get the base information for any HIR node
    pub fn base(&self) -> Option<&HirBase<'hir>> {
        match self {
            HirNode::Undefined => None,
            HirNode::Root(node) => Some(&node.base),
            HirNode::Text(node) => Some(&node.base),
            HirNode::Internal(node) => Some(&node.base),
            HirNode::Scope(node) => Some(&node.base),
            HirNode::File(node) => Some(&node.base),
            HirNode::Ident(node) => Some(&node.base),
        }
    }

    /// Get the kind of this HIR node
    pub fn kind(&self) -> HirKind {
        self.base().map_or(HirKind::Undefined, |base| base.kind)
    }

    /// Check if this node is of a specific kind
    pub fn is_kind(&self, kind: HirKind) -> bool {
        self.kind() == kind
    }

    /// Get children of this node
    pub fn children(&self) -> &[HirNode<'hir>] {
        self.base().map_or(&[], |base| &base.children)
    }
}

#[derive(Debug, Clone, Default)]
pub struct HirBase<'hir> {
    pub token_id: u16,
    pub field_id: u16,
    pub kind: HirKind,
    pub start_pos: Point,
    pub end_pos: Point,
    pub start_byte: usize,
    pub end_byte: usize,
    pub children: Vec<HirNode<'hir>>,
}

impl<'hir> HirBase<'hir> {
    pub fn new(
        token_id: u16,
        field_id: u16,
        kind: HirKind,
        start_pos: Point,
        end_pos: Point,
        start_byte: usize,
        end_byte: usize,
    ) -> Self {
        Self {
            token_id,
            field_id,
            kind,
            start_pos,
            end_pos,
            start_byte,
            end_byte,
            children: Vec::new(),
        }
    }

    /// Add a child to this node
    pub fn add_child(&mut self, child: HirNode<'hir>) {
        self.children.push(child);
    }

    /// Get the text span covered by this node
    pub fn span(&self) -> (usize, usize) {
        (self.start_byte, self.end_byte)
    }

    /// Get the line span covered by this node
    pub fn line_span(&self) -> (usize, usize) {
        (self.start_pos.row, self.end_pos.row)
    }
}

#[derive(Debug, Clone)]
pub struct HirRoot<'hir> {
    pub base: HirBase<'hir>,
}

impl<'hir> HirRoot<'hir> {
    pub fn new(arena: &'hir Arena<'hir>, base: HirBase<'hir>) -> HirNode<'hir> {
        let root = Self { base };
        HirNode::Root(arena.alloc(root))
    }
}

#[derive(Debug, Clone)]
pub struct HirText<'hir> {
    pub base: HirBase<'hir>,
    pub text: String,
}

impl<'hir> HirText<'hir> {
    pub fn new(arena: &'hir Arena<'hir>, base: HirBase<'hir>, text: String) -> HirNode<'hir> {
        let text = Self { base, text };
        HirNode::Text(arena.alloc(text))
    }
}

#[derive(Debug, Clone)]
pub struct HirInternal<'hir> {
    pub base: HirBase<'hir>,
}

impl<'hir> HirInternal<'hir> {
    pub fn new(arena: &'hir Arena<'hir>, base: HirBase<'hir>) -> HirNode<'hir> {
        let internal = Self { base };
        HirNode::Internal(arena.alloc(internal))
    }
}

#[derive(Debug, Clone)]
pub struct HirScope<'hir> {
    pub base: HirBase<'hir>,
    pub scope_type: String,
}

impl<'hir> HirScope<'hir> {
    pub fn new(arena: &'hir Arena<'hir>, base: HirBase<'hir>, scope_type: String) -> HirNode<'hir> {
        let scope = Self { base, scope_type };
        HirNode::Scope(arena.alloc(scope))
    }
}

#[derive(Debug, Clone)]
pub struct HirFile<'hir> {
    pub base: HirBase<'hir>,
    pub file_path: String,
}

impl<'hir> HirFile<'hir> {
    pub fn new(arena: &'hir Arena<'hir>, base: HirBase<'hir>, file_path: String) -> HirNode<'hir> {
        let file = Self { base, file_path };
        HirNode::File(arena.alloc(file))
    }
}

#[derive(Debug, Clone)]
pub struct HirIdent<'hir> {
    pub base: HirBase<'hir>,
    pub name: String,
}

impl<'hir> HirIdent<'hir> {
    pub fn new(arena: &'hir Arena<'hir>, base: HirBase<'hir>, name: String) -> HirNode<'hir> {
        let ident = Self { base, name };
        HirNode::Ident(arena.alloc(ident))
    }
}
