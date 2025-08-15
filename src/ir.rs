use std::hash::{DefaultHasher, Hasher};
use std::{panic, vec};

use crate::arena::{ArenaIdNode, ArenaIdScope, ArenaIdSymbol, IrArena};
use crate::visit::NodeTrait;
use tree_sitter::Point;

use strum_macros::{Display, EnumIter, EnumString, FromRepr};

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
    Root(Box<IrNodeRoot>),
    Text(Box<IrNodeText>),
    Internal(Box<IrNodeInternal>),
    Scope(Box<IrNodeScope>),
    File(Box<IrNodeFile>),
    Identifier(Box<IrNodeId>),
}

impl Default for IrKindNode {
    fn default() -> Self {
        IrKindNode::Undefined
    }
}

impl IrKindNode {
    pub fn child_by_field_id(&self, arena: &mut IrArena, field_id: u16) -> Option<ArenaIdNode> {
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

    pub fn get_id(&self) -> ArenaIdNode {
        self.get_base().arena_id
    }

    pub fn get_scope(&self) -> Option<ArenaIdScope> {
        match self {
            IrKindNode::Scope(node) => Some(node.scope),
            _ => None,
        }
    }

    pub fn get_symbol(&self) -> Option<ArenaIdSymbol> {
        match self {
            IrKindNode::Identifier(node) => Some(node.symbol),
            _ => None,
        }
    }

    pub fn get_base(&self) -> &IrNodeBase {
        match self {
            IrKindNode::Undefined => {
                panic!("should not happen")
            }
            IrKindNode::Root(node) => &node.base,
            IrKindNode::Text(node) => &node.base,
            IrKindNode::Internal(node) => &node.base,
            IrKindNode::Scope(node) => &node.base,
            IrKindNode::File(node) => &node.base,
            IrKindNode::Identifier(node) => &node.base,
        }
    }

    pub fn get_base_mut(&mut self) -> &mut IrNodeBase {
        match self {
            IrKindNode::Undefined => {
                panic!("should not happen")
            }
            IrKindNode::Root(node) => &mut node.base,
            IrKindNode::Text(node) => &mut node.base,
            IrKindNode::Internal(node) => &mut node.base,
            IrKindNode::Scope(node) => &mut node.base,
            IrKindNode::File(node) => &mut node.base,
            IrKindNode::Identifier(node) => &mut node.base,
        }
    }

    pub fn format_node(&self, arena: &mut IrArena) -> String {
        match self {
            IrKindNode::Undefined => "undefined".into(),
            IrKindNode::Root(node) => {
                format!("root [{}]", node.base.arena_id)
            }
            IrKindNode::Text(node) => {
                format!("text [{}] \"{}\"", node.base.arena_id, node.text)
            }
            IrKindNode::Internal(node) => {
                format!(
                    "internal [{}] (parent: {:?})",
                    node.base.arena_id, node.base.parent
                )
            }
            IrKindNode::Scope(node) => {
                let symbol_count = arena
                    .get_scope(node.scope)
                    .map(|s| s.symbols.len())
                    .unwrap_or(0);
                format!("scope [{}], {} symbols", node.base.arena_id, symbol_count)
            }
            IrKindNode::File(node) => {
                format!("file [{}]", node.base.arena_id)
            }
            IrKindNode::Identifier(node) => {
                let symbol = arena.get_symbol(node.symbol);
                if let Some(sym) = symbol {
                    format!(
                        "{} [{}] '{}', '{}', @{:?}, ${:?}, %{:?}",
                        node.base.kind.to_string(),
                        node.base.arena_id,
                        sym.name,
                        sym.mangled_name,
                        sym.defined,
                        sym.type_of,
                        sym.field_of
                    )
                } else {
                    format!(
                        "{} [{}] <invalid symbol>",
                        node.base.kind.to_string(),
                        node.base.arena_id
                    )
                }
            }
        }
    }

    pub fn set_parent(&mut self, parent: ArenaIdNode) {
        self.get_base_mut().parent = Some(parent);
    }

    pub fn get_child(&self, index: usize) -> Option<ArenaIdNode> {
        self.get_base().children.get(index).copied()
    }

    pub fn add_child(&mut self, child: ArenaIdNode) {
        self.get_base_mut().children.push(child)
    }
}

#[derive(Debug, Clone, Default)]
pub struct IrNodeBase {
    pub arena_id: ArenaIdNode,
    pub debug_id: i64,
    pub token_id: u16,
    pub field_id: u16,
    pub kind: IrKind,
    pub start_pos: IrPoint,
    pub end_pos: IrPoint,
    pub start_byte: usize,
    pub end_byte: usize,
    pub parent: Option<ArenaIdNode>,
    pub children: Vec<ArenaIdNode>,
}

#[derive(Debug, Clone)]
pub struct IrNodeInternal {
    pub base: IrNodeBase,
    pub name: Option<IrNodeId>,
}

impl IrNodeInternal {
    pub fn new_with_name(
        arena: &mut IrArena,
        base: IrNodeBase,
        name: Option<IrNodeId>,
    ) -> ArenaIdNode {
        let internal = Self { base, name };
        arena.add_node(IrKindNode::Internal(Box::new(internal)))
    }

    pub fn new(arena: &mut IrArena, base: IrNodeBase) -> ArenaIdNode {
        Self::new_with_name(arena, base, None)
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeId {
    pub base: IrNodeBase,
    pub symbol: ArenaIdSymbol,
}

impl IrNodeId {
    pub fn new(arena: &mut IrArena, base: IrNodeBase, symbol: ArenaIdSymbol) -> ArenaIdNode {
        let id = Self { base, symbol };
        arena.add_node(IrKindNode::Identifier(Box::new(id)))
    }
}

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
    pub fn new(arena: &mut IrArena, base: IrNodeBase, text: String) -> ArenaIdNode {
        let text = Self { base, text };
        arena.add_node(IrKindNode::Text(Box::new(text)))
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
    pub fn new(arena: &mut IrArena, base: IrNodeBase) -> ArenaIdNode {
        let file = Self { base };
        arena.add_node(IrKindNode::File(Box::new(file)))
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeScope {
    pub base: IrNodeBase,
    pub scope: ArenaIdScope,
    pub name: Option<ArenaIdNode>,
}

impl IrNodeScope {
    pub fn new(
        arena: &mut IrArena,
        base: IrNodeBase,
        scope: ArenaIdScope,
        name: Option<ArenaIdNode>,
    ) -> ArenaIdNode {
        let scope = Self { base, scope, name };
        arena.add_node(IrKindNode::Scope(Box::new(scope)))
    }
}

#[derive(Debug, Clone)]
pub struct IrNodeRoot {
    pub base: IrNodeBase,
    pub children: Vec<ArenaIdNode>,
}

impl IrNodeRoot {
    pub fn new(arena: &mut IrArena) -> ArenaIdNode {
        let mut base = IrNodeBase::default();
        base.debug_id = -1;
        let root = Self {
            base,
            children: vec![],
        };
        arena.add_node(IrKindNode::Root(Box::new(root)))
    }
}

impl NodeTrait for IrKindNode {
    fn get_child(&self, index: usize) -> Option<ArenaIdNode> {
        self.get_child(index)
    }

    fn child_count(&self) -> usize {
        self.get_base().children.len()
    }
}
