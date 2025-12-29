use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{LanguageTrait, ParseNode, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{CompileCtxt, CompileUnit};
use llmcc_resolver::ResolverOption;

#[allow(clippy::single_component_path_imports)]
use tree_sitter_rust;

// Include the auto-generated language definition from build script
// The generated file contains a define_lang! call that expands to LangRust
include!(concat!(env!("OUT_DIR"), "/rust_tokens.rs"));

impl LanguageTraitImpl for LangRust {
    /// Block kind with parent context - handles tuple struct fields
    fn block_kind_with_parent_impl(kind_id: u16, field_id: u16, parent_kind_id: u16) -> BlockKind {
        // Tuple struct fields: types inside ordered_field_declaration_list with field "type"
        if parent_kind_id == LangRust::ordered_field_declaration_list
            && field_id == LangRust::field_type
        {
            return BlockKind::Field;
        }
        // Don't create return blocks inside function_type (type annotations, not function definitions)
        // e.g., `type F = impl FnOnce() -> T;` should not create a return block for T
        if parent_kind_id == LangRust::function_type && field_id == LangRust::field_return_type {
            return BlockKind::Undefined;
        }
        // Default behavior: check field kind first, then node kind
        let field_kind = <Self as LanguageTrait>::block_kind(field_id);
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            <Self as LanguageTrait>::block_kind(kind_id)
        }
    }

    #[rustfmt::skip]
    fn collect_init_impl<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        let stack = ScopeStack::new(cc.arena(), &cc.interner);
        let globals = cc.create_globals();
        stack.push(globals);
        debug_assert!(stack.depth() == 1);

        for prim in crate::RUST_PRIMITIVES {
            let name = cc.interner.intern(prim);
            let symbol_val = Symbol::new(CompileCtxt::GLOBAL_SCOPE_OWNER, name);
            let sym_id = symbol_val.id().0;
            let symbol = cc.arena().alloc_with_id(sym_id, symbol_val);
            symbol.set_kind(SymKind::Primitive);
            symbol.set_is_global(true);
            globals.insert(symbol);
        }

        stack
    }

    fn parse_impl(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
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
            let tree = parser.parse(bytes, None)?;
            Some(Box::new(TreeSitterParseTree { tree }) as Box<dyn ParseTree>)
        })
    }

    fn supported_extensions_impl() -> &'static [&'static str] {
        &["rs"]
    }

    /// Check if the given parse node is a Rust test attribute.
    /// Detects: #[test], #[cfg(test)], #[tokio::test], #[async_std::test], etc.
    fn is_test_attribute_impl(node: &dyn ParseNode, source: &[u8]) -> bool {
        // First check if this is an attribute_item or inner_attribute_item
        let kind_id = node.kind_id();
        if kind_id != LangRust::attribute_item && kind_id != LangRust::inner_attribute_item {
            return false;
        }

        // Extract the text of the attribute
        let start = node.start_byte();
        let end = node.end_byte();
        if end <= start || end > source.len() {
            return false;
        }

        let attr_text = match std::str::from_utf8(&source[start..end]) {
            Ok(text) => text,
            Err(_) => return false,
        };

        // Check for test-related attributes
        attr_text.contains("#[test]")
            || attr_text.contains("#[cfg(test)]")
            || attr_text.contains("::test]") // catches #[tokio::test], #[async_std::test], etc.
    }

    fn collect_symbols_impl<'tcx, C>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        scope_stack: ScopeStack<'tcx>,
        config: &C,
    ) -> &'tcx Scope<'tcx> {
        unsafe {
            let config_ref = config as *const C as *const ResolverOption;
            crate::collect::collect_symbols(unit, &node, scope_stack, &*config_ref)
        }
    }

    fn bind_symbols_impl<'tcx, C>(
        unit: CompileUnit<'tcx>,
        node: HirNode<'tcx>,
        globals: &'tcx Scope<'tcx>,
        config: &C,
    ) {
        unsafe {
            let config = config as *const C as *const ResolverOption;
            crate::bind::bind_symbols(unit, &node, globals, &*config);
        }
    }
}
