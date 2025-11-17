use llmcc_core::LanguageTraitImpl;
use llmcc_core::graph_builder::BlockKind;
use llmcc_core::ir::HirKind;
use llmcc_core::lang_def::{ParseTree, TreeSitterParseTree};
use llmcc_resolver::{BinderScopes, CollectorScopes};

#[allow(clippy::single_component_path_imports)]
use tree_sitter_rust;

// Include the auto-generated language definition from build script
// The generated file contains a define_lang! call that expands to LangRust
include!(concat!(env!("OUT_DIR"), "/rust_tokens.rs"));

impl LanguageTraitImpl for LangRust {
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

    fn collect_symbols_impl<'tcx, T>(
        unit: &llmcc_core::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        scopes: &mut T,
        namespace: &'tcx llmcc_core::scope::Scope<'tcx>,
    ) {
        // We use unsafe transmute here because at the call site from the collector,
        // T is known to be CollectorScopes<'tcx>. The trait design uses generic T
        // but the actual concrete type is always CollectorScopes for symbol collection.
        unsafe {
            let scopes = scopes as *mut T as *mut CollectorScopes<'tcx>;
            crate::collect::collect_symbols(unit, node, &mut *scopes, namespace);
        }
    }

    fn bind_symbols_impl<'tcx, T>(
        unit: &llmcc_core::CompileUnit<'tcx>,
        node: &llmcc_core::ir::HirNode<'tcx>,
        scopes: &mut T,
        namespace: &'tcx llmcc_core::scope::Scope<'tcx>,
    ) {
        // Similar to collect_symbols_impl, T is known to be BinderScopes<'tcx> at the call site
        unsafe {
            let scopes = scopes as *mut T as *mut BinderScopes<'tcx>;
            crate::bind::bind_symbols(*unit, node, &mut *scopes, namespace);
        }
    }
}
