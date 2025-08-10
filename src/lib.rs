// pub mod block;
mod lang;
pub mod visit;

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{DefaultHasher, Hasher};
use std::num::NonZeroU16;
use std::rc::Rc;
use std::{panic, vec};

pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

pub use crate::visit::*;

pub type AstArenaShare<T> = Rc<RefCell<AstArena<T>>>;

#[derive(Debug, Default)]
pub struct AstArena<T> {
    nodes: Vec<T>,
}

impl<T: Default> AstArena<T> {
    pub fn new() -> AstArenaShare<T> {
        // NOTE: id 1 is resvered for root node
        Rc::new(RefCell::new(Self {
            nodes: vec![T::default()],
        }))
    }

    pub fn add(&mut self, node: T) -> usize {
        self.nodes.push(node);
        self.nodes.len() - 1
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.nodes.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.nodes.get_mut(index)
    }

    pub fn get_next_id(&self) -> usize {
        self.nodes.len()
    }
}

#[derive(Debug, Clone, Default)]
struct AstScope {
    // The symbol defines this scope
    owner: Box<AstSymbol>,
    // Parent index inside a scope stack
    parent: Option<usize>,
    // all symbols in this scope
    symbols: HashMap<String, Box<AstSymbol>>,
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

    fn global() -> Self {
        Self {
            ..Default::default()
        }
    }

    fn set_ast_node(&mut self, ast_node: usize) {
        self.ast_node = Some(ast_node);
    }

    fn add_symbol(&mut self, name: String, symbol: Box<AstSymbol>) {
        self.symbols.insert(name, symbol);
    }
}

#[derive(Debug)]
struct AstScopeStack {
    scopes: Vec<usize>,
    current_scope: usize,
}

impl<'a> AstScopeStack {
    fn new() -> Self {
        Self {
            scopes: vec![],
            current_scope: 0,
        }
    }

    fn enter_scope(&mut self, arena: AstArenaShare<AstKindNode>, index: usize) {
        let mut arena = arena.borrow_mut();
        let node = arena.get_mut(index).unwrap();
        let scope = node.get_scope_mut().unwrap();

        scope.parent = Some(self.current_scope);
        self.scopes.push(node.get_id());
        self.current_scope = self.scopes.len() - 1;
    }

    fn leave_scope(&mut self, arena: AstArenaShare<AstKindNode>) {
        let index = self.scopes[self.current_scope];
        let mut arena = arena.borrow_mut();
        let node = arena.get_mut(index).unwrap();
        let scope = node.get_scope_mut().unwrap();

        if let Some(parent) = scope.parent {
            self.current_scope = parent;
        } else {
            panic!("already at root scope");
        }
    }

    fn add_symbol(&mut self, arena: AstArenaShare<AstKindNode>, symbol: &Box<AstSymbol>) {
        let index = self.scopes[self.current_scope];
        let mut arena = arena.borrow_mut();
        let node = arena.get_mut(index).unwrap();
        let scope = node.get_scope_mut().unwrap();

        scope.add_symbol(symbol.mangled_name.clone(), symbol.clone());
    }

    fn lookup(
        &self,
        arena: AstArenaShare<AstKindNode>,
        mangled_name: &str,
    ) -> Option<Box<AstSymbol>> {
        let mut current = self.current_scope;
        loop {
            let index = self.scopes[current];
            let mut arena = arena.borrow_mut();
            let node = arena.get_mut(index).unwrap();
            let scope = node.get_scope_mut().unwrap();

            if let Some(sym) = scope.symbols.get(mangled_name) {
                return Some(sym.clone());
            }
            if current == 0 {
                break;
            }
            if let Some(parent) = scope.parent {
                current = parent;
            }
        }
        None
    }
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
    // The ast node defines this symbol,
    defined: Option<Box<AstSymbol>>,
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
    // The block defining this symbol,
    // block: Option<Box<BasicBlock>>,
}

impl AstSymbol {
    fn new(token_id: u16, name: String) -> Box<AstSymbol> {
        Box::new(AstSymbol {
            token_id: AstToken::new(token_id),
            name,
            ..Default::default()
        })
    }
}

#[derive(Debug, Clone, Default)]
struct AstNodeId {
    base: AstNodeBase,
    symbol: Box<AstSymbol>,
}

impl AstNodeId {
    fn new(base: AstNodeBase, symbol: Box<AstSymbol>) -> Self {
        Self { base, symbol }
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
    arena: AstArenaShare<AstKindNode>,
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

#[derive(Debug, Clone, Default)]
struct AstNodeText {
    base: AstNodeBase,
    text: String,
}

impl AstNodeText {
    fn new(base: AstNodeBase, text: String) -> Self {
        Self { base, text }
    }
}

#[derive(Debug, Clone, Default)]
struct AstNode {
    base: AstNodeBase,
    name: Option<Box<AstNodeId>>,
}

impl AstNode {
    fn new(base: AstNodeBase, name: Option<Box<AstNodeId>>) -> Self {
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

#[derive(Debug, Clone, Default)]
struct AstNodeFile {
    base: AstNodeBase,
    scope: AstScope,
    // file: AstFile,
}

impl AstNodeFile {
    fn new(base: AstNodeBase, scope: AstScope) -> Self {
        Self { base, scope }
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
    name: Option<Box<AstNodeId>>,
}

impl AstNodeScope {
    fn new(base: AstNodeBase, scope: AstScope, name: Option<Box<AstNodeId>>) -> Self {
        Self { base, scope, name }
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

#[derive(Debug, Clone, EnumIter, EnumString)]
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
    pub fn get_id(&self) -> usize {
        self.get_base().id
    }

    pub fn get_scope_mut(&mut self) -> Option<&mut AstScope> {
        match self {
            AstKindNode::Scope(node) => Some(&mut node.scope),
            _ => None,
        }
    }

    pub fn get_base(&self) -> &AstNodeBase {
        match self {
            AstKindNode::Undefined => {
                panic!("shoud not happen")
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
                panic!("shoud not happen")
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
                    "internal [{}] ({})",
                    node.base.id,
                    node.base.parent.unwrap()
                )
            }
            AstKindNode::Scope(node) => {
                format!("scope [{}]", node.base.id)
            }
            AstKindNode::File(node) => {
                format!("file [{}]", node.base.id)
            }
            AstKindNode::Identifier(node) => {
                format!(
                    "identifier_use [{}] '{}', '{}'",
                    node.base.id, node.symbol.name, node.symbol.mangled_name
                )
            }
        }
    }

    fn set_parent(&mut self, parent: usize) {
        self.get_base_mut().parent = Some(parent);
    }

    fn get_child(&self, index: usize) -> Option<usize> {
        self.get_base().children.get(index).cloned()
    }

    fn add_child(&mut self, child: usize) {
        self.get_base_mut().children.push(child)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AstNodeRoot {
    base: AstNodeBase,
    arena: AstArenaShare<AstKindNode>,
    children: Vec<usize>,
}

impl AstNodeRoot {
    fn new(arena: AstArenaShare<AstKindNode>) -> Self {
        let mut base = AstNodeBase::default();
        base.id = 1;
        Self {
            base,
            arena,
            children: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct AstTree {
    root: AstKindNode,
}

impl AstTree {
    fn new(root: Box<AstNodeRoot>) -> Self {
        AstTree {
            root: AstKindNode::Root(root),
        }
    }
}

impl NodeTrait for AstKindNode {
    fn get_child(&self, index: usize) -> Option<usize> {
        self.get_child(index)
    }

    fn child_count(&self) -> usize {
        self.get_base().children.len()
    }
}

pub type AstTreeCursor<'a> = CursorGeneric<'a, AstKindNode>;

#[derive(Debug)]
pub struct AstContext {
    language: AstLanguage,
    file: AstFile,
    arena: AstArenaShare<AstKindNode>,
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
    #[strum(serialize = ":")]
    Text_COLON = 11,
    #[strum(serialize = ",")]
    Text_COMMA = 83,
    #[strum(serialize = "->")]
    Text_ARROW = 85,

    integer_literal = 127,
    identifier = 1,
    parameter = 213,
    parameters = 210,
    let_declaration = 203,
    block = 293,
    source_file = 157,
    function_item = 188,
    mutable_specifier = 122,
    expression_statement = 160,
    assignment_expression = 251,
    binary_expression = 250,
    operator = 14,
    call_expression = 256,
    arguments = 257,
    primitive_type = 32,
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
            AstTokenRust::expression_statement => AstKind::Internal,
            AstTokenRust::assignment_expression => AstKind::Internal,
            AstTokenRust::binary_expression => AstKind::Internal,
            AstTokenRust::operator => AstKind::Internal,
            AstTokenRust::call_expression => AstKind::Internal,
            AstTokenRust::arguments => AstKind::Internal,
            AstTokenRust::primitive_type => AstKind::Internal,
            AstTokenRust::parameters => AstKind::Internal,
            AstTokenRust::parameter => AstKind::Internal,
            AstTokenRust::identifier => AstKind::IdentifierUse,
            AstTokenRust::integer_literal => AstKind::Text,
            AstTokenRust::mutable_specifier => AstKind::Text,
            AstTokenRust::Text_fn
            | AstTokenRust::Text_LPAREN
            | AstTokenRust::Text_RPAREN
            | AstTokenRust::Text_LBRACE
            | AstTokenRust::Text_RBRACE
            | AstTokenRust::Text_let
            | AstTokenRust::Text_EQ
            | AstTokenRust::Text_ARROW
            | AstTokenRust::Text_COLON
            | AstTokenRust::Text_COMMA
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
        AstTokenRust::from_repr(token_id)
            .expect(&format!("unknown token id: {}", token_id))
            .into()
    }

    fn get_name_child<'a>(&self, node: &Node<'a>) -> Option<Node<'a>> {
        node.child_by_field_id(AstFieldRust::name as u16)
    }

    fn get_name_field_id(&self) -> u16 {
        AstFieldRust::name as u16
    }

    fn upgrade_identifier(&self, token_id: u16) -> Option<AstKind> {
        match AstTokenRust::from_repr(token_id) {
            Some(AstTokenRust::function_item) => {
                return Some(AstKind::IdentifierDef);
            }
            _ => None,
        }
    }

    fn mangled_name(&self, name: &mut Box<AstNodeId>, scope_stack: &AstScopeStack) {}
}

#[derive(Debug)]
struct AstBuilder<'a> {
    stack: Vec<usize>,
    context: &'a mut AstContext,
    arena: AstArenaShare<AstKindNode>,
}

impl<'a> AstBuilder<'a> {
    fn new(context: &'a mut AstContext, arena: AstArenaShare<AstKindNode>) -> Self {
        let root = AstKindNode::Root(Box::new(AstNodeRoot::new(arena.clone())));
        let root_id = arena.borrow_mut().add(root);
        Self {
            stack: vec![root_id],
            context: context,
            arena,
        }
    }

    fn root_node(&self) -> Box<AstNodeRoot> {
        assert!(!self.stack.is_empty());
        let id = self.stack[self.stack.len() - 1];
        let node = self.arena.borrow().get(id).cloned().unwrap();
        match node {
            AstKindNode::Root(node) => node.clone(),
            _ => panic!("should not happen"),
        }
    }

    fn step_to_name_child(&mut self, node: &Node, node_id: usize) -> Option<Box<AstNodeId>> {
        let child = self.context.language.get_name_child(node)?;
        let name_id = self.context.language.get_name_field_id();
        let start = child.start_byte();
        let end = child.end_byte();
        let text = self.context.file.get_text(start, end).unwrap();
        let base = self.create_base_node(&child, node_id, name_id);
        let symbol = AstSymbol::new(node.kind_id(), text);
        let ast_node = AstNodeId::new(base, symbol);
        Some(Box::new(ast_node))
    }

    fn create_ast_node(&mut self, base: AstNodeBase, kind: AstKind, node: &Node) -> usize {
        let arena = self.arena.borrow();
        let node_id = arena.get_next_id();
        drop(arena);

        match kind {
            AstKind::File => {
                let symbol = AstSymbol::new(base.token_id, "FILE_SCOPE".into());
                let mut scope = AstScope::new(symbol);
                scope.ast_node = Some(node_id);

                let file = AstKindNode::File(Box::new(AstNodeFile::new(base, scope)));
                self.arena.borrow_mut().add(file)
            }
            AstKind::Text => {
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                self.arena
                    .borrow_mut()
                    .add(AstKindNode::Text(Box::new(AstNodeText::new(
                        base,
                        text.unwrap(),
                    ))))
            }
            AstKind::Internal => {
                let name = self.step_to_name_child(node, node_id);
                self.arena
                    .borrow_mut()
                    .add(AstKindNode::Internal(Box::new(AstNode::new(base, name))))
            }
            AstKind::Scope => {
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                let symbol = AstSymbol::new(base.token_id, text.unwrap());
                let mut scope = AstScope::new(symbol);
                scope.ast_node = Some(node_id);

                let name = self.step_to_name_child(node, node_id);

                let mut arena = self.arena.borrow_mut();
                arena.add(AstKindNode::Scope(Box::new(AstNodeScope::new(
                    base, scope, name,
                ))))
            }
            AstKind::IdentifierUse => {
                let mut arena = self.arena.borrow_mut();
                let text = self.context.file.get_text(base.start_byte, base.end_byte);
                let text = text.unwrap();
                let symbol = AstSymbol::new(base.token_id, text);
                let ast = AstNodeId::new(base, symbol);
                arena.add(AstKindNode::Identifier(Box::new(ast)))
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
            arena: self.arena.clone(),
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

        let id = self.arena.borrow().get_next_id();
        let base = self.create_base_node(&node, id, field_id.into());
        let child = self.create_ast_node(base, kind, &node);
        debug_assert!(id == child);

        let parent = self.stack[self.stack.len() - 1];
        let mut arena_mut = self.arena.borrow_mut();
        arena_mut.get_mut(parent).unwrap().add_child(child);
        arena_mut.get_mut(child).unwrap().set_parent(parent);

        // Push this node onto the stack if it can have children
        if node.child_count() > 0 {
            self.stack.push(child);
        }
    }

    fn visit_leave_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();

        // Pop the current node from the stack when we're done with it
        if node.child_count() > 0 {
            if let Some(_completed_node) = self.stack.pop() {
                // let mut arena_mut = self.arena.borrow_mut();
                // arena_mut.get_mut(completed_node).unwrap().add_child(child);
                // self.finalize_node(&completed_node);
            }
        }
    }
}

#[derive(Debug)]
struct AstPrinter<'a> {
    context: &'a AstContext,
    depth: usize,
    output: String,
}

impl<'a> AstPrinter<'a> {
    fn new(context: &'a AstContext) -> Self {
        Self {
            context,
            depth: 0,
            output: String::new(),
        }
    }

    fn get_output(&self) -> &str {
        &self.output
    }

    fn print_output(&self) {
        println!("{}", self.output);
    }
}

impl<'a> Visitor<AstTreeCursor<'a>> for AstPrinter<'a> {
    fn visit_enter_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        let node = cursor.node();

        self.output.push_str(&"  ".repeat(self.depth));
        self.output.push('(');
        let base = node.get_base();
        let text = self.context.file.get_text(base.start_byte, base.end_byte);
        if let Some(mut text) = text {
            text = text.replace("\n", "");
            self.output.push_str(&format!("{} |{}|", node, text));
        } else {
            self.output.push_str(&format!("{}", node));
        }

        if node.child_count() == 0 {
            self.output.push(')');
        } else {
            self.output.push('\n');
        }

        self.depth += 1;
    }

    fn visit_leave_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        self.depth -= 1;
        let node = cursor.node();

        if node.child_count() > 0 {
            self.output.push_str(&"  ".repeat(self.depth));
            self.output.push(')');
        }

        if self.depth > 0 {
            self.output.push('\n');
        }
    }

    fn visit_node(&mut self, _cursor: &mut AstTreeCursor<'a>) {}
}

pub fn print_llmcc_ast(_tree: &AstTree, context: &AstContext, arena: AstArenaShare<AstKindNode>) {
    let mut arena = arena.borrow_mut();

    let mut vistor = AstPrinter::new(context);
    let mut cursor = AstTreeCursor::new(&mut *arena);
    dfs(&mut cursor, &mut vistor);
    vistor.print_output();
}

pub fn build_llmcc_ast(
    tree: &Tree,
    context: &mut AstContext,
    arena: AstArenaShare<AstKindNode>,
) -> Result<AstTree, Box<dyn std::error::Error>> {
    let mut vistor = AstBuilder::new(context, arena);
    let mut cursor = tree.walk();
    dfs(&mut cursor, &mut vistor);
    Ok(AstTree::new(vistor.root_node()))
}

#[derive(Debug)]
struct AstSymbolCollector<'a> {
    context: &'a AstContext,
    scope_stack: AstScopeStack,
}

impl<'a> AstSymbolCollector<'a> {
    fn new(context: &'a AstContext) -> Self {
        Self {
            context,
            scope_stack: AstScopeStack::new(),
        }
    }

    fn upgrade_identifier_if_any(&self, token_id: u16, name: &mut Box<AstNodeId>) {
        let change_to = self.context.language.upgrade_identifier(token_id);
        if let Some(change) = change_to {
            name.base.kind = change;
        }
    }

    fn mangled_name(&self, name: &mut Box<AstNodeId>) {
        self.context.language.mangled_name(name, &self.scope_stack);
    }
}

impl<'a> Visitor<AstTreeCursor<'a>> for AstSymbolCollector<'a> {
    fn visit_enter_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        match cursor.node() {
            AstKindNode::Scope(node) => {
                self.scope_stack
                    .enter_scope(node.base.arena.clone(), node.base.id);
            }
            _ => {}
        }
    }

    fn visit_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        let node = cursor.node();
        let base = node.get_base();
        let token_id = base.token_id;
        let arena = base.arena.clone();
        match node {
            AstKindNode::Scope(node) => {
                if let Some(mut name) = node.name.as_mut() {
                    self.upgrade_identifier_if_any(token_id, &mut name);
                    self.mangled_name(&mut name);
                    self.scope_stack.add_symbol(arena, &name.symbol);
                }
            }
            AstKindNode::Internal(node) => {
                if let Some(mut name) = node.name.as_mut() {
                    self.upgrade_identifier_if_any(token_id, &mut name);
                    self.mangled_name(&mut name);
                    self.scope_stack.add_symbol(arena, &name.symbol);
                }
            }
            AstKindNode::Identifier(node) => {
                match node.base.kind {
                    AstKind::IdentifierDef
                    | AstKind::IdentifierFieldDef
                    | AstKind::IdentifierTypeDef => {
                        self.mangled_name(node);
                        self.scope_stack.add_symbol(arena, &node.symbol);
                    }
                    AstKind::IdentifierUse
                    | AstKind::IdentifierTypeUse
                    | AstKind::IdentifierFieldUse => {
                        // Do nothing here in declaration pass
                    }
                    _ => unimplemented!(),
                }
            }
            _ => {}
        }
    }

    fn visit_leave_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        match cursor.node() {
            AstKindNode::Scope(node) => {
                // TODO: need to assgin scope back?
                // scope_node.scope = self.scope_stack.current_scope();
                self.scope_stack.leave_scope(node.base.arena.clone());
            }
            _ => {}
        }
    }
}

struct AstSymbolBinder<'a> {
    context: &'a AstContext,
    scope_stack: AstScopeStack,
}

impl<'a> AstSymbolBinder<'a> {
    fn new(context: &'a AstContext, scope_stack: AstScopeStack) -> Self {
        Self {
            context,
            scope_stack,
        }
    }

    fn resolve_symbol(
        &self,
        arena: AstArenaShare<AstKindNode>,
        name: &Box<AstSymbol>,
    ) -> Option<Box<AstSymbol>> {
        self.scope_stack.lookup(arena, &name.mangled_name)
    }
}

impl<'a> Visitor<AstTreeCursor<'a>> for AstSymbolBinder<'a> {
    fn visit_enter_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        match cursor.node() {
            AstKindNode::Scope(node) => {
                self.scope_stack
                    .enter_scope(node.base.arena.clone(), node.base.id);
            }
            _ => {}
        }
    }

    fn visit_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        let node = cursor.node();
        match node {
            AstKindNode::Identifier(node) => {
                match node.base.kind {
                    AstKind::IdentifierUse => {
                        let arena = node.base.arena.clone();
                        if let Some(define) = self.resolve_symbol(arena, &node.symbol) {
                            node.symbol.defined = Some(define);
                        } else {
                            // Optionally handle unresolved symbol error or warning
                        }
                    }
                    AstKind::IdentifierTypeUse => {
                        let arena = node.base.arena.clone();
                        if let Some(type_of) = self.resolve_symbol(arena, &node.symbol) {
                            node.symbol.type_of = Some(type_of);
                        } else {
                            // Optionally handle unresolved symbol error or warning
                        }
                    }
                    AstKind::IdentifierFieldUse => {
                        let arena = node.base.arena.clone();
                        if let Some(filed_of) = self.resolve_symbol(arena, &node.symbol) {
                            node.symbol.field_of = Some(filed_of);
                        } else {
                            // Optionally handle unresolved symbol error or warning
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    fn visit_leave_node(&mut self, cursor: &mut AstTreeCursor<'a>) {
        match cursor.node() {
            AstKindNode::Scope(node) => {
                self.scope_stack.leave_scope(node.base.arena.clone());
            }
            _ => {}
        }
    }
}

pub fn bind_llmcc_ast(_tree: &AstTree, context: &AstContext, arena: AstArenaShare<AstKindNode>) {
    let mut arena_ref = arena.borrow_mut();
    let mut collector = AstSymbolCollector::new(context);
    let mut cursor = AstTreeCursor::new(&mut *arena_ref);
    dfs(&mut cursor, &mut collector);

    let mut arena_ref = arena.borrow_mut();
    let mut binder = AstSymbolBinder::new(context, collector.scope_stack);
    let mut cursor = AstTreeCursor::new(&mut *arena_ref);
    dfs(&mut cursor, &mut binder);
}
