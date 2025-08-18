use std::panic;

use crate::{
    IrArena,
    arena::{NodeId, ScopeId},
    ir::{File, IrKind, IrKindNode, IrNodeId, IrTree},
    symbol::{Scope, ScopeStack},
};

use strum_macros::{Display, EnumIter, EnumString, EnumVariantNames, FromRepr, IntoStaticStr};

#[derive(Debug)]
pub struct AstContext {
    pub language: Language,
    pub file: File,
}

impl AstContext {
    pub fn from_source(source: &[u8]) -> AstContext {
        AstContext {
            language: Language::new(),
            file: File::new_source(source.to_vec()),
        }
    }
}

#[derive(Debug)]
pub struct Language {}

impl Language {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_token_kind(&self, token_id: u16) -> IrKind {
        AstTokenRust::from_repr(token_id)
            .expect(&format!("unknown token id: {}", token_id))
            .into()
    }

    pub fn find_child_declaration(
        &self,
        arena: &mut IrArena,
        scope_stack: &mut ScopeStack,
        node: IrKindNode,
    ) {
        let children = node.children(arena);
        for child in children {
            self.find_declaration(arena, scope_stack, child);
        }
    }

    pub fn find_declaration(
        &self,
        arena: &mut IrArena,
        scope_stack: &mut ScopeStack,
        mut node: IrKindNode,
    ) {
        let token_id = node.get_base().token_id;
        let scope_depth = scope_stack.scope_depth();

        match AstTokenRust::from_repr(token_id).unwrap() {
            AstTokenRust::function_item => {
                let name = node.unwrap_identifier(arena, AstFieldRust::name as u16);
                name.borrow_mut()
                    .upgrade_identifier_to_def(arena, node.get_id());
                scope_stack.find_or_add(arena, name);

                let sn = node.expect_scope();
                sn.borrow_mut().name = Some(node.get_id());

                // let symbol = self.arena.get_symbol_mut(symbol).unwrap();
                // TODO:
                // symbol.mangled_name =
            }
            AstTokenRust::let_declaration => {
                let name = node.unwrap_identifier(arena, AstFieldRust::pattern as u16);
                name.borrow_mut()
                    .upgrade_identifier_to_def(arena, node.get_id());
                scope_stack.find_or_add(arena, name.clone());

                let internal = node.expect_internal();
                internal.borrow_mut().name = Some(node.get_id());
            }
            AstTokenRust::block => {
                let new_scope = Scope::new(arena, None);
                scope_stack.enter_scope(arena, new_scope);
            }
            AstTokenRust::parameter => {
                let name = node.unwrap_identifier(arena, AstFieldRust::pattern as u16);
                name.borrow_mut()
                    .upgrade_identifier_to_def(arena, node.get_id());
                let symbol = scope_stack.find_or_add(arena, name);

                let internal = node.expect_internal();
                internal.borrow_mut().name = Some(node.get_id());
            }
            AstTokenRust::primitive_type | AstTokenRust::identifier => {
                let id = node.expect_identifier();
                let symbol = scope_stack.find(arena, id.clone());
                if let Some(found_id) = symbol {
                    let owner = arena.get_symbol_mut(found_id).unwrap().owner;
                    let symbol = arena.get_symbol_mut(id.borrow().symbol).unwrap();
                    // this is a use of symbol, and its defined by another symbol
                    symbol.defined = Some(owner);
                } else {
                    println!("not find symbol: {}", node.format_node(arena))
                }
            }
            AstTokenRust::source_file
            | AstTokenRust::mutable_specifier
            | AstTokenRust::parameters
            | AstTokenRust::integer_literal
            | AstTokenRust::expression_statement
            | AstTokenRust::assignment_expression
            | AstTokenRust::binary_expression
            | AstTokenRust::operator
            | AstTokenRust::call_expression
            | AstTokenRust::arguments
            | AstTokenRust::Text_ARROW
            | AstTokenRust::Text_EQ
            | AstTokenRust::Text_COMMA
            | AstTokenRust::Text_LBRACE
            | AstTokenRust::Text_LPAREN
            | AstTokenRust::Text_RBRACE
            | AstTokenRust::Text_RPAREN
            | AstTokenRust::Text_SEMI
            | AstTokenRust::Text_let
            | AstTokenRust::Text_fn
            | AstTokenRust::Text_COLON => {}
            _ => {
                panic!(
                    "unsupported {:?}",
                    AstTokenRust::from_repr(token_id).unwrap()
                )
            }
        }

        self.find_child_declaration(arena, scope_stack, node);
        scope_stack.pop_until(scope_depth);
    }
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
    pattern = 24,
}

impl From<AstTokenRust> for IrKind {
    fn from(token: AstTokenRust) -> Self {
        match token {
            AstTokenRust::source_file => IrKind::File,
            AstTokenRust::function_item => IrKind::Scope,
            AstTokenRust::block => IrKind::Scope,
            AstTokenRust::let_declaration => IrKind::Internal,
            AstTokenRust::expression_statement => IrKind::Internal,
            AstTokenRust::assignment_expression => IrKind::Internal,
            AstTokenRust::binary_expression => IrKind::Internal,
            AstTokenRust::operator => IrKind::Internal,
            AstTokenRust::call_expression => IrKind::Internal,
            AstTokenRust::arguments => IrKind::Internal,
            AstTokenRust::primitive_type => IrKind::IdentifierUse,
            AstTokenRust::parameters => IrKind::Internal,
            AstTokenRust::parameter => IrKind::Internal,
            AstTokenRust::identifier => IrKind::IdentifierUse,
            AstTokenRust::integer_literal => IrKind::Text,
            AstTokenRust::mutable_specifier => IrKind::Text,
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
            | AstTokenRust::Text_SEMI => IrKind::Text,
        }
    }
}
