use std::{collections::HashMap, rc::Rc};
use tree_sitter::{Node, Parser, Point, TreeCursor};

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
}

// Terminal node, no children
#[derive(Debug, Clone)]
struct AstNodeText {
    base: AstNodeBase,
    parent: Box<AstNode>,
    text: String,
}

#[derive(Debug, Clone)]
struct AstNode {
    base: AstNodeBase,
    name: Option<AstNodeId>,
    parent: Box<AstNode>,
    children: Vec<AstKindNode>,
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
    file: AstFile,
}

#[derive(Debug, Clone)]
struct AstFile {
    id: AstFileId,
}

#[derive(Debug, Clone)]
struct AstNodeScope {}

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
    Text(AstNodeText),
    Internal(AstNode),
    Scope(AstNodeScope),
    File(AstNodeFile),
}

#[derive(Debug)]
struct AstLanguage {
    // Language-specific mappings
    // id <-> tree_sitter token enum mapping
    // id --> AstKind mapping
}

impl AstLanguage {
    fn new() -> Self {
        Self {}
    }

    fn get_token_kind(&self, token_id: u16) -> AstKind {
        AstKind::Undefined
    }
}

#[derive(Debug)]
struct AstContext {
    language: AstLanguage,
    file: AstFile,
}

// fn build_tree(
//     cursor: &mut TreeCursor,
//     context: &mut AstContext,
// ) -> Result<AstNode, Box<dyn std::error::Error>> {
//     let mut root = AstRootNode::new();
//     let mut stack = Vec::new();
//     let mut parent = root;
//     stack.append(root);
//     loop {
//         let node = cursor.node();
//         let token_id = node.kind_id();
//         let field_id = cursor.field_id().unwrap_or(0);
//         let kind = context.language.get_token_kind(token_id);

//         let base = AstNodeBase {
//             token_id,
//             field_id,
//             kind,
//             start_pos: node.start_position().into(),
//             end_pos: node.end_position().into(),
//             start_byte: node.start_byte(),
//             end_byte: node.end_byte(),
//         };

//         let ast_node = match kind {
//             AstKind::Text => AstNodeText::new(),
//             AstKind::File => AstNode::new(),
//             AstKind::Scope => AstScope::new(),
//             _ => AstNode::new(),
//         }

//         parent.add_child(ast_node);

//         if !cursor.goto_first_child() {
//             break;
//         }
//     }

//     return Ok(parent);
// }

use std::convert::TryFrom;
use strum_macros::{Display, EnumIter, EnumString, EnumVariantNames, IntoStaticStr};

#[repr(u8)]
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
    IntoStaticStr,
)]
#[strum(serialize_all = "snake_case")] // You can also use "camelCase", "kebab-case", etc.
pub enum AstTokenRust {
    #[strum(serialize = "AAAAAAAaa")]
    Foo = 1,

    Bar = 2,

    Baz = 3,
}

// Implement numeric -> enum
impl TryFrom<u8> for AstTokenRust {
    type Error = &'static str;
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(AstTokenRust::Foo),
            2 => Ok(AstTokenRust::Bar),
            3 => Ok(AstTokenRust::Baz),
            _ => Err("Invalid number for AstTokenRust"),
        }
    }
}

// Implement enum -> numeric
impl From<AstTokenRust> for u8 {
    fn from(val: AstTokenRust) -> Self {
        val as u8
    }
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
    let root = tree.root_node();

    println!(
        "Root node kind: {}\nStart: {:?}, End: {:?}",
        root.kind(),
        root.start_position(),
        root.end_position(),
    );
}
