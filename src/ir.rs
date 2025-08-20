use std::cell::RefCell;
use std::hash::{DefaultHasher, Hasher};
use std::rc::Rc;
use std::{panic, vec};
use strum_macros::{Display, EnumIter, EnumString, FromRepr};
use tree_sitter::Point;

use crate::TreeTrait;
use crate::arena::{HirArena, NodeId, ScopeId, SymbolId};
use crate::lang::AstContext;

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum HirKind {
    Undefined,
    Error,
    File,
    Scope,
    Text,
    Internal,
    Comment,
    IdentifierUse,
    IdentifierTypeUse,
    IdentifierFieldUse,
    IdentifierDef,
    IdentifierTypeDef,
    IdentifierFieldDef,
}

impl Default for HirKind {
    fn default() -> Self {
        HirKind::Undefined
    }
}

#[derive(Debug, Clone)]
pub enum HirKindNode {
    Undefined,
    Root(Rc<RefCell<HirNodeRoot>>),
    Text(Rc<RefCell<HirNodeText>>),
    Internal(Rc<RefCell<HirNodeInternal>>),
    Scope(Rc<RefCell<HirNodeScope>>),
    File(Rc<RefCell<HirNodeFile>>),
    Identifier(Rc<RefCell<HirNodeId>>),
}

impl Default for HirKindNode {
    fn default() -> Self {
        HirKindNode::Undefined
    }
}

impl HirKindNode {
    pub fn get_id(&self) -> NodeId {
        self.get_base().arena_id
    }

    pub fn children(&self, arena: &mut HirArena) -> Vec<HirKindNode> {
        let base = self.get_base();
        base.children
            .iter()
            .map(|child_id| arena.get_node(*child_id).cloned().unwrap())
            .collect()
    }

    pub fn get_base(&self) -> HirNodeBase {
        match self {
            HirKindNode::Undefined => {
                panic!("should not happen")
            }
            HirKindNode::Root(node) => node.borrow().base.clone(),
            HirKindNode::Text(node) => node.borrow().base.clone(),
            HirKindNode::Internal(node) => node.borrow().base.clone(),
            HirKindNode::Scope(node) => node.borrow().base.clone(),
            HirKindNode::File(node) => node.borrow().base.clone(),
            HirKindNode::Identifier(node) => node.borrow().base.clone(),
        }
    }

    pub fn set_new_base(&mut self, base: HirNodeBase) {
        match self {
            HirKindNode::Undefined => {
                panic!("should not happen")
            }
            HirKindNode::Root(node) => node.borrow_mut().base = base,
            HirKindNode::Text(node) => node.borrow_mut().base = base,
            HirKindNode::Internal(node) => node.borrow_mut().base = base,
            HirKindNode::Scope(node) => node.borrow_mut().base = base,
            HirKindNode::File(node) => node.borrow_mut().base = base,
            HirKindNode::Identifier(node) => node.borrow_mut().base = base,
        }
    }

    pub fn format_node(&self, arena: &mut HirArena) -> String {
        match self {
            HirKindNode::Undefined => "undefined".into(),
            HirKindNode::Root(node) => {
                format!("root:{}", node.borrow().base.arena_id)
            }
            HirKindNode::Text(node) => {
                format!(
                    "text:{} \"{}\"",
                    node.borrow().base.arena_id,
                    node.borrow().text
                )
            }
            HirKindNode::Internal(node) => {
                format!(
                    "internal:{} >{}",
                    node.borrow().base.arena_id,
                    node.borrow().base.parent.unwrap()
                )
            }
            HirKindNode::Scope(node) => {
                let symbol_count = arena
                    .get_scope(node.borrow().scope)
                    .map(|s| s.symbols.len())
                    .unwrap_or(0);
                format!("scope:{}, #{}", node.borrow().base.arena_id, symbol_count)
            }
            HirKindNode::File(node) => {
                format!("file:{}", node.borrow().base.arena_id)
            }
            HirKindNode::Identifier(node) => {
                let symbol = arena.get_symbol(node.borrow().symbol);
                if let Some(sym) = symbol {
                    let defined = sym.defined.map_or("".into(), |id| id.to_string());
                    let type_of = sym.type_of.map_or("".into(), |id| id.to_string());
                    let field_of = sym.field_of.map_or("".into(), |id| id.to_string());
                    format!(
                        "{}:{} ^{}, ^{}, ${}, @{}, %{}",
                        node.borrow().base.kind.to_string(),
                        node.borrow().base.arena_id,
                        sym.name,
                        sym.mangled_name,
                        defined,
                        type_of,
                        field_of
                    )
                } else {
                    format!(
                        "{}:{} <invalid symbol>",
                        node.borrow().base.kind.to_string(),
                        node.borrow().base.arena_id
                    )
                }
            }
        }
    }

    pub fn set_parent(&mut self, parent: NodeId) {
        let mut base = self.get_base();
        base.parent = Some(parent);
        self.set_new_base(base);
    }

    pub fn get_child(&self, index: usize) -> Option<NodeId> {
        self.get_base().children.get(index).copied()
    }

    pub fn add_child(&mut self, child: NodeId) {
        let mut base = self.get_base();
        base.children.push(child);
        self.set_new_base(base);
    }

    pub fn child_by_field_id(&self, arena: &mut HirArena, field_id: u16) -> Option<NodeId> {
        self.get_base()
            .children
            .iter()
            .find(|&&node_id| {
                arena
                    .get_node(node_id)
                    .map_or(false, |node| node.get_base().field_id == field_id)
            })
            .copied()
    }

    pub fn find_identifier(&self, arena: &mut HirArena, field_id: u16) -> Option<HirNodeIdPtr> {
        let id = self.child_by_field_id(arena, field_id);
        if let Some(id) = id {
            Some(arena.clone_node(id).unwrap().expect_identifier())
        } else {
            None
        }
    }

    pub fn unwrap_identifier(&self, arena: &mut HirArena, field_id: u16) -> HirNodeIdPtr {
        self.find_identifier(arena, field_id).unwrap()
    }
}

macro_rules! impl_getters {
    ($($variant:ident => $type:ty),* $(,)?) => {
        impl HirKindNode {
            $(
                paste::paste! {
                    pub fn [<as_ $variant:lower>](&self) -> Option<&$type> {
                        match self {
                            HirKindNode::$variant(rc) => Some(rc),
                            _ => None,
                        }
                    }

                    pub fn [<into_ $variant:lower>](self) -> Option<$type> {
                        match self {
                            HirKindNode::$variant(rc) => Some(rc),
                            _ => None,
                        }
                    }

                    pub fn [<expect_ $variant:lower>](&self) -> $type {
                        match self {
                            HirKindNode::$variant(rc) => rc.clone(),
                            _ => panic!("Expected {} variant", stringify!($variant)),
                        }
                    }

                    pub fn [<is_ $variant:lower>](&self) -> bool {
                        matches!(self, HirKindNode::$variant(_))
                    }
                }
            )*
        }
    };
}

impl_getters! {
    Root => Rc<RefCell<HirNodeRoot>>,
    Text => Rc<RefCell<HirNodeText>>,
    Internal => Rc<RefCell<HirNodeInternal>>,
    Scope => Rc<RefCell<HirNodeScope>>,
    File => Rc<RefCell<HirNodeFile>>,
    Identifier => Rc<RefCell<HirNodeId>>,
}

#[derive(Debug, Clone, Default)]
pub struct HirNodeBase {
    pub arena_id: NodeId,
    pub token_id: u16,
    pub field_id: u16,
    pub kind: HirKind,
    pub start_pos: HirPoint,
    pub end_pos: HirPoint,
    pub start_byte: usize,
    pub end_byte: usize,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct HirNodeInternal {
    pub base: HirNodeBase,
    pub name: Option<NodeId>,
}

impl HirNodeInternal {
    pub fn new_with_name(arena: &mut HirArena, base: HirNodeBase, name: Option<NodeId>) -> NodeId {
        let internal = Self { base, name };
        arena.add_node(HirKindNode::Internal(Rc::new(RefCell::new(internal))))
    }

    pub fn new(arena: &mut HirArena, base: HirNodeBase) -> NodeId {
        Self::new_with_name(arena, base, None)
    }
}

#[derive(Debug, Clone)]
pub struct HirNodeId {
    pub base: HirNodeBase,
    pub symbol: SymbolId,
}

impl HirNodeId {
    pub fn new(arena: &mut HirArena, base: HirNodeBase, symbol: SymbolId) -> NodeId {
        let id = Self { base, symbol };
        arena.add_node(HirKindNode::Identifier(Rc::new(RefCell::new(id))))
    }

    pub fn get_symbol_name(&self, arena: &HirArena) -> String {
        arena.get_symbol(self.symbol).unwrap().name.clone()
    }

    pub fn upgrade_identifier_to_def(&mut self, arena: &mut HirArena, owner: NodeId) {
        self.base.kind = HirKind::IdentifierDef;
        let symbol = arena.get_symbol_mut(self.symbol).unwrap();
        symbol.owner = owner;
    }
}

pub type HirNodeIdPtr = Rc<RefCell<HirNodeId>>;

#[derive(Debug, Clone, Default)]
pub struct HirPoint {
    pub row: usize,
    pub col: usize,
}

impl From<Point> for HirPoint {
    fn from(point: Point) -> Self {
        Self {
            row: point.row,
            col: point.column,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HirNodeText {
    pub base: HirNodeBase,
    pub text: String,
}

impl HirNodeText {
    pub fn new(arena: &mut HirArena, base: HirNodeBase, text: String) -> NodeId {
        let text = Self { base, text };
        arena.add_node(HirKindNode::Text(Rc::new(RefCell::new(text))))
    }
}

#[derive(Debug, Clone)]
pub struct HirNodeError {
    pub error_place: HirPoint,
}

#[derive(Debug, Clone, Default)]
pub struct FileId {
    pub path: Option<String>,
    pub content: Option<Vec<u8>>,
    pub content_hash: u64,
}

impl FileId {
    pub fn new_path(path: String) -> Self {
        FileId {
            path: Some(path),
            content: None,
            content_hash: 0,
        }
    }

    pub fn new_content(content: Vec<u8>) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(&content);
        let content_hash = hasher.finish();

        FileId {
            path: None,
            content: Some(content),
            content_hash,
        }
    }

    pub fn get_text(&self, start_byte: usize, end_byte: usize) -> Option<String> {
        let content_bytes = self.content.as_ref()?;

        if start_byte > end_byte
            || start_byte > content_bytes.len()
            || end_byte > content_bytes.len()
        {
            return None;
        }

        let slice = &content_bytes[start_byte..end_byte];
        Some(String::from_utf8_lossy(slice).into_owned())
    }

    pub fn get_full_text(&self) -> Option<String> {
        let content_bytes = self.content.as_ref()?;
        Some(String::from_utf8_lossy(content_bytes).into_owned())
    }
}

#[derive(Debug, Clone)]
pub struct File {
    // TODO: add cache and all other stuff
    pub file: FileId,
}

impl File {
    pub fn new_source(source: Vec<u8>) -> Self {
        File {
            file: FileId::new_content(source),
        }
    }

    pub fn get_text(&self, start: usize, end: usize) -> Option<String> {
        self.file.get_text(start, end)
    }
}

#[derive(Debug, Clone)]
pub struct HirNodeFile {
    pub base: HirNodeBase,
    // pub file: File,
}

impl HirNodeFile {
    pub fn new(arena: &mut HirArena, base: HirNodeBase) -> NodeId {
        let file = Self { base };
        arena.add_node(HirKindNode::File(Rc::new(RefCell::new(file))))
    }
}

#[derive(Debug, Clone)]
pub struct HirNodeScope {
    pub base: HirNodeBase,
    pub scope: ScopeId,
    pub name: Option<NodeId>,
}

impl HirNodeScope {
    pub fn new(
        arena: &mut HirArena,
        base: HirNodeBase,
        scope: ScopeId,
        name: Option<NodeId>,
    ) -> NodeId {
        let scope = Self { base, scope, name };
        arena.add_node(HirKindNode::Scope(Rc::new(RefCell::new(scope))))
    }
}

#[derive(Debug, Clone)]
pub struct HirNodeRoot {
    pub base: HirNodeBase,
    pub children: Vec<NodeId>,
}

impl HirNodeRoot {
    pub fn new(arena: &mut HirArena) -> NodeId {
        let base = HirNodeBase::default();
        let root = Self {
            base,
            children: vec![],
        };
        arena.add_node(HirKindNode::Root(Rc::new(RefCell::new(root))))
    }
}

#[derive(Debug, Clone)]
pub struct HirTree {}

impl<'a> TreeTrait<'a> for HirTree {
    type NodeType = HirKindNode;
    type ScopeType = ScopeId;
    type ParentType = NodeId;
}
