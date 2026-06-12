use llmcc_core::LanguageDefinition;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{ParseNode, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{CompileCtxt, CompileUnit, Error, HirBuildAction, ResolveOptions, Result};

#[allow(clippy::single_component_path_imports)]
use tree_sitter_rust;

// Include the auto-generated language definition from build script
// The generated file contains a define_lang! call that expands to LangRust
include!(concat!(env!("OUT_DIR"), "/rust_tokens.rs"));

impl LanguageDefinition for LangRust {
    /// Block kind with parent context - handles tuple struct fields
    fn block_kind_for_child(
        _kind_id: u16,
        field_id: u16,
        parent_kind_id: u16,
    ) -> Option<BlockKind> {
        // Tuple struct fields: types inside ordered_field_declaration_list with field "type"
        if parent_kind_id == LangRust::ordered_field_declaration_list
            && field_id == LangRust::field_type
        {
            return Some(BlockKind::Field);
        }
        // Don't create return blocks inside function_type (type annotations, not function definitions)
        // e.g., `type F = impl FnOnce() -> T;` should not create a return block for T
        if parent_kind_id == LangRust::function_type && field_id == LangRust::field_return_type {
            return Some(BlockKind::Undefined);
        }
        None
    }

    #[rustfmt::skip]
    fn initial_scopes<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        let stack = ScopeStack::new(cc.arena(), cc.interner());
        let globals = cc.create_globals();
        stack.push(globals);
        debug_assert!(stack.depth() == 1);

        for prim in crate::RUST_PRIMITIVES {
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

        // Thread-local parser reuse to avoid contention from Parser::new()
        thread_local! {
            static PARSER: RefCell<tree_sitter::Parser> = {
                let mut parser = tree_sitter::Parser::new();
                parser.set_language(&tree_sitter_rust::LANGUAGE.into()).unwrap();
                RefCell::new(parser)
            };
        }

        PARSER.with(|parser| {
            let mut parser = parser.borrow_mut();
            let bytes = text.as_ref();
            let tree = parser.parse(bytes, None).ok_or_else(|| {
                Error::parse_failed("tree-sitter returned no parse tree")
                    .with_operation("parse_source")
                    .with_context("language", "rust")
            })?;
            Ok(Box::new(TreeSitterParseTree::new(tree)) as Box<dyn ParseTree>)
        })
    }

    fn file_extensions() -> &'static [&'static str] {
        &["rs"]
    }

    fn manifest_file() -> &'static str {
        "Cargo.toml"
    }

    fn container_dirs() -> &'static [&'static str] {
        &["src"]
    }

    fn hir_build_action(node: &dyn ParseNode, source: &[u8]) -> HirBuildAction {
        // First check if this is an attribute_item or inner_attribute_item
        let kind_id = node.kind_id();
        if kind_id != LangRust::attribute_item && kind_id != LangRust::inner_attribute_item {
            return HirBuildAction::Build;
        }

        // Extract the text of the attribute
        let start = node.start_byte();
        let end = node.end_byte();
        if end <= start || end > source.len() {
            return HirBuildAction::Build;
        }

        let attr_text = match std::str::from_utf8(&source[start..end]) {
            Ok(text) => text,
            Err(_) => return HirBuildAction::Build,
        };

        // Check for test-related attributes
        if attr_text.contains("#[test]")
            || attr_text.contains("#[cfg(test)]")
            || attr_text.contains("::test]")
        // catches #[tokio::test], #[async_std::test], etc.
        {
            HirBuildAction::SkipNextSibling
        } else {
            HirBuildAction::Build
        }
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
