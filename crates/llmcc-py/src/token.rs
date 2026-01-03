use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{LanguageTrait, ParseNode, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{CompileCtxt, CompileUnit};
use llmcc_resolver::ResolverOption;

#[allow(clippy::single_component_path_imports)]
use tree_sitter_python;

// Include the auto-generated language definition from build script
// The generated file contains a define_lang! call that expands to LangPython
include!(concat!(env!("OUT_DIR"), "/python_tokens.rs"));

impl LanguageTraitImpl for LangPython {
    /// Block kind with parent context - handles special Python cases
    fn block_kind_with_parent_impl(kind_id: u16, field_id: u16, _parent_kind_id: u16) -> BlockKind {
        // Handle field-specific block kinds explicitly
        // (tree-sitter field IDs can collide with node IDs)
        if field_id == Self::field_return_type {
            return BlockKind::Return;
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

        for builtin in crate::PYTHON_BUILTINS {
            let name = cc.interner.intern(builtin);
            let symbol_val = Symbol::new(CompileCtxt::GLOBAL_SCOPE_OWNER, name);
            let sym_id = symbol_val.id().0;
            let symbol = cc.arena().alloc_with_id(sym_id, symbol_val);
            // Python builtins can be types, functions, or constants
            if matches!(*builtin, "int" | "str" | "float" | "bool" | "list" | "dict" |
                       "set" | "tuple" | "bytes" | "bytearray" | "object" | "type" |
                       "None" | "NoneType" | "complex" | "range" | "frozenset") {
                symbol.set_kind(SymKind::Primitive);
            } else {
                symbol.set_kind(SymKind::Function);
            }
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
                parser.set_language(&tree_sitter_python::LANGUAGE.into()).unwrap();
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
        &["py", "pyi"]
    }

    /// Check if the given parse node is a Python test decorator.
    /// Detects: @pytest.mark.*, @unittest.*, etc.
    fn is_test_attribute_impl(node: &dyn ParseNode, source: &[u8]) -> bool {
        // Check if this is a decorator node
        let kind_id = node.kind_id();
        if kind_id != LangPython::decorator {
            return false;
        }

        // Extract the text of the decorator
        let start = node.start_byte();
        let end = node.end_byte();
        if end <= start || end > source.len() {
            return false;
        }

        let decorator_text = match std::str::from_utf8(&source[start..end]) {
            Ok(text) => text,
            Err(_) => return false,
        };

        // Check for test-related decorators
        decorator_text.contains("@pytest")
            || decorator_text.contains("@unittest")
            || decorator_text.contains("@test")
            || decorator_text.contains("@fixture")
            || decorator_text.contains("@parametrize")
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
