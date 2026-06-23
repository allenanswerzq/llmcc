use llmcc_core::BlockKind;
use llmcc_core::LanguageDefinition;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{ParseNode, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{
    CompileCtxt, CompileUnit, Error, HirBuildAction, ResolveOptions, Result, SupportedLang,
};

// Generated from token_map.toml and tree-sitter-rust NODE_TYPES.
include!(concat!(env!("OUT_DIR"), "/rust_tokens.rs"));

impl LanguageDefinition for LangRust {
    /// Parent context disambiguates Rust syntax that tree-sitter represents generically.
    fn block_kind_for_child(
        _kind_id: u16,
        field_id: u16,
        parent_kind_id: u16,
    ) -> Option<BlockKind> {
        if parent_kind_id == LangRust::ordered_field_declaration_list
            && field_id == LangRust::field_type
        {
            return Some(BlockKind::Field);
        }

        // Function type annotations are types, not callable definitions with returns.
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

        // Parser instances are not shared across threads; reuse one per worker thread.
        thread_local! {
            static PARSER: RefCell<Option<tree_sitter::Parser>> = const { RefCell::new(None) };
        }

        PARSER.with(|parser| {
            let mut parser_slot = parser.borrow_mut();
            if parser_slot.is_none() {
                let mut parser = tree_sitter::Parser::new();
                parser
                    .set_language(&tree_sitter_rust::LANGUAGE.into())
                    .map_err(|error| {
                        Error::parse_failed(format!(
                            "failed to initialize tree-sitter-rust parser: {error}"
                        ))
                        .with_operation("parse_source")
                        .with_context("language", "rust")
                    })?;
                *parser_slot = Some(parser);
            }

            let Some(parser) = parser_slot.as_mut() else {
                return Err(Error::parse_failed("rust parser was not initialized")
                    .with_operation("parse_source")
                    .with_context("language", "rust"));
            };

            let bytes = text.as_ref();
            let tree = parser.parse(bytes, None).ok_or_else(|| {
                Error::parse_failed("tree-sitter returned no parse tree")
                    .with_operation("parse_source")
                    .with_context("language", "rust")
            })?;
            Ok(Box::new(TreeSitterParseTree::new(tree)) as Box<dyn ParseTree>)
        })
    }

    fn supported_lang() -> SupportedLang {
        SupportedLang::Rust
    }

    fn hir_build_action(node: &dyn ParseNode, source: &[u8]) -> HirBuildAction {
        let kind_id = node.kind_id();
        if kind_id != LangRust::attribute_item && kind_id != LangRust::inner_attribute_item {
            return HirBuildAction::Build;
        }

        let start = node.start_byte();
        let end = node.end_byte();
        if end <= start || end > source.len() {
            return HirBuildAction::Build;
        }

        let attr_text = match std::str::from_utf8(&source[start..end]) {
            Ok(text) => text,
            Err(_) => return HirBuildAction::Build,
        };

        if is_test_attribute(attr_text) {
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

fn is_test_attribute(attr_text: &str) -> bool {
    let attr = attr_text.trim();
    let inner = attr
        .strip_prefix("#[")
        .or_else(|| attr.strip_prefix("#!["))
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(attr)
        .trim();

    let path = inner.split_once('(').map_or(inner, |(path, _)| path).trim();
    if path.rsplit("::").next() == Some("test") {
        return true;
    }

    let compact: String = inner.chars().filter(|ch| !ch.is_whitespace()).collect();
    compact == "cfg(test)" || compact.starts_with("cfg_attr(test,")
}

#[cfg(test)]
mod tests {
    use super::is_test_attribute;

    #[test]
    fn detects_test_attributes() {
        assert!(is_test_attribute("#[test]"));
        assert!(is_test_attribute("#[cfg(test)]"));
        assert!(is_test_attribute("#[tokio::test]"));
        assert!(is_test_attribute(
            "#[tokio::test(flavor = \"multi_thread\")]"
        ));
        assert!(is_test_attribute("#[async_std::test(attributes)]"));
        assert!(!is_test_attribute("#[derive(Debug)]"));
        assert!(!is_test_attribute("#[cfg(not(test))]"));
    }
}
