use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{LanguageTrait, ParseTree, TreeSitterParseTree};
use llmcc_core::scope::{Scope, ScopeStack};
use llmcc_core::symbol::{SymKind, Symbol};
use llmcc_core::{CompileCtxt, CompileUnit};
use llmcc_resolver::ResolverOption;

#[allow(clippy::single_component_path_imports)]
use tree_sitter_go;

include!(concat!(env!("OUT_DIR"), "/go_tokens.rs"));

impl LanguageTraitImpl for LangGo {
    fn block_kind_with_parent_impl(kind_id: u16, field_id: u16, _parent_kind_id: u16) -> BlockKind {
        let field_kind = <Self as LanguageTrait>::block_kind(field_id);
        if field_kind != BlockKind::Undefined {
            field_kind
        } else {
            <Self as LanguageTrait>::block_kind(kind_id)
        }
    }

    fn collect_init_impl<'tcx>(cc: &'tcx CompileCtxt<'tcx>) -> ScopeStack<'tcx> {
        let stack = ScopeStack::new(cc.arena(), &cc.interner);
        let globals = cc.create_globals();
        stack.push(globals);

        for prim in crate::GO_PRIMITIVES {
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

        thread_local! {
            static PARSER: RefCell<tree_sitter::Parser> = {
                let mut parser = tree_sitter::Parser::new();
                parser.set_language(&tree_sitter_go::LANGUAGE.into()).unwrap();
                RefCell::new(parser)
            };
        }

        PARSER.with(|parser| {
            let mut parser = parser.borrow_mut();
            let tree = parser.parse(text.as_ref(), None)?;
            Some(Box::new(TreeSitterParseTree { tree }) as Box<dyn ParseTree>)
        })
    }

    fn supported_extensions_impl() -> &'static [&'static str] {
        &["go"]
    }

    fn manifest_name_impl() -> &'static str {
        "go.mod"
    }

    fn container_dirs_impl() -> &'static [&'static str] {
        &["cmd", "internal", "pkg", "src"]
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
