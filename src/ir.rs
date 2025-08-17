use std::cell::RefCell;
use std::hash::{DefaultHasher, Hasher};
use std::rc::Rc;
use std::{panic, vec};
use strum_macros::{Display, EnumIter, EnumString, FromRepr};
use tree_sitter::Point;

use crate::TreeTrait;
use crate::arena::{IrArena, NodeId, ScopeId, SymbolId};
use crate::lang::AstContext;

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum IrKind {
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

impl Default for IrKind {
    fn default() -> Self {
        IrKind::Undefined
    }
}

#[derive(Debug, Clone)]
pub enum IrKindNode {
    Undefined,
    Root(Rc<RefCell<IrNodeRoot>>),
    Text(Rc<RefCell<IrNodeText>>),
    Internal(Rc<RefCell<IrNodeInternal>>),
    Scope(Rc<RefCell<IrNodeScope>>),
    File(Rc<RefCell<IrNodeFile>>),
    Identifier(Rc<RefCell<IrNodeId>>),
}

impl Default for IrKindNode {
    fn default() -> Self {
        IrKindNode::Undefined
    }
}

impl IrKindNode {
    pub fn get_id(&self) -> NodeId {
        self.get_base().arena_id
    }

    pub fn children(&self, arena: &mut IrArena) -> Vec<IrKindNode> {
        let base = self.get_base();
        base.children
            .iter()
            .map(|child_id| arena.get_node(*child_id).cloned().unwrap())
            .collect()
    }

    pub fn get_base(&self) -> IrNodeBase {
        match self {
            IrKindNode::Undefined => {
                panic!("should not happen")
            }
            IrKindNode::Root(node) => node.borrow().base.clone(),
            IrKindNode::Text(node) => node.borrow().base.clone(),
            IrKindNode::Internal(node) => node.borrow().base.clone(),
            IrKindNode::Scope(node) => node.borrow().base.clone(),
            IrKindNode::File(node) => node.borrow().base.clone(),
            IrKindNode::Identifier(node) => node.borrow().base.clone(),
        }
    }

    pub fn set_new_base(&mut self, base: IrNodeBase) {
        match self {
            IrKindNode::Undefined => {
                panic!("should not happen")
            }
            IrKindNode::Root(node) => node.borrow_mut().base = base,
            IrKindNode::Text(node) => node.borrow_mut().base = base,
            IrKindNode::Internal(node) => node.borrow_mut().base = base,
            IrKindNode::Scope(node) => node.borrow_mut().base = base,
            IrKindNode::File(node) => node.borrow_mut().base = base,
            IrKindNode::Identifier(node) => node.borrow_mut().base = base,
        }
    }

    pub fn format_node(&self, arena: &mut IrArena) -> String {
        match self {
            IrKindNode::Undefined => "undefined".into(),
            IrKindNode::Root(node) => {
                format!("root:{}", node.borrow().base.arena_id)
            }
            IrKindNode::Text(node) => {
                format!(
                    "text:{} \"{}\"",
                    node.borrow().base.arena_id,
                    node.borrow().text
                )
            }
            IrKindNode::Internal(node) => {
                format!(
                    "internal:{} >{}",
                    node.borrow().base.arena_id,
                    node.borrow().base.parent.unwrap()
                )
            }
            IrKindNode::Scope(node) => {
                let symbol_count = arena
                    .get_scope(node.borrow().scope)
                    .map(|s| s.symbols.len())
                    .unwrap_or(0);
                format!("scope:{}, #{}", node.borrow().base.arena_id, symbol_count)
            }
            IrKindNode::File(node) => {
                format!("file:{}", node.borrow().base.arena_id)
            }
            IrKindNode::Identifier(node) => {
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

    pub fn child_by_field_id(&self, arena: &mut IrArena, field_id: u16) -> Option<NodeId> {
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

    pub fn find_identifier(&self, arena: &mut IrArena, field_id: u16) -> Option<IrNodeIdPtr> {
        let id = self.child_by_field_id(arena, field_id);
        if let Some(id) = id {
            Some(arena.clone_node(id).unwrap().expect_identifier())
        } else {
            None
        }
    }

    pub fn unwrap_identifier(&self, arena: &mut IrArena, field_id: u16) -> IrNodeIdPtr {
        self.find_identifier(arena, field_id).unwrap()
    }

    pub fn upgrade_identifier_to_def(&mut self) {
        let mut base = self.get_base();
        base.kind = IrKind::IdentifierDef;
        self.set_new_base(base);
    }
}

macro_rules! impl_getters {
    ($($variant:ident => $type:ty),* $(,)?) => {
        impl IrKindNode {
            $(
                paste::paste! {
                    pub fn [<as_ $variant:lower>](&self) -> Option<&$type> {
                        match self {
                            IrKindNode::$variant(rc) => Some(rc),
                            _ => None,
                        }
                    }

                    pub fn [<into_ $variant:lower>](self) -> Option<$type> {
                        match self {
                            IrKindNode::$variant(rc) => Some(rc),
                            _ => None,
                        }
                    }

                    pub fn [<expect_ $variant:lower>](&self) -> $type {
                        match self {
                            IrKindNode::$variant(rc) => rc.clone(),
                            _ => panic!("Expected {} variant", stringify!($variant)),
                        }
                    }

                    pub fn [<is_ $variant:lower>](&self) -> bool {
                        matches!(self, IrKindNode::$variant(_))
                    }
                }
            )*
        }
    };
}

impl_getters! {
    Root => Rc<RefCell<IrNodeRoot>>,
    Text => Rc<RefCell<IrNodeText>>,
    Internal => Rc<RefCell<IrNodeInternal>>,
    Scope => Rc<RefCell<IrNodeScope>>,
    File => Rc<RefCell<IrNodeFile>>,
    Identifier => Rc<RefCell<IrNodeId>>,
}

#[derive(Debug, Clone, Default)]
pub struct IrNodeBase {
    pub arena_id: NodeId,
    pub token_id: u16,
    pub field_id: u16,
    pub kind: IrKind,
    pub start_pos: IrPoint,
    pub end_pos: IrPoint,
    pub start_byte: usize,
    pub end_byte: usize,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
}

#[derive(Debug, Clone)]
pub struct IrNodeInternal {
    pub base: IrNodeBase,
    pub name: Option<IrNodeId>,
}

impl IrNodeInternal {
    pub fn new_with_name(arena: &mut IrArena, base: IrNodeBase, name: Option<IrNodeId>) -> NodeId {
        let internal = Self { base, name };
        arena.add_node(IrKindNode::Internal(Rc::new(RefCell::new(internal))))
    }

    pub fn new(arena: &mut IrArena, base: IrNodeBase) -> NodeId {
        Self::new_with_name(arena, base, None)
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeId {
    pub base: IrNodeBase,
    pub symbol: SymbolId,
}

impl IrNodeId {
    pub fn new(arena: &mut IrArena, base: IrNodeBase, symbol: SymbolId) -> NodeId {
        let id = Self { base, symbol };
        arena.add_node(IrKindNode::Identifier(Rc::new(RefCell::new(id))))
    }

    pub fn get_symbol_name(&self, arena: &IrArena) -> String {
        arena.get_symbol(self.symbol).unwrap().name.clone()
    }
}

pub type IrNodeIdPtr = Rc<RefCell<IrNodeId>>;

#[derive(Debug, Clone, Default)]
pub struct IrPoint {
    pub row: usize,
    pub col: usize,
}

impl From<Point> for IrPoint {
    fn from(point: Point) -> Self {
        Self {
            row: point.row,
            col: point.column,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeText {
    pub base: IrNodeBase,
    pub text: String,
}

impl IrNodeText {
    pub fn new(arena: &mut IrArena, base: IrNodeBase, text: String) -> NodeId {
        let text = Self { base, text };
        arena.add_node(IrKindNode::Text(Rc::new(RefCell::new(text))))
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeError {
    pub error_place: IrPoint,
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
pub struct IrNodeFile {
    pub base: IrNodeBase,
    // pub file: File,
}

impl IrNodeFile {
    pub fn new(arena: &mut IrArena, base: IrNodeBase) -> NodeId {
        let file = Self { base };
        arena.add_node(IrKindNode::File(Rc::new(RefCell::new(file))))
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeScope {
    pub base: IrNodeBase,
    pub scope: ScopeId,
    pub symbol: Option<SymbolId>,
}

impl IrNodeScope {
    pub fn new(
        arena: &mut IrArena,
        base: IrNodeBase,
        scope: ScopeId,
        symbol: Option<SymbolId>,
    ) -> NodeId {
        let scope = Self {
            base,
            scope,
            symbol,
        };
        arena.add_node(IrKindNode::Scope(Rc::new(RefCell::new(scope))))
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeRoot {
    pub base: IrNodeBase,
    pub children: Vec<NodeId>,
}

impl IrNodeRoot {
    pub fn new(arena: &mut IrArena) -> NodeId {
        let base = IrNodeBase::default();
        let root = Self {
            base,
            children: vec![],
        };
        arena.add_node(IrKindNode::Root(Rc::new(RefCell::new(root))))
    }
}

#[derive(Debug, Clone)]
pub struct IrTree {}

impl<'a> TreeTrait<'a> for IrTree {
    type NodeType = IrKindNode;
    type ScopeType = ScopeId;
    type ParentType = NodeId;
}
