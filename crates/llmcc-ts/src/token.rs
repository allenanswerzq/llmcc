use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{LanguageTrait, ParseNode, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{CompileCtxt, CompileUnit};
use llmcc_resolver::ResolverOption;

#[allow(clippy::single_component_path_imports)]
use tree_sitter_typescript;

// Include the auto-generated language definition from build script
// The generated file contains a define_lang! call that expands to LangTypeScript
include!(concat!(env!("OUT_DIR"), "/typescript_tokens.rs"));

impl LanguageTraitImpl for LangTypeScript {
    /// Block kind with parent context - handles special TypeScript cases
    fn block_kind_with_parent_impl(kind_id: u16, field_id: u16, _parent_kind_id: u16) -> BlockKind {
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

        for prim in crate::TYPESCRIPT_PRIMITIVES {
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
                parser.set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()).unwrap();
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
        &["ts", "mts", "cts"]
    }

    /// Check if the given parse node is a TypeScript test attribute.
    /// Detects: @test, describe(), it(), test(), etc.
    fn is_test_attribute_impl(node: &dyn ParseNode, source: &[u8]) -> bool {
        let kind_id = node.kind_id();

        // Check for decorator nodes (e.g., @Test)
        if kind_id == LangTypeScript::decorator {
            let start = node.start_byte();
            let end = node.end_byte();
            if end <= start || end > source.len() {
                return false;
            }

            let attr_text = match std::str::from_utf8(&source[start..end]) {
                Ok(text) => text,
                Err(_) => return false,
            };

            // Check for test-related decorators
            return attr_text.contains("@Test")
                || attr_text.contains("@test")
                || attr_text.contains("@it")
                || attr_text.contains("@describe");
        }

        false
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
