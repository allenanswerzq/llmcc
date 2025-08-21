use paste::paste;
use std::cell::RefCell;
use std::collections::HashMap;

use crate::context::TyCtxt;
use crate::ir::{HirId, HirIdent, HirKind, HirNode};
use crate::symbol::{Scope, ScopeStack, SymId, Symbol};

// ---------------- Token Definition Macro ----------------

macro_rules! define_tokens {
    (
        $( ($const:ident, $id:expr, $str:expr, $kind:expr) ),* $(,)?
    ) => {
        /// Language context for HIR processing
        #[derive(Debug)]
        pub struct Language<'tcx> {
            pub ctx: &'tcx TyCtxt<'tcx>,
        }

        impl<'tcx> Language<'tcx> {
            /// Create a new Language instance
            pub fn new(ctx: &'tcx TyCtxt<'tcx>) -> Self {
                Self { ctx }
            }

            // Generate token ID constants
            $(
                pub const $const: u16 = $id;
            )*

            /// Get the HIR kind for a given token ID
            pub fn hir_kind(token_id: u16) -> HirKind {
                match token_id {
                    $(
                        Self::$const => $kind,
                    )*
                    _ => HirKind::Internal,
                }
            }

            /// Get the string representation of a token ID
            pub fn token_str(token_id: u16) -> Option<&'static str> {
                match token_id {
                    $(
                        Self::$const => Some($str),
                    )*
                    _ => None,
                }
            }

            /// Check if a token ID is valid
            pub fn is_valid_token(token_id: u16) -> bool {
                matches!(token_id, $(Self::$const)|*)
            }
        }

        // ---------------- Visitor Trait ----------------

        /// Trait for visiting HIR nodes with type-specific dispatch
        pub trait HirVisitor<'tcx> {
            /// Visit a node, dispatching to the appropriate method based on token ID
            fn visit_node(&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
                match node.token_id() {
                    $(
                        Language::$const => paste::paste! { self.[<visit_ $const>](node, lang) },
                    )*
                    _ => self.visit_unknown(node, lang),
                }
            }

            /// Visit all children of a node
            fn visit_children(&mut self, node: &HirNode<'tcx>, lang: &mut Language<'tcx>) {
                for id in node.children() {
                    let child = lang.ctx.hir_node(*id);
                    self.visit_node(child, lang);
                }
            }

            /// Handle unknown/unrecognized token types
            fn visit_unknown(&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
                self.visit_children(&node, lang);
            }

            // Generate visit methods for each token type with visit_ prefix
            $(
                paste::paste! {
                    fn [<visit_ $const>](&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
                        self.visit_children(&node, lang)
                    }
                }
            )*
        }
    };
}

// ---------------- Token Definitions ----------------
define_tokens! {
    // ---------------- Text Tokens ----------------
    (Text_fn                ,  96 , "fn"                        , HirKind::Text),
    (Text_LPAREN            ,   4 , "("                         , HirKind::Text),
    (Text_RPAREN            ,   5 , ")"                         , HirKind::Text),
    (Text_LBRACE            ,   8 , "{"                         , HirKind::Text),
    (Text_RBRACE            ,   9 , "}"                         , HirKind::Text),
    (Text_let               , 101 , "let"                       , HirKind::Text),
    (Text_EQ                ,  70 , "="                         , HirKind::Text),
    (Text_SEMI              ,   2 , ";"                         , HirKind::Text),
    (Text_COLON             ,  11 , ":"                         , HirKind::Text),
    (Text_COMMA             ,  83 , ","                         , HirKind::Text),
    (Text_ARROW             ,  85 , "->"                        , HirKind::Text),

    // ---------------- Node Tokens ----------------
    (integer_literal       , 127 , "integer_literal"            , HirKind::Text),
    (identifier            ,   1 , "identifier"                 , HirKind::IdentUse),
    (parameter             , 213 , "parameter"                  , HirKind::Internal),
    (parameters            , 210 , "parameters"                 , HirKind::Internal),
    (let_declaration       , 203 , "let_declaration"            , HirKind::Internal),
    (block                 , 293 , "block"                      , HirKind::Scope),
    (source_file           , 157 , "source_file"                , HirKind::File),
    (function_item         , 188 , "function_item"              , HirKind::Scope),
    (mutable_specifier     , 122 , "mutable_specifier"          , HirKind::Text),
    (expression_statement  , 160 , "expression_statement"       , HirKind::Internal),
    (assignment_expression , 251 , "assignment_expression"      , HirKind::Internal),
    (binary_expression     , 250 , "binary_expression"          , HirKind::Internal),
    (operator              ,  14 , "operator"                   , HirKind::Internal),
    (call_expression       , 256 , "call_expression"            , HirKind::Internal),
    (arguments             , 257 , "arguments"                  , HirKind::Internal),
    (primitive_type        ,  32 , "primitive_type"             , HirKind::IdentUse),

    // ---------------- Field IDs ----------------
    (field_name            ,  19 , "name"                       , HirKind::Internal),
    (field_pattern         ,  24 , "pattern"                    , HirKind::Internal),
}

// ---------------- Declaration Finder Implementation ----------------

/// Visitor that finds and processes variable/function declarations
#[derive(Debug)]
pub struct DeclFinder<'tcx> {
    pub scope_stack: ScopeStack<'tcx>,
}

impl<'tcx> DeclFinder<'tcx> {
    /// Create a new DeclFinder with an empty scope stack
    pub fn new(scope_stack: ScopeStack<'tcx>) -> Self {
        Self { scope_stack }
    }

    /// Generate a unique mangled name for a symbol
    fn generate_mangled_name(&self, base_name: &str, node_id: HirId) -> String {
        format!("{}_{:x}", base_name, node_id.0)
    }

    /// Process a declaration by adding it to the current scope
    fn process_declaration(
        &mut self,
        node: &HirNode<'tcx>,
        field_id: u16,
        lang: &Language<'tcx>,
    ) -> SymId {
        let ident = node.expect_ident_from_child(&lang.ctx, field_id);
        let symbol = self.scope_stack.find_or_add(node.hir_id(), ident);

        let mangled = self.generate_mangled_name(&ident.name, node.hir_id());
        *symbol.mangled_name.borrow_mut() = mangled;

        symbol.id
    }
}

impl<'tcx> HirVisitor<'tcx> for DeclFinder<'tcx> {
    fn visit_function_item(&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
        let depth = self.scope_stack.depth();
        self.scope_stack.push_scope(node.hir_id());

        self.process_declaration(&node, Language::field_name, lang);

        self.visit_children(&node, lang);
        self.scope_stack.pop_until(depth);
    }

    fn visit_let_declaration(&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
        self.process_declaration(&node, Language::field_pattern, lang);

        self.visit_children(&node, lang);
    }

    fn visit_block(&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
        let depth = self.scope_stack.depth();
        self.scope_stack.push_scope(node.hir_id());

        self.visit_children(&node, lang);

        self.scope_stack.pop_until(depth);
    }

    fn visit_parameter(&mut self, node: HirNode<'tcx>, lang: &mut Language<'tcx>) {
        self.process_declaration(&node, Language::field_pattern, lang);

        self.visit_children(&node, lang);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_constants() {
        assert_eq!(Language::Text_fn, 96);
        assert_eq!(Language::identifier, 1);
        assert_eq!(Language::function_item, 188);
    }

    #[test]
    fn test_hir_kind_mapping() {
        assert_eq!(Language::hir_kind(Language::Text_fn), HirKind::Text);
        assert_eq!(Language::hir_kind(Language::block), HirKind::Scope);
        assert_eq!(Language::hir_kind(999), HirKind::Internal);
    }

    #[test]
    fn test_token_str() {
        assert_eq!(Language::token_str(Language::Text_fn), Some("fn"));
        assert_eq!(Language::token_str(Language::Text_LPAREN), Some("("));
        assert_eq!(Language::token_str(999), None);
    }

    #[test]
    fn test_valid_token() {
        assert!(Language::is_valid_token(Language::Text_fn));
        assert!(Language::is_valid_token(Language::identifier));
        assert!(!Language::is_valid_token(999));
    }
}
