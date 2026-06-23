use llmcc_core::BlockKind;
use llmcc_core::LanguageDefinition;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{CompileCtxt, CompileUnit, Error, ResolveOptions, Result, SupportedLang};

include!(concat!(env!("OUT_DIR"), "/python_tokens.rs"));

impl LanguageDefinition for LangPython {
    #[rustfmt::skip]
    fn initial_scopes<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        let stack = ScopeStack::new(cc.arena(), cc.interner());
        let globals = cc.create_globals();
        stack.push(globals);
        debug_assert!(stack.depth() == 1);

        for primitive in crate::PYTHON_PRIMITIVES {
            let name = cc.interner().intern(primitive);
            let symbol_value = Symbol::new(CompileCtxt::GLOBAL_SCOPE_OWNER, name);
            let symbol_id = symbol_value.id().0;
            let symbol = cc.arena().alloc_with_id(symbol_id, symbol_value);
            symbol.set_kind(SymKind::Primitive);
            symbol.set_is_global(true);
            globals.insert(symbol);
        }

        stack
    }

    fn parse_source(text: impl AsRef<[u8]>) -> Result<Box<dyn ParseTree>> {
        use std::cell::RefCell;

        thread_local! {
            static PARSER: RefCell<Option<tree_sitter::Parser>> = const { RefCell::new(None) };
        }

        PARSER.with(|parser| {
            let mut parser_slot = parser.borrow_mut();
            if parser_slot.is_none() {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_python::LANGUAGE.into())
                    .map_err(|error| {
                        Error::parse_failed(format!(
                            "failed to initialize tree-sitter-python parser: {error}"
                        ))
                        .with_operation("parse_source")
                        .with_context("language", "python")
                    })?;
                *parser_slot = Some(parser);
            }

            let Some(parser) = parser_slot.as_mut() else {
                return Err(Error::parse_failed("python parser was not initialized")
                    .with_operation("parse_source")
                    .with_context("language", "python"));
            };

            let bytes = text.as_ref();
            let tree = parser.parse(bytes, None).ok_or_else(|| {
                Error::parse_failed("tree-sitter returned no parse tree")
                    .with_operation("parse_source")
                    .with_context("language", "python")
            })?;
            Ok(Box::new(TreeSitterParseTree::new(tree)) as Box<dyn ParseTree>)
        })
    }

    fn supported_lang() -> SupportedLang {
        SupportedLang::Python
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
