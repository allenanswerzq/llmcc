use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::num::NonZeroU16;
use std::{panic, vec};

use crate::arena::{ArenaIdNode, ArenaIdScope, ArenaIdSymbol, ast_arena, ast_arena_mut};
use crate::visit::NodeTrait;
use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use strum::{Display, EnumIter, EnumString, FromRepr};

#[derive(Debug, Clone, Copy, PartialEq, EnumIter, EnumString, FromRepr, Display)]
#[strum(serialize_all = "snake_case")]
pub enum AstKind {
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

impl Default for AstKind {
    fn default() -> Self {
        AstKind::Undefined
    }
}

#[derive(Debug, Clone)]
#[strum(serialize_all = "snake_case")]
pub enum AstKindNode {
    Undefined,
    Root(Box<AstNodeRoot>),
    Text(Box<AstNodeText>),
    Internal(Box<AstNode>),
    Scope(Box<AstNodeScope>),
    File(Box<AstNodeFile>),
    Identifier(Box<AstNodeId>),
}

impl std::fmt::Display for AstKindNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.format_node())
    }
}

impl Default for AstKindNode {
    fn default() -> Self {
        AstKindNode::Undefined
    }
}

impl AstKindNode {
    pub fn child_by_field_id(&self, field_id: u16) -> Option<ArenaIdNode> {
        let arena = ast_arena();
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
        ArenaIdNode(self.get_base().arena_id)
    }

    pub fn get_scope(&self) -> Option<ArenaIdScope> {
        match self {
            AstKindNode::Scope(node) => Some(node.scope),
            _ => None,
        }
    }

    pub fn get_symbol(&self) -> Option<ArenaIdSymbol> {
        match self {
            AstKindNode::Identifier(node) => Some(node.symbol),
            _ => None,
        }
    }

    pub fn get_base(&self) -> &AstNodeBase {
        match self {
            AstKindNode::Undefined => {
                panic!("should not happen")
            }
            AstKindNode::Root(node) => &node.base,
            AstKindNode::Text(node) => &node.base,
            AstKindNode::Internal(node) => &node.base,
            AstKindNode::Scope(node) => &node.base,
            AstKindNode::File(node) => &node.base,
            AstKindNode::Identifier(node) => &node.base,
        }
    }

    pub fn get_base_mut(&mut self) -> &mut AstNodeBase {
        match self {
            AstKindNode::Undefined => {
                panic!("should not happen")
            }
            AstKindNode::Root(node) => &mut node.base,
            AstKindNode::Text(node) => &mut node.base,
            AstKindNode::Internal(node) => &mut node.base,
            AstKindNode::Scope(node) => &mut node.base,
            AstKindNode::File(node) => &mut node.base,
            AstKindNode::Identifier(node) => &mut node.base,
        }
    }

    fn format_node(&self) -> String {
        match self {
            AstKindNode::Undefined => "undefined".into(),
            AstKindNode::Root(node) => {
                format!("root [{}]", node.base.id)
            }
            AstKindNode::Text(node) => {
                format!("text [{}] \"{}\"", node.base.id, node.text)
            }
            AstKindNode::Internal(node) => {
                format!(
                    "internal [{}] (parent: {:?})",
                    node.base.id, node.base.parent
                )
            }
            AstKindNode::Scope(node) => {
                let arena = ast_arena();
                let symbol_count = arena
                    .get_scope(node.scope)
                    .map(|s| s.symbols.len())
                    .unwrap_or(0);
                format!("scope [{}], {} symbols", node.base.id, symbol_count)
            }
            AstKindNode::File(node) => {
                format!("file [{}]", node.base.id)
            }
            AstKindNode::Identifier(node) => {
                let arena = ast_arena();
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

    fn set_parent(&mut self, parent: ArenaIdNode) {
        self.get_base_mut().parent = Some(parent);
    }

    fn get_child(&self, index: usize) -> Option<ArenaIdNode> {
        self.get_base().children.get(index).copied()
    }

    fn add_child(&mut self, child: ArenaIdNode) {
        self.get_base_mut().children.push(child)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AstNodeBase {
    pub arena_id: usize,
    pub debug_id: usize,
    pub token_id: u16,
    pub field_id: u16,
    pub kind: AstKind,
    pub start_pos: AstPoint,
    pub end_pos: AstPoint,
    pub start_byte: usize,
    pub end_byte: usize,
    pub parent: Option<ArenaIdNode>,
    pub children: Vec<ArenaIdNode>,
}

#[derive(Debug, Clone)]
pub struct AstNode {
    pub base: AstNodeBase,
    pub name: Option<AstNodeId>,
}

impl AstNode {
    pub fn new_with_name(base: AstNodeBase, name: Option<AstNodeId>) -> ArenaIdNode {
        let internal = Self { base, name };
        ast_arena_mut().add_node(AstKindNode::Internal(Box::new(internal)))
    }

    pub fn new(base: AstNodeBase) -> ArenaIdNode {
        Self::new_with_name(base, None)
    }
}

#[derive(Debug, Clone)]
pub struct AstNodeId {
    pub base: AstNodeBase,
    pub symbol: ArenaIdSymbol,
}

impl AstNodeId {
    pub fn new(base: AstNodeBase, symbol: ArenaIdSymbol) -> ArenaIdNode {
        let id = Self { base, symbol };
        ast_arena_mut().add_node(AstKindNode::Identifier(Box::new(id)))
    }
}

#[derive(Debug, Clone, Default)]
struct AstPoint {
    row: usize,
    col: usize,
}

impl From<Point> for AstPoint {
    fn from(point: Point) -> Self {
        Self {
            row: point.row,
            col: point.column,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AstNodeText {
    pub base: AstNodeBase,
    pub text: String,
}

impl AstNodeText {
    fn new(base: AstNodeBase, text: String) -> ArenaIdNode {
        let text = Self { base, text };
        ast_arena_mut().add_node(AstKindNode::Text(Box::new(text)))
    }
}

#[derive(Debug, Clone)]
pub struct AstNodeError {
    pub error_place: AstPoint,
}

#[derive(Debug, Clone, Default)]
pub struct AstFileId {
    pub path: Option<String>,
    pub content: Option<Vec<u8>>,
    pub content_hash: u64,
}

impl AstFileId {
    pub fn new_path(path: String) -> Self {
        AstFileId {
            path: Some(path),
            content: None,
            content_hash: 0,
        }
    }

    pub fn new_content(content: Vec<u8>) -> Self {
        let mut hasher = DefaultHasher::new();
        hasher.write(&content);
        let content_hash = hasher.finish();

        AstFileId {
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
pub struct AstFile {
    // TODO: add cache and all other stuff
    pub file: AstFileId,
}

impl AstFile {
    fn new_source(source: Vec<u8>) -> Self {
        AstFile {
            file: AstFileId::new_content(source),
        }
    }

    fn get_text(&self, start: usize, end: usize) -> Option<String> {
        self.file.get_text(start, end)
    }
}

#[derive(Debug, Clone)]
pub struct AstNodeFile {
    pub base: AstNodeBase,
    pub file: AstFile,
}

impl AstNodeFile {
    pub fn new(base: AstNodeBase, file: AstFile) -> ArenaIdNode {
        let file = Self { base, file };
        ast_arena_mut().add_node(AstKindNode::File(Box::new(file)))
    }
}

#[derive(Debug, Clone)]
pub struct AstNodeScope {
    pub base: AstNodeBase,
    pub scope: ArenaIdScope,
    pub name: Option<ArenaIdNode>,
}

impl AstNodeScope {
    fn new(base: AstNodeBase, scope: ArenaIdScope, name: Option<ArenaIdNode>) -> ArenaIdNode {
        let scope = Self { base, scope, name };
        ast_arena_mut().add_node(AstKindNode::Scope(Box::new(scope)))
    }
}

#[derive(Debug, Clone, Default)]
pub struct AstNodeRoot {
    children: Vec<ArenaIdNode>,
}

impl AstNodeRoot {
    fn new() -> Self {
        Self { children: vec![] }
    }
}

impl NodeTrait for AstKindNode {
    fn get_child(&self, index: usize) -> Option<ArenaIdNode> {
        self.get_child(index)
    }

    fn child_count(&self) -> usize {
        self.get_base().children.len()
    }
}
