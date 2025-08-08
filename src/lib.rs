mod lang;
pub mod visit;

use std::hash::{DefaultHasher, Hasher};
use std::num::NonZeroU16;
use std::vec;
use std::{collections::HashMap, rc::Rc};
pub use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

pub use crate::visit::*;

#[derive(Debug, Clone)]
struct AstScope {
    // The symbol defines this scope
    owner: Option<Box<AstSymbol>>,
    // Base scopes,
    bases: Vec<Box<AstScope>>,
    // all symbols in this scope
    symbols: HashMap<String, AstSymbol>,
    // The ast node owns this scope
    root: AstNodeScope,
}

#[derive(Debug, Clone)]
struct AstScopeStack {
    scopes: Vec<AstScope>,
}

#[derive(Debug, Clone)]
struct AstField {
    value: u16,
}

#[derive(Debug, Clone)]
struct AstToken {
    value: u16,
}

#[derive(Debug, Clone)]
struct BasicBlock {
    _value: u16,
}

#[derive(Debug, Clone)]
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
    parent_scope: AstScope,
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
    defined: Option<Box<AstKindNode>>,
    // The block defining this symbol,
    block: Option<Box<BasicBlock>>,
}

#[derive(Debug, Clone)]
struct AstNodeId {
    base: AstNodeBase,
    name: String,
    mangled_name: String,
    symbol: Option<Box<AstSymbol>>,
}

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
struct AstNodeBase {
    token_id: u16,
    field_id: u16,
    kind: AstKind,
    start_pos: AstPoint,
    end_pos: AstPoint,
    start_byte: usize,
    end_byte: usize,
    parent: Option<AstKindNode>,
    children: Vec<AstKindNode>,
}

#[derive(Debug, Clone)]
struct AstNodeText {
    base: AstNodeBase,
    text: String,
}

impl AstNodeText {
    fn new(base: AstNodeBase, text: String) -> Box<AstNodeText> {
        Box::new(AstNodeText { base, text })
    }
}

#[derive(Debug, Clone)]
struct AstNode {
    base: AstNodeBase,
    name: Option<AstNodeId>,
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
    fn new(base: AstNodeBase) -> Box<AstNodeFile> {
        Box::new(AstNodeFile { base: base })
    }
}

#[derive(Debug, Clone)]
struct AstNodeLeaf {
    base: AstNodeBase,
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

#[derive(Debug, Clone)]
struct AstNodeScope {
    base: AstNodeBase,
    // scope: AstScope,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AstKind {
    Undefined,
    Error,
    File,
    Scope,
    Text,
    TextBlcok,
    Leaf,
    Internal,
    Comment,
    IdentifierUse,
    IdentifierTypeUse,
    IdentifierFieldUse,
    IdentifierDef,
    IdentifierTypeDef,
    IdentifierFieldDef,
}

#[derive(Debug, Clone)]
enum AstKindNode {
    Undefined,
    Root(Box<AstNodeRoot>),
    Text(Box<AstNodeText>),
    Internal(Box<AstNode>),
    Scope(Box<AstNodeScope>),
    File(Box<AstNodeFile>),
    Leaf(Box<AstNodeLeaf>),
    IdentifierUse(Box<AstNodeId>),
}

impl AstKindNode {
    fn set_parent(&mut self, parent: AstKindNode) {
        match self {
            AstKindNode::Root(_) => {
                panic!("Cannot set a parent to root node.");
            }
            AstKindNode::Internal(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Scope(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::File(_) => {
                panic!("Cannot set a parent to root node.");
            }
            AstKindNode::IdentifierUse(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Text(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Leaf(node) => {
                node.base.parent = Some(parent);
            }
            AstKindNode::Undefined => {
                panic!("Cannot set a parent ton Undefined node.");
            }
        }
    }

    fn add_child(&mut self, child: AstKindNode) {
        let mut child = child;
        child.set_parent(self.clone());

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
                panic!("Cannot add child to a identifier node.");
            }
            AstKindNode::Text(_) => {
                panic!("Cannot add child to a Text node.");
            }
            AstKindNode::Leaf(_) => {
                panic!("Cannot add child to a Leaf node.");
            }
            AstKindNode::Undefined => {
                panic!("Cannot add child to an Undefined node.");
            }
        }
    }
}

#[derive(Debug, Clone)]
struct AstNodeRoot {
    children: Vec<AstKindNode>,
}

impl AstNodeRoot {
    fn new() -> Self {
        AstNodeRoot { children: vec![] }
    }
}

#[derive(Debug)]
pub struct AstContext {
    language: AstLanguage,
    file: AstFile,
}

impl AstContext {
    pub fn from_source(source: &[u8]) -> Box<AstContext> {
        Box::new(AstContext {
            language: AstLanguage::new(),
            file: AstFile::new_source(source.to_vec()),
        })
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
    // source_file,157: 65535
    // function_item,188: 65535
    // fn,96: 65535
    // identifier,1: 19
    // parameters,210: 22
    // (,4: 65535
    // ),5: 65535
    // block,293: 5
    // {,8: 65535
    // let_declaration,203: 65535
    // let,101: 65535
    // identifier,1: 24
    // =,70: 65535
    // integer_literal,127: 31
    // ;,2: 65535
    // },9: 65535
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

impl From<AstTokenRust> for AstKind {
    /// Converts an `AstTokenRust` into its corresponding `AstKind`.
    /// This mapping is based on the semantic meaning of the Tree-sitter token/node kind.
    fn from(token: AstTokenRust) -> Self {
        match token {
            AstTokenRust::source_file => AstKind::File,
            AstTokenRust::function_item => AstKind::Scope, // Functions define a new scope
            AstTokenRust::block => AstKind::Scope,
            AstTokenRust::let_declaration => AstKind::Internal, // Represents a declaration structure
            AstTokenRust::parameters => AstKind::Internal, // Represents a structural part of a function

            // Identifiers: context determines if it's a definition or use.
            // For a generic mapping, we might default or refine later.
            // Here, we'll make an assumption for simplicity.
            AstTokenRust::identifier => AstKind::IdentifierUse,

            // Literal values are typically leaves in the AST
            AstTokenRust::integer_literal => AstKind::Leaf,

            // Text tokens (keywords, punctuation)
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
}

#[derive(Debug)]
struct AstBuilder {
    stack: Vec<AstKindNode>,
    context: Box<AstContext>,
}

impl AstBuilder {
    fn new(context: Box<AstContext>) -> Self {
        Self {
            stack: vec![AstKindNode::Root(Box::new(AstNodeRoot::new()))],
            context: context,
        }
    }

    fn get_root(&self) -> Box<AstNodeRoot> {
        assert!(!self.stack.is_empty());
        match self.stack.last().unwrap() {
            AstKindNode::Root(node) => node.clone(),
            _ => panic!("shoud not happen"),
        }
    }

    fn get_text(&self, base: &AstNodeBase) -> Option<String> {
        self.context.file.get_text(base.start_byte, base.end_byte)
    }

    fn create_ast_node(&self, base: AstNodeBase, kind: AstKind) -> AstKindNode {
        match kind {
            AstKind::File => AstKindNode::File(AstNodeFile::new(base)),
            AstKind::Text => {
                let text = self.get_text(&base).unwrap();
                AstKindNode::Text(AstNodeText::new(base, text))
            }
            // AstKind::Internal => AstKindNode::Internal(AstNode::new(base))
            _ => AstKindNode::Undefined,
        }
    }
}

impl<'a> Visitor<'a> for AstBuilder {
    fn visit_node(&mut self, cursor: &mut TreeCursor<'a>) {
        let node = cursor.node();
        let token_id = node.kind_id();
        let field_id = cursor.field_id().unwrap_or(NonZeroU16::new(65535).unwrap());
        let kind = self.context.language.get_token_kind(token_id);

        let base = AstNodeBase {
            token_id,
            field_id: field_id.into(),
            kind,
            start_pos: node.start_position().into(),
            end_pos: node.end_position().into(),
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            parent: None,
            children: vec![],
        };

        let ast_node = self.create_ast_node(base, kind);

        let parent = self.stack.last_mut().unwrap();
        parent.add_child(ast_node.clone());

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
                // self.finalize_node(completed_node);
            }
        }
    }
}

fn build_tree(
    tree: &Tree,
    context: Box<AstContext>,
) -> Result<Box<AstNodeRoot>, Box<dyn std::error::Error>> {
    let mut vistor = AstBuilder::new(context);
    dfs(&tree, &mut vistor);
    Ok(vistor.get_root())
}
