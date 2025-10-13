use strum_macros::{Display, EnumIter, EnumString, FromRepr};
use tree_sitter::Node;

use crate::context::CompileUnit;
use crate::declare_arena;
use crate::symbol::{Scope, Symbol};

// Declare the arena with all HIR types
declare_arena!([
    hir_root: HirRoot<'tcx>,
    hir_text: HirText<'tcx>,
    hir_internal: HirInternal<'tcx>,
    hir_scope: HirScope<'tcx>,
    hir_file: HirFile<'tcx>,
    hir_ident: HirIdent<'tcx>,
    symbol: Symbol,
    scope: Scope<'tcx>,
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
    Identifier,
}

impl Default for HirKind {
    fn default() -> Self {
        HirKind::Undefined
    }
}

#[derive(Debug, Clone, Copy)]
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
    pub fn format_node(&self, unit: CompileUnit<'hir>) -> String {
        let id = self.hir_id();
        let kind = self.kind();
        let mut f = format!("{}:{}", kind, id);

        if let Some(def) = unit.opt_defs(id) {
            f.push_str(&format!("   d:{}", def.format_compact()));
        } else if let Some(sym) = unit.opt_uses(id) {
            f.push_str(&format!("   u:{}", sym.format_compact()));
        }

        if let Some(scope) = unit.opt_scope(id) {
            f.push_str(&format!("   s:{}", scope.format_compact()));
        }

        f
    }

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

    pub fn field_id(&self) -> u16 {
        self.base().unwrap().field_id
    }

    /// Get children of this node
    pub fn children(&self) -> &[HirId] {
        self.base().map_or(&[], |base| &base.children)
    }

    pub fn kind_id(&self) -> u16 {
        self.base().unwrap().node.kind_id()
    }

    pub fn hir_id(&self) -> HirId {
        self.base().unwrap().hir_id
    }

    pub fn start_byte(&self) -> usize {
        self.base().unwrap().node.start_byte()
    }

    pub fn end_byte(&self) -> usize {
        self.base().unwrap().node.end_byte()
    }

    pub fn child_count(&self) -> usize {
        self.children().len()
    }

    pub fn inner_ts_node(&self) -> Node<'hir> {
        self.base().unwrap().node
    }

    pub fn parent(&self) -> Option<HirId> {
        self.base().and_then(|base| base.parent)
    }

    pub fn opt_child_by_field(&self, unit: CompileUnit<'hir>, field_id: u16) -> Option<HirNode<'hir>> {
        self.base().unwrap().opt_child_by_field(unit, field_id)
    }

    pub fn child_by_field(&self, unit: CompileUnit<'hir>, field_id: u16) -> HirNode<'hir> {
        self.opt_child_by_field(unit, field_id)
            .unwrap_or_else(|| panic!("no child with field_id {}", field_id))
    }

    pub fn expect_ident_child_by_field(
        &self,
        unit: CompileUnit<'hir>,
        field_id: u16,
    ) -> &'hir HirIdent<'hir> {
        self.opt_child_by_field(unit, field_id)
            .map(|child| child.expect_ident())
            .unwrap_or_else(|| panic!("no child with field_id {}", field_id))
    }

    pub fn opt_child_by_kind(&self, unit: CompileUnit<'hir>, kind_id: u16) -> Option<HirNode<'hir>> {
        self.children()
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| child.kind_id() == kind_id)
    }

    pub fn child_by_kind(&self, unit: CompileUnit<'hir>, kind_id: u16) -> HirNode<'hir> {
        self.opt_child_by_kind(unit, kind_id)
            .unwrap_or_else(|| panic!("no child with kind_id {}", kind_id))
    }
}

macro_rules! impl_getters {
    ($($variant:ident => $type:ty),* $(,)?) => {
        impl<'hir> HirNode<'hir> {
            $(
                paste::paste! {
                    pub fn [<as_ $variant:lower>](&self) -> Option<$type> {
                        match self {
                            HirNode::$variant(r) => Some(r),
                            _ => None,
                        }
                    }

                    pub fn [<expect_ $variant:lower>](&self) -> $type {
                        match self {
                            HirNode::$variant(r) => r,
                            _ => panic!("Expected {} variant", stringify!($variant)),
                        }
                    }

                    pub fn [<is_ $variant:lower>](&self) -> bool {
                        matches!(self, HirNode::$variant(_))
                    }
                }
            )*
        }
    };
}

impl_getters! {
    Root => &'hir HirRoot<'hir>,
    Text => &'hir HirText<'hir>,
    Internal => &'hir HirInternal<'hir>,
    Scope => &'hir HirScope<'hir>,
    File => &'hir HirFile<'hir>,
    Ident => &'hir HirIdent<'hir>,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug, Hash, Default)]
pub struct HirId(pub u32);

impl std::fmt::Display for HirId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone)]
pub struct HirBase<'hir> {
    pub hir_id: HirId,
    pub parent: Option<HirId>,
    pub node: Node<'hir>,
    pub kind: HirKind,
    pub field_id: u16,
    pub children: Vec<HirId>,
}

impl<'hir> HirBase<'hir> {
    pub fn opt_child_by_fields(
        &self,
        unit: CompileUnit<'hir>,
        fields_id: &[u16],
    ) -> Option<HirNode<'hir>> {
        self.children
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| fields_id.contains(&child.field_id()))
    }

    pub fn opt_child_by_field(&self, unit: CompileUnit<'hir>, field_id: u16) -> Option<HirNode<'hir>> {
        self.children
            .iter()
            .map(|id| unit.hir_node(*id))
            .find(|child| child.field_id() == field_id)
    }
}

#[derive(Debug, Clone)]
pub struct HirRoot<'hir> {
    pub base: HirBase<'hir>,
}

impl<'hir> HirRoot<'hir> {
    pub fn new(base: HirBase<'hir>) -> Self {
        Self { base }
    }
}

#[derive(Debug, Clone)]
pub struct HirText<'hir> {
    pub base: HirBase<'hir>,
    pub text: String,
}

impl<'hir> HirText<'hir> {
    pub fn new(base: HirBase<'hir>, text: String) -> Self {
        Self { base, text }
    }
}

#[derive(Debug, Clone)]
pub struct HirInternal<'hir> {
    pub base: HirBase<'hir>,
}

impl<'hir> HirInternal<'hir> {
    pub fn new(base: HirBase<'hir>) -> Self {
        Self { base }
    }
}

#[derive(Debug, Clone)]
pub struct HirScope<'hir> {
    pub base: HirBase<'hir>,
    pub ident: Option<&'hir HirIdent<'hir>>,
}

impl<'hir> HirScope<'hir> {
    pub fn new(base: HirBase<'hir>, ident: Option<&'hir HirIdent<'hir>>) -> Self {
        Self { base, ident }
    }

    pub fn owner_name(&self) -> String {
        if let Some(id) = self.ident {
            return id.name.clone();
        } else {
            "unamed_scope".to_string()
        }
    }
}

#[derive(Debug, Clone)]
pub struct HirIdent<'hir> {
    pub base: HirBase<'hir>,
    pub name: String,
}

impl<'hir> HirIdent<'hir> {
    pub fn new(base: HirBase<'hir>, name: String) -> Self {
        Self { base, name }
    }
}

#[derive(Debug, Clone)]
pub struct HirFile<'hir> {
    pub base: HirBase<'hir>,
    pub file_path: String,
}

impl<'hir> HirFile<'hir> {
    pub fn new(base: HirBase<'hir>, file_path: String) -> Self {
        Self { base, file_path }
    }
}
