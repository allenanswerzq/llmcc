use llmcc_rust::token::LangRust;

use llmcc_core::context::CompileCtxt;
use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
use llmcc_core::symbol::SymKind;
use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};
use tracing_subscriber::EnvFilter;

#[allow(dead_code)]
pub fn with_compiled_unit<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a CompileCtxt<'a>),
{
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with_test_writer()
        .try_init();

    let bytes = sources.iter().map(|src| src.as_bytes().to_vec()).collect::<Vec<_>>();

    let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
    build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();

    let resolver_option = ResolverOption::default().with_sequential(true).with_print_ir(true).with_bind_func_bodies(true);
    let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
    bind_symbols_with::<LangRust>(&cc, globals, &resolver_option);
    check(&cc);
}

#[allow(dead_code)]
pub fn with_collected_unit<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a CompileCtxt<'a>),
{
    let _ = tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
        .with_test_writer()
        .try_init();

    let bytes = sources.iter().map(|src| src.as_bytes().to_vec()).collect::<Vec<_>>();

    let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
    build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();

    let resolver_option = ResolverOption::default().with_sequential(true).with_print_ir(true);
    let _globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
    check(&cc);
}

#[allow(dead_code)]
pub fn find_symbol_id<'a>(cc: &'a CompileCtxt<'a>, name: &str, kind: SymKind) -> llmcc_core::symbol::SymId {
    let name_key = cc.interner.intern(name);
    cc.get_all_symbols()
        .into_iter()
        .find(|symbol| symbol.name == name_key && symbol.kind() == kind)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("symbol {name} with kind {:?} not found", kind))
}

#[allow(dead_code)]
pub fn assert_exists<'a>(cc: &'a CompileCtxt<'a>, name: &str, kind: SymKind) {
    let name_key = cc.interner.intern(name);
    let all_symbols = cc.get_all_symbols();
    for sym in &all_symbols {
        tracing::debug!("Symbol: {:?}", cc.interner.resolve_owned(sym.name).unwrap());
    }
    let symbol = all_symbols
        .iter()
        .find(|sym| sym.name == name_key && sym.kind() == kind)
        .unwrap_or_else(|| panic!("symbol {} with kind {:?} not found", name, kind));
    // prints all symbol for debugging
    assert!(symbol.id().0 > 0, "symbol should have a valid id");
}
