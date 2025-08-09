mod lang;
pub mod visit;

use std::cell::RefCell;
use std::hash::{DefaultHasher, Hasher};
use std::num::NonZeroU16;
use std::{collections::HashMap, sync::Arc};
use std::{panic, vec};

pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

pub use crate::visit::*;

#[derive(Debug, Default)]
struct AstArena<T> {
    nodes: Vec<T>,
}

impl<T: Default> AstArena<T> {
    fn new() -> Self {
        Self {
            nodes: vec![T::default()],
        }
    }

    fn add(&mut self, node: T) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    fn get(&self, index: usize) -> Option<&T> {
        self.nodes.get(index)
    }

    fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.nodes.get_mut(index)
    }

    fn get_next_id(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug, Clone, Default)]
struct AstScope {
    // The symbol defines this scope
    owner: Box<AstSymbol>,
    // Base scopes,
    bases: Vec<Box<AstScope>>,
    // all symbols in this scope
    symbols: HashMap<String, AstSymbol>,
    // The ast node owns this scope
    ast_node: Option<usize>,
}

impl AstScope {
    fn new(owner: Box<AstSymbol>) -> AstScope {
        AstScope {
            owner,
            ..Default::default()
        }
    }

    fn set_ast_node(&mut self, ast_node: usize) {
        self.ast_node = Some(ast_node);
    }
}

#[derive(Debug, Clone)]
struct AstScopeStack {
    scopes: Vec<AstScope>,
}

#[derive(Debug, Clone)]
struct AstField {
    value: u16,
}

#[derive(Debug, Clone, Default)]
struct AstToken {
    value: u16,
}

impl AstToken {
    fn new(id: u16) -> Self {
        AstToken { value: id }
    }
}

#[derive(Debug, Clone)]
struct BasicBlock {
    _value: u16,
}

#[derive(Debug, Clone, Default)]
struct AstSymbol {
    //
    token_id: AstToken,
    // The name of the symbol
    name: String,
    // full mangled name, used for resolve symbols overlaods etc
    mangled_name: String,
    // The point from the source code
    origin: AstPoint,
    // The scope this symbol defines, if any (e.g., functions, classes)
    defines_scope: Option<Box<AstScope>>,
    // The scope where this symbol defined in,
    parent_scope: Option<Box<AstScope>>,
    // The type of this symbol, if any
    type_of: Option<Box<AstSymbol>>,
    // The field this symbol belongs to, if any
    field_of: Option<Box<AstSymbol>>,
    // The base this symbol derived from, if any
    base_symbol: Option<Box<AstSymbol>>,
    // All overloads for this symbol, if exists
    overloads: Vec<AstSymbol>,
    // The list of nested types inside this symbol
    nested_types: Vec<AstSymbol>,
    // The ast node defines this symbol,
    defined: Option<usize>,
    // The block defining this symbol,
    block: Option<Box<BasicBlock>>,
}

impl AstSymbol {
    fn new(token_id: u16, name: String) -> Box<AstSymbol> {
        Box::new(AstSymbol {
            token_id: AstToken::new(token_id),
            name,
            ..Default::default()
        })
    }

    fn set_defined(&mut self, defined: usize) {
        self.defined = Some(defined);
    }
}

#[derive(Debug, Clone)]
struct AstNodeId {
    base: AstNodeBase,
    name: String,
    mangled_name: String,
    symbol: Box<AstSymbol>,
}

impl AstNodeId {
    fn new(base: AstNodeBase, name: String, symbol: Box<AstSymbol>) -> Self {
        Self {
            base,
            name,
            mangled_name: "".into(),
            symbol,
        }
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
struct AstNodeBase {
    id: usize,
    token_id: u16,
    field_id: u16,
    kind: AstKind,
    start_pos: AstPoint,
    end_pos: AstPoint,
    start_byte: usize,
    end_byte: usize,
    parent: Option<usize>,
    children: Vec<usize>,
}

#[derive(Debug, Clone)]
struct AstNodeText {
    base: AstNodeBase,
    text: String,
}

impl AstNodeText {
    fn new(base: AstNodeBase, text: String) -> Self {
        Self { base, text }
    }
}

#[derive(Debug, Clone)]
struct AstNode {
    base: AstNodeBase,
    name: Option<AstKindNode>,
}

impl AstNode {
    fn new(base: AstNodeBase, name: Option<AstKindNode>) -> Self {
        Self { base, name }
    }
}

#[derive(Debug, Clone)]
struct AstNodeError {
    error_place: AstPoint,
}

#[derive(Debug, Clone, Default)]
struct AstFileId {
    path: Option<String>,
    content: Option<Vec<u8>>,
    /// Stores the hash of the content
    content_hash: u64,
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

        // Convert the byte slice to a String, replacing invalid UTF-8 sequences
        Some(String::from_utf8_lossy(slice).into_owned())
    }

    pub fn get_full_text(&self) -> Option<String> {
        let content_bytes = self.content.as_ref()?;
        Some(String::from_utf8_lossy(content_bytes).into_owned())
    }
}

#[derive(Debug, Clone)]
struct AstNodeFile {
    base: AstNodeBase,
    // file: AstFile,
}

impl AstNodeFile {
    fn new(base: AstNodeBase) -> Self {
        Self { base: base }
    }
}

#[derive(Debug, Clone)]
struct AstFile {
    // TODO: add cache and all other stuff
    file: AstFileId,
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

#[derive(Debug, Clone, Default)]
struct AstNodeScope {
    base: AstNodeBase,
    scope: AstScope,
}

impl AstNodeScope {
    fn new(base: AstNodeBase, scope: AstScope) -> Self {
        Self { base, scope }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AstKind {
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
pub enum AstKindNode {
    Undefined,
    Root(Box<AstNodeRoot>),
    Text(Box<AstNodeText>),
    Internal(Box<AstNode>),
    Scope(Box<AstNodeScope>),
    File(Box<AstNodeFile>),
    IdentifierUse(Box<AstNodeId>),
}

impl NodeTrait for AstKindNode {
    fn get_child(&self, index: usize) -> Option<&usize> {
        self.get_child(index)
    }

    fn child_count(&self) -> usize {
        self.child_count()
    }
}

impl Default for AstKindNode {
    fn default() -> Self {
        AstKindNode::Undefined
    }
}

impl AstKindNode {
    fn set_parent(&mut self, parent: usize) {
        match self {
            AstKindNode::Root(_) => {
                panic!("cannot set a parent to root node.");
            }
            AstKindNode::Internal(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Scope(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::File(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::IdentifierUse(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Text(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Undefined => {
                panic!("cannot set a parent ton Undefined node.");
            }
        }
    }

    fn get_child(&self, index: usize) -> Option<&usize> {
        match self {
            AstKindNode::Root(node) => node.children.get(index),
            AstKindNode::Internal(node) => node.base.children.get(index),
            AstKindNode::Scope(node) => node.base.children.get(index),
            AstKindNode::File(node) => node.base.children.get(index),
            AstKindNode::IdentifierUse(node) => node.base.children.get(index),
            AstKindNode::Text(node) => node.base.children.get(index),
            AstKindNode::Undefined => {
                panic!("cannot set a parent ton Undefined node.");
            }
        }
    }

    fn get_id(&self) -> usize {
        match self {
            AstKindNode::Root(_) => 0,
            AstKindNode::Internal(node) => node.base.id,
            AstKindNode::Scope(node) => node.base.id,
            AstKindNode::File(node) => node.base.id,
            AstKindNode::IdentifierUse(node) => node.base.id,
            AstKindNode::Text(node) => node.base.id,
            AstKindNode::Undefined => {
                panic!("cannot set a parent ton Undefined node.");
            }
        }
    }

    fn child_count(&self) -> usize {
        match self {
            AstKindNode::Root(node) => node.children.len(),
            AstKindNode::Internal(node) => node.base.children.len(),
            AstKindNode::Scope(node) => node.base.children.len(),
            AstKindNode::File(node) => node.base.children.len(),
            AstKindNode::IdentifierUse(node) => node.base.children.len(),
            AstKindNode::Text(node) => node.base.children.len(),
            AstKindNode::Undefined => {
                panic!("cannot set a parent ton Undefined node.");
            }
        }
    }

    fn add_child(&mut self, child: usize) {
        match self {
            AstKindNode::Root(node) => {
                node.children.push(child);
            }
            AstKindNode::Internal(node) => {
                node.base.children.push(child);
            }
            AstKindNode::Scope(node) => {
                node.base.children.push(child);
            }
            AstKindNode::File(node) => {
                node.base.children.push(child);
            }
            AstKindNode::IdentifierUse(_) => {
                panic!("cannot add child to a identifier node.");
            }
            AstKindNode::Text(_) => {
                panic!("cannot add child to a Text node.");
            }
            AstKindNode::Undefined => {
                panic!("cannot add child to an Undefined node.");
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct AstNodeRoot {
    children: Vec<usize>,
}

impl AstNodeRoot {
    pub fn new() -> Self {
        Self { children: vec![] }
    }
}

#[derive(Debug, Clone)]
pub struct AstTree {
    root: AstKindNode,
}

impl AstTree {
    fn new(root: AstNodeRoot) -> Self {
        AstTree {
            root: AstKindNode::Root(root),
        }
    }
}

impl<'a> TreeTrait<'a> for AstTree {
    type Node = AstKindNode;
    type Cursor = CursorGeneric<'a, AstTree, AstKindNode>;

    fn root_node(&'a self) -> &'a Self::Node {
        &self.root
    }

    fn walk(&'a self) -> Self::Cursor {
        Self::Cursor::new(self)
    }
}

#[derive(Debug)]
pub struct AstContext {
    language: AstLanguage,
    file: AstFile,
    arena: AstArena<AstKindNode>,
}

impl AstContext {
    pub fn from_source(source: &[u8]) -> AstContext {
        AstContext {
            language: AstLanguage::new(),
            file: AstFile::new_source(source.to_vec()),
            arena: AstArena::new(),
        }
    }
}

use strum_macros::{Display, EnumIter, EnumString, EnumVariantNames, FromRepr, IntoStaticStr};

#[repr(u16)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumString,
    EnumIter,
    EnumVariantNames,
    Display,
    FromRepr,
    IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[allow(non_snake_case)]
pub enum AstTokenRust {
    #[strum(serialize = "fn")]
    Text_fn = 96,
    #[strum(serialize = "(")]
    Text_LPAREN = 4,
    #[strum(serialize = ")")]
    Text_RPAREN = 5,
    #[strum(serialize = "{")]
    Text_LBRACE = 8,
    #[strum(serialize = "}}")]
    Text_RBRACE = 9,
    #[strum(serialize = "let")]
    Text_let = 101,
    #[strum(serialize = "=")]
    Text_EQ = 70,
    #[strum(serialize = ";")]
    Text_SEMI = 2,
    integer_literal = 127,
    identifier = 1,
    parameters = 210,
    let_declaration = 203,
    block = 293,
    source_file = 157,
    function_item = 188,
}

#[repr(u16)]
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    EnumString,
    EnumIter,
    EnumVariantNames,
    Display,
    FromRepr,
    IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")]
#[allow(non_snake_case)]
pub enum AstFieldRust {
    #[strum(serialize = "name")]
    name = 19,
}

impl From<AstTokenRust> for AstKind {
    fn from(token: AstTokenRust) -> Self {
        match token {
            AstTokenRust::source_file => AstKind::File,
            AstTokenRust::function_item => AstKind::Scope,
            AstTokenRust::block => AstKind::Scope,
            AstTokenRust::let_declaration => AstKind::Internal,
            AstTokenRust::parameters => AstKind::Internal,
            AstTokenRust::identifier => AstKind::IdentifierUse,
            AstTokenRust::integer_literal => AstKind::Text,
            AstTokenRust::Text_fn
            | AstTokenRust::Text_LPAREN
            | AstTokenRust::Text_RPAREN
            | AstTokenRust::Text_LBRACE
            | AstTokenRust::Text_RBRACE
            | AstTokenRust::Text_let
            | AstTokenRust::Text_EQ
            | AstTokenRust::Text_SEMI => AstKind::Text,
        }
    }
}

#[derive(Debug)]
struct AstLanguage {}

impl AstLanguage {
    fn new() -> Self {
        Self {}
    }

    fn get_token_kind(&self, token_id: u16) -> AstKind {
        AstTokenRust::from_repr(token_id).unwrap().into()
    }

    fn get_name_child<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_id(AstFieldRust::name as u16)
    }

    fn get_name_field_id(&self) -> u16 {
        AstFieldRust::name as u16
    }
}

#[derive(Debug)]
struct AstBuilder<'a> {
    stack: Vec<usize>,
    context: &'a mut AstContext,
}

impl<'a> AstBuilder<'a> {
    fn new(context: &'a mut AstContext) -> Self {
        let root = AstKindNode::Root(AstNodeRoot::new());
        let root_id = context.arena.add(root);
        Self {
            stack: vec![root_id],
            context: context,
        }
    }

    fn root_node(&self) -> AstNodeRoot {
        assert!(!self.stack.is_empty());
        let id = self.stack[self.stack.len() - 1];
        let node = self.context.arena.get(id).cloned().unwrap();
        match node {
            AstKindNode::Root(node) => node.clone(),
            _ => panic!("should not happen"),
        }
    }

    fn step_to_name_child(&mut self, node: &Node, node_id: usize) -> Option<AstKindNode> {
        let child = self.context.language.get_name_child(node)?;
        let name_id = self.context.language.get_name_field_id();
        let start = child.start_byte();
        let end = child.end_byte();
        let text = self.context.file.get_text(start, end).unwrap();
        let base = self.create_base_node(&child, node_id, name_id);
        let symbol = AstSymbol::new(node.kind_id(), text.clone());
        let mut ast_node = AstNodeId::new(base, text.clone(), symbol);
        ast_node.symbol.set_defined(node_id);
        Some(AstKindNode::IdentifierUse(ast_node))
    }

    fn create_ast_node(&mut self, base: AstNodeBase, kind: AstKind, node: &Node) -> usize {
        match kind {
            AstKind::File => {
                let file = AstKindNode::File(Box::new(AstNodeFile::new(base)));
                self.context.arena.add(file)
            }
            AstKind::Text => {
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                self.context
                    .arena
                    .add(AstKindNode::Text(Box::new(AstNodeText::new(
                        base,
                        text.unwrap(),
                    ))))
            }
            AstKind::Internal => {
                let node_id = self.context.arena.get_next_id();
                let name = self.step_to_name_child(node, node_id);
                self.context
                    .arena
                    .add(AstKindNode::Internal(Box::new(AstNode::new(base, name))))
            }
            AstKind::Scope => {
                let node_id = self.context.arena.get_next_id();
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                let symbol = AstSymbol::new(base.token_id, text.unwrap());
                let mut scope = AstScope::new(symbol);
                scope.ast_node = Some(node_id);
                self.context
                    .arena
                    .add(AstKindNode::Scope(Box::new(AstNodeScope::new(base, scope))))
            }
            AstKind::IdentifierUse => {
                let node_id = self.context.arena.get_next_id();
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                let text = text.unwrap();
                let symbol = AstSymbol::new(base.token_id, text.clone());
                let mut ast = AstNodeId::new(base, text, symbol);
                ast.symbol.defined = Some(node_id);
                self.context
                    .arena
                    .add(AstKindNode::IdentifierUse(Box::new(ast)))
            }
            _ => {
                panic!("unknown kind: {:?}", node)
            }
        }
    }

    fn create_base_node(&self, node: &Node, id: usize, field_id: u16) -> AstNodeBase {
        let token_id = node.kind_id();
        let kind = self.context.language.get_token_kind(token_id);

        AstNodeBase {
            id,
            token_id,
            field_id: field_id.into(),
            kind,
            start_pos: node.start_position().into(),
            end_pos: node.end_position().into(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            parent: None,
            children: vec![],
        }
    }
}

impl<'a> Visitor<TreeCursor<'a>> for AstBuilder<'_> {
    fn visit_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();
        let token_id = node.kind_id();
        let field_id = cursor.field_id().unwrap_or(NonZeroU16::new(65535).unwrap());
        let kind = self.context.language.get_token_kind(token_id);
        let id = self.context.arena.get_next_id();
        let base = self.create_base_node(&node, id, field_id.into());
        let ast_node = self.create_ast_node(base, kind, &node);

        let parent = self.stack[self.stack.len() - 1];
        self.context
            .arena
            .get_mut(parent)
            .unwrap()
            .add_child(ast_node);

        // Push this node onto the stack if it can have children
        if node.child_count() > 0 {
            self.stack.push(ast_node);
        }
    }

    fn visit_leave_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();

        // Pop the current node from the stack when we're done with it
        if node.child_count() > 0 {
            if let Some(completed_node) = self.stack.pop() {
                // TODO: utilize this
                // self.finalize_node(&completed_node);
            }
        }
    }
}

pub fn build_llmcc_ast(
    tree: &Tree,
    context: &AstContext,
) -> Result<AstTree, Box<dyn std::error::Error>> {
    let mut vistor = AstBuilder::new(context);
    dfs(tree, &mut vistor);
    Ok(AstTree::new(vistor.root_node()))
}
