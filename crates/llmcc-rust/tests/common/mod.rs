use llmcc_rust::token::LangRust;

use llmcc_core::context::CompileCtxt;
use llmcc_core::ir_builder::{IrBuildOption, build_llmcc_ir};
use llmcc_core::symbol::{DepKind, SymKind};
use llmcc_resolver::{ResolverOption, bind_symbols_with, collect_symbols_with};

pub fn with_compiled_unit<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a CompileCtxt<'a>),
{
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_test_writer()
        .try_init();

    let bytes = sources
        .iter()
        .map(|src| src.as_bytes().to_vec())
        .collect::<Vec<_>>();

    let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
    build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();

    let resolver_option = ResolverOption::default()
        .with_sequential(true)
        .with_print_ir(true);
    let globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
    bind_symbols_with::<LangRust>(&cc, &globals, &resolver_option);
    check(&cc);
}

pub fn with_collected_unit<F>(sources: &[&str], check: F)
where
    F: for<'a> FnOnce(&'a CompileCtxt<'a>),
{
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_test_writer()
        .try_init();

    let bytes = sources
        .iter()
        .map(|src| src.as_bytes().to_vec())
        .collect::<Vec<_>>();

    let cc = CompileCtxt::from_sources::<LangRust>(&bytes);
    build_llmcc_ir::<LangRust>(&cc, IrBuildOption::default()).unwrap();

    let resolver_option = ResolverOption::default()
        .with_sequential(true)
        .with_print_ir(true);
    let _globals = collect_symbols_with::<LangRust>(&cc, &resolver_option);
    check(&cc);
}

pub fn find_symbol_id<'a>(
    cc: &'a CompileCtxt<'a>,
    name: &str,
    kind: SymKind,
) -> llmcc_core::symbol::SymId {
    let name_key = cc.interner.intern(name);
    cc.get_all_symbols()
        .into_iter()
        .find(|symbol| symbol.name == name_key && symbol.kind() == kind)
        .map(|symbol| symbol.id())
        .unwrap_or_else(|| panic!("symbol {name} with kind {:?} not found", kind))
}

pub fn assert_depends<'a>(
    cc: &'a CompileCtxt<'a>,
    from_name: &str,
    from_kind: SymKind,
    to_name: &str,
    to_kind: SymKind,
    dep_kind: Option<DepKind>,
) {
    let from_sym = cc
        .get_all_symbols()
        .iter()
        .find(|sym| {
            let name_key = cc.interner.intern(from_name);
            sym.name == name_key && sym.kind() == from_kind
        })
        .copied()
        .expect(&format!(
            "symbol {} with kind {:?} not found",
            from_name, from_kind
        ));

    let to_sym = cc
        .get_all_symbols()
        .iter()
        .find(|sym| {
            let name_key = cc.interner.intern(to_name);
            sym.name == name_key && sym.kind() == to_kind
        })
        .copied()
        .expect(&format!(
            "symbol {} with kind {:?} not found",
            to_name, to_kind
        ));

    let from_id = from_sym.id();
    let to_id = to_sym.id();

    let has_dep = if let Some(kind) = dep_kind {
        from_sym
            .depends
            .read()
            .iter()
            .any(|(dep_id, dep_k)| *dep_id == to_id && *dep_k == kind)
    } else {
        from_sym
            .depends
            .read()
            .iter()
            .any(|(dep_id, _)| *dep_id == to_id)
    };

    assert!(
        has_dep,
        "'{}' ({:?}) should depend on '{}' ({:?}){}",
        from_name,
        from_kind,
        to_name,
        to_kind,
        dep_kind
            .map(|k| format!(" with kind {:?}", k))
            .unwrap_or_default()
    );
}

pub fn assert_exists<'a>(cc: &'a CompileCtxt<'a>, name: &str, kind: SymKind) {
    let name_key = cc.interner.intern(name);
    let all_symbols = cc.get_all_symbols();
    let symbol = all_symbols
        .iter()
        .find(|sym| sym.name == name_key && sym.kind() == kind)
        .expect(&format!("symbol {} with kind {:?} not found", name, kind));
    assert!(symbol.id().0 > 0, "symbol should have a valid id");
}
