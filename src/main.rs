use std::num::NonZeroU16;
use std::{collections::HashMap, rc::Rc};
use tree_sitter::{Node, Parser, Point, Tree, TreeCursor};

use llmcc::visit::{Visitor, print_ast};

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

#[derive(Debug, Clone)]
struct AstNode {
    base: AstNodeBase,
    name: Option<AstNodeId>,
}

#[derive(Debug, Clone)]
struct AstNodeError {
    error_place: AstPoint,
}

#[derive(Debug, Clone)]
struct AstFileId {
    path: String,
    content_hash: u64,
}

#[derive(Debug, Clone)]
struct AstNodeFile {
    base: AstNodeBase,
    file: AstFile,
}

#[derive(Debug, Clone)]
struct AstNodeLeaf {
    base: AstNodeBase,
}

#[derive(Debug, Clone)]
struct AstFile {
    id: AstFileId,
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
            // These node types typically do not have children.
            // You can choose to ignore the call, return a Result, or panic.
            // Here, we panic to indicate an invalid operation.
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
            // These node types typically do not have children.
            // You can choose to ignore the call, return a Result, or panic.
            // Here, we panic to indicate an invalid operation.
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
struct AstContext {
    language: AstLanguage,
    file: AstFile,
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

// let mut stack = Vec::new();
// let mut parent = root;
// stack.append(root);
// loop {
//     let node = cursor.node();
//     let token_id = node.kind_id();
//     let field_id = cursor.field_id().unwrap_or(0);
//     let kind = context.language.get_token_kind(token_id);

//     let base = AstNodeBase {
//         token_id,
//         field_id,
//         kind,
//         start_pos: node.start_position().into(),
//         end_pos: node.end_position().into(),
//         start_byte: node.start_byte(),
//         end_byte: node.end_byte(),
//     };

//     let ast_node = match kind {
//         AstKind::Text => AstNodeText::new(),
//         AstKind::File => AstNode::new(),
//         AstKind::Scope => AstScope::new(),
//         _ => AstNode::new(),
//     }

//     parent.add_child(ast_node);

//     if !cursor.goto_first_child() {
//         break;
//     }

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

    fn create_ast_node(base: &AstNodeBase, kind: AstKind) -> AstKindNode {
        AstKindNode::Undefined
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

        let ast_node = AstBuilder::create_ast_node(&base, kind);

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
    context: &mut AstContext,
) -> Result<AstNodeRoot, Box<dyn std::error::Error>> {
    let mut root = AstNodeRoot::new();

    return Ok(root);
}

fn main() {
    // // Enum -> number
    // let num: u8 = AstTokenRust::Foo.into();
    // println!("Enum to number: {}", num);

    // // Number -> enum
    // let e = AstTokenRust::try_from(1).unwrap();
    // println!("Number to enum: {}", e.to_string());

    // // // Enum -> string
    // // let s = e.to_string();
    // // println!("Enum to string: {}", s);

    // // // String -> enum
    // // let e2: AstTokenRust = "foo".parse().unwrap();
    // // println!("String to enum: {:?}", e2);
    let source_code = "fn example() { let x = 42; }";

    // Create a new parser
    let mut parser = Parser::new();

    // Set the language to Rust
    parser
        .set_language(&tree_sitter_rust::LANGUAGE.into())
        .expect("Error loading Rust grammar");

    // Parse the source code
    let tree = parser.parse(source_code, None).unwrap();
    print_ast(&tree);
}
