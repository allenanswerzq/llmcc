use std::panic;
use std::{collections::HashMap, marker::PhantomData};
use strum_macros::{Display, EnumIter, EnumString, EnumVariantNames, FromRepr, IntoStaticStr};

use crate::context::TyCtxt;
use crate::ir::{HirId, HirIdent, HirKind, HirNode};
use crate::symbol::{Scope, ScopeStack, SymId, Symbol};

#[derive(Debug)]
pub struct Language<'tcx> {
    ctx: &'tcx TyCtxt<'tcx>,
    scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> Language<'tcx> {
    pub fn new(ctx: &'tcx TyCtxt<'tcx>) -> Self {
        Self {
            ctx,
            scope_stack: ScopeStack::new(&ctx.arena),
        }
    }

    pub fn token_kind(&self, token_id: u16) -> HirKind {
        AstTokenRust::from_repr(token_id)
            .unwrap_or_else(|| panic!("unknown token {}", token_id))
            .into()
    }

    pub fn find_child_decl(&mut self, node: HirNode<'tcx>) {
        let children = node.children();
        for id in children {
            let child = self.ctx.hir_node(*id);
            self.find_decl(child);
        }
    }

    pub fn find_decl(&mut self, node: HirNode<'tcx>) {
        let token_id = node.token_id();
        let scope_depth = self.scope_stack.depth();

        match AstTokenRust::from_repr(token_id).unwrap() {
            AstTokenRust::function_item => {
                let ident = node.expect_ident_from_child(&self.ctx, AstFieldRust::name as u16);
                let sy = self.scope_stack.find_or_add(node.hir_id(), ident);
                *sy.mangled_name.borrow_mut() = "aaaa".to_string();
            }
            AstTokenRust::let_declaration => {
                let ident = node.expect_ident_from_child(&self.ctx, AstFieldRust::pattern as u16);
                let sy = self.scope_stack.find_or_add(node.hir_id(), ident);
            }
            AstTokenRust::block => {
                self.scope_stack.push_scope(node.hir_id());
            }
            AstTokenRust::parameter => {
                let ident = node.expect_ident_from_child(&self.ctx, AstFieldRust::pattern as u16);
                let sy = self.scope_stack.find_or_add(node.hir_id(), ident);
            }
            AstTokenRust::primitive_type | AstTokenRust::identifier => {
                // let ident = node.expect_ident();
                // let symbol = ctx.scope_stack.find(ident);
                // if let Some(sym) = symbol {
                //     sym.defined = Some(owner);
                // } else {
                // }
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

        self.find_child_decl(node);
        self.scope_stack.pop_until(scope_depth);
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

impl From<AstTokenRust> for HirKind {
    fn from(token: AstTokenRust) -> Self {
        match token {
            AstTokenRust::source_file => HirKind::File,
            AstTokenRust::function_item => HirKind::Scope,
            AstTokenRust::block => HirKind::Scope,
            AstTokenRust::let_declaration => HirKind::Internal,
            AstTokenRust::expression_statement => HirKind::Internal,
            AstTokenRust::assignment_expression => HirKind::Internal,
            AstTokenRust::binary_expression => HirKind::Internal,
            AstTokenRust::operator => HirKind::Internal,
            AstTokenRust::call_expression => HirKind::Internal,
            AstTokenRust::arguments => HirKind::Internal,
            AstTokenRust::primitive_type => HirKind::IdentUse,
            AstTokenRust::parameters => HirKind::Internal,
            AstTokenRust::parameter => HirKind::Internal,
            AstTokenRust::identifier => HirKind::IdentUse,
            AstTokenRust::integer_literal => HirKind::Text,
            AstTokenRust::mutable_specifier => HirKind::Text,
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
            | AstTokenRust::Text_SEMI => HirKind::Text,
        }
    }
}
