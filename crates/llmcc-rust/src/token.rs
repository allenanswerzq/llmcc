use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{ParseTree, TreeSitterParseTree};
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
    #[rustfmt::skip]
    fn collect_init_impl<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        let stack = ScopeStack::new(cc.arena(), &cc.interner);
        let globals = cc.create_globals();
        stack.push(globals);

        for prim in crate::RUST_PRIMITIVES {
            let name = cc.interner.intern(prim);
            let symbol = cc.arena().alloc(Symbol::new(CompileCtxt::GLOBAL_SCOPE_OWNER, name));
            symbol.set_kind(SymKind::Primitive);
            symbol.set_fqn(name);
            symbol.set_is_global(true);
            globals.insert(symbol);
            // Symbol will be added to symbol_map when build_symbol_map_from_arena() is called
        }

        stack
    }

    fn parse_impl(text: impl AsRef<[u8]>) -> Option<Box<dyn ParseTree>> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .ok()?;

        let bytes = text.as_ref();
        let tree = parser.parse(bytes, None)?;

        Some(Box::new(TreeSitterParseTree { tree }))
    }

    fn supported_extensions_impl() -> &'static [&'static str] {
        &["rs"]
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
