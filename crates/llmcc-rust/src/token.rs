use llmcc_core::block::BlockKind;
use llmcc_core::define_tokens;
use llmcc_core::ir::HirKind;
use llmcc_core::paste;
use llmcc_core::{Parser, Tree};

define_tokens! {
    Rust,
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
    (type_identifier       , 354 , "type_identifier"            , HirKind::Identifier),
    (scoped_identifier     , 243 , "scoped_identifier"          , HirKind::Identifier),
    (identifier            ,   1 , "identifier"                 , HirKind::Identifier),
    (parameter             , 213 , "parameter"                  , HirKind::Internal),
    (parameters            , 210 , "parameters"                 , HirKind::Internal),
    (let_declaration       , 203 , "let_declaration"            , HirKind::Internal),
    (block                 , 293 , "block"                      , HirKind::Scope,               BlockKind::Scope),
    (source_file           , 157 , "source_file"                , HirKind::File,                BlockKind::Root),
    (mod_item              , 173 , "mod_item"                   , HirKind::Scope,               BlockKind::Scope),
    (struct_item           , 176 , "struct_item"                , HirKind::Scope,               BlockKind::Scope),
    (enum_item             , 178 , "enum_item"                  , HirKind::Scope,               BlockKind::Scope),
    (enum_variant_list     , 179 , "enum_variant_list"                  , HirKind::Internal),
    (enum_variant          , 180 , "enum_variant"                  , HirKind::Identifier),
    (impl_item             , 193 , "impl_item"                  , HirKind::Scope,               BlockKind::Scope),
    (trait_item            , 194 , "trait_item"                 , HirKind::Scope,               BlockKind::Scope),
    (const_item            , 185 , "const_item"                 , HirKind::Internal),
    (static_item           , 186 , "static_item"                , HirKind::Internal),
    (function_item         , 188 , "function_item"              , HirKind::Scope,               BlockKind::Func),
    // (function_signature_item , 189 , "function_signature_item"              , HirKind::Scope,               BlockKind::Func),
    (mutable_specifier     , 122 , "mutable_specifier"          , HirKind::Text),
    (expression_statement  , 160 , "expression_statement"       , HirKind::Internal),
    (assignment_expression , 251 , "assignment_expression"      , HirKind::Internal),
    (binary_expression     , 250 , "binary_expression"          , HirKind::Internal),
    (operator              ,  14 , "operator"                   , HirKind::Internal),
    (call_expression       , 256 , "call_expression"            , HirKind::Internal,            BlockKind::Call),
    (arguments             , 257 , "arguments"                  , HirKind::Internal),
    (primitive_type        ,  32 , "primitive_type"             , HirKind::Identifier),

    // ---------------- Field IDs ----------------
    (field_name            ,  19 , "name"                       , HirKind::Internal),
    (field_type            ,  28 , "type"                       , HirKind::Internal),
    (field_pattern         ,  24 , "pattern"                    , HirKind::Internal),
    (field_return_type     ,  25 , "return_type"                , HirKind::Internal),
    (field_parameters      ,  22 , "parameters"                 , HirKind::Internal),
}
