use llmcc_core::BlockKind;
use llmcc_core::LanguageDefinition;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{ParseNode, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{
    CompileCtxt, CompileUnit, Error, HirBuildAction, ResolveOptions, Result, SupportedLang,
};

// Generated from token_map.toml and tree-sitter-cpp node-types.json.
include!(concat!(env!("OUT_DIR"), "/cpp_tokens.rs"));

impl LanguageDefinition for LangCpp {
    #[rustfmt::skip]
    fn initial_scopes<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        let stack = ScopeStack::new(cc.arena(), cc.interner());
        let globals = cc.create_globals();
        stack.push(globals);
        debug_assert!(stack.depth() == 1);

        for prim in crate::CPP_PRIMITIVES {
            let name = cc.interner().intern(prim);
            let symbol_val = Symbol::new(CompileCtxt::GLOBAL_SCOPE_OWNER, name);
            let sym_id = symbol_val.id().0;
            let symbol = cc.arena().alloc_with_id(sym_id, symbol_val);
            symbol.set_kind(SymKind::Primitive);
            symbol.set_is_global(true);
            globals.insert(symbol);
        }

        stack
    }

    fn parse_source(text: impl AsRef<[u8]>) -> Result<Box<dyn ParseTree>> {
        use std::cell::RefCell;

        // Parser instances are not shared across threads; reuse one per worker thread.
        thread_local! {
            static PARSER: RefCell<Option<tree_sitter::Parser>> = const { RefCell::new(None) };
        }

        PARSER.with(|parser| {
            let mut parser_slot = parser.borrow_mut();
            if parser_slot.is_none() {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_cpp::LANGUAGE.into())
                    .map_err(|error| {
                        Error::parse_failed(format!(
                            "failed to initialize tree-sitter-cpp parser: {error}"
                        ))
                        .with_operation("parse_source")
                        .with_context("language", "cpp")
                    })?;
                *parser_slot = Some(parser);
            }

            let Some(parser) = parser_slot.as_mut() else {
                return Err(Error::parse_failed("cpp parser was not initialized")
                    .with_operation("parse_source")
                    .with_context("language", "cpp"));
            };

            let bytes = text.as_ref();
            let tree = parser.parse(bytes, None).ok_or_else(|| {
                Error::parse_failed("tree-sitter returned no parse tree")
                    .with_operation("parse_source")
                    .with_context("language", "cpp")
            })?;
            Ok(Box::new(TreeSitterParseTree::new(tree)) as Box<dyn ParseTree>)
        })
    }

    fn supported_lang() -> SupportedLang {
        SupportedLang::Cpp
    }

    fn hir_build_action(node: &dyn ParseNode, source: &[u8]) -> HirBuildAction {
        let kind_id = node.kind_id();

        // Check for call expressions that might be test macros
        if kind_id == LangCpp::call_expression {
            let start = node.start_byte();
            let end = node.end_byte();
            if end <= start || end > source.len() {
                return HirBuildAction::Build;
            }

            let text = match std::str::from_utf8(&source[start..end]) {
                Ok(text) => text,
                Err(_) => return HirBuildAction::Build,
            };

            // Check for common test frameworks
            return if text.starts_with("TEST(")
                || text.starts_with("TEST_F(")
                || text.starts_with("TEST_P(")
                || text.starts_with("TYPED_TEST(")
                || text.starts_with("TYPED_TEST_P(")
                || text.starts_with("BOOST_AUTO_TEST_CASE(")
                || text.starts_with("BOOST_TEST_CASE(")
                || text.starts_with("CATCH_TEST_CASE(")
                || text.starts_with("TEST_CASE(")
                || text.starts_with("SCENARIO(")
            {
                HirBuildAction::SkipNextSibling
            } else {
                HirBuildAction::Build
            };
        }

        HirBuildAction::Build
    }

    fn collect_symbols<'tcx>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        scope_stack: ScopeStack<'tcx>,
        options: &ResolveOptions,
    ) -> &'tcx Scope<'tcx> {
        crate::collect::collect_symbols(unit, &node, scope_stack, options)
    }

    fn bind_symbols<'tcx>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        globals: &'tcx Scope<'tcx>,
        options: &ResolveOptions,
    ) {
        crate::bind::bind_symbols(unit, &node, globals, options);
    }
}
