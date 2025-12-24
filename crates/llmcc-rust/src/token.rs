use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::{HirKind, HirNode};
use llmcc_core::lang_def::{LanguageTrait, ParseTree, TreeSitterParseTree};
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
        if parent_kind_id == LangRust::function_type
            && field_id == LangRust::field_return_type
        {
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
            let symbol = cc.arena().alloc(Symbol::new(CompileCtxt::GLOBAL_SCOPE_OWNER, name));
            symbol.set_kind(SymKind::Primitive);
            symbol.set_is_global(true);
            globals.insert(symbol);
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
