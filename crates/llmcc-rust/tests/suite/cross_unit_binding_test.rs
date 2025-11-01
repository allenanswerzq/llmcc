use llmcc_core::{symbol::SymbolKind, IrBuildConfig};
use llmcc_rust::{bind_symbols, build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

fn compile(sources: &[&str]) -> (&'static CompileCtxt<'static>, Vec<&'static CompileCtxt<'static>>) {
    let byte_sources: Vec<Vec<u8>> = sources
        .iter()
        .map(|s| s.as_bytes().to_vec())
        .collect();

    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&byte_sources)));

    build_llmcc_ir::<LangRust>(cc, IrBuildConfig::default()).unwrap();
    let globals = cc.create_globals();

    let mut units = Vec::new();
    for i in 0..sources.len() {
        let unit = cc.compile_unit(i);
        units.push(unit);
    }

    // Collect
    for unit in &units {
        let _ = collect_symbols(*unit, globals);
    }

    // Bind
    for unit in &units {
        let _ = bind_symbols(*unit, globals);
    }

    (cc, units)
}

#[test]
fn cross_unit_struct_function_call() {
    // File 0: Define struct and impl
    let impl_source = r#"
pub struct MyStruct;
impl MyStruct {
    pub fn new() -> Self {
        Self
    }
}
"#;

    // File 1: Call the function
    let caller_source = r#"
fn caller() {
    let _x = MyStruct::new();
}
"#;

    let (cc, units) = compile(&[impl_source, caller_source]);

    // Check that caller() function exists
    assert!(
        cc.global_symbols()
            .find_global_suffix(&[cc.interner().intern("caller")])
            .is_some(),
        "caller function should be in global symbols"
    );

    // Check that MyStruct symbol exists
    let my_struct_sym = cc
        .global_symbols()
        .find_global_suffix(&[cc.interner().intern("MyStruct")]);
    assert!(my_struct_sym.is_some(), "MyStruct should be in global symbols");

    // Check that MyStruct::new function exists
    let new_func = cc
        .global_symbols()
        .find_global_suffix_vec(&[
            cc.interner().intern("new"),
            cc.interner().intern("MyStruct"),
        ]);
    assert!(
        !new_func.is_empty(),
        "MyStruct::new function should exist in symbol table"
    );

    // Check that caller has MyStruct::new as a dependency
    let caller_sym = cc
        .global_symbols()
        .find_global_suffix(&[cc.interner().intern("caller")])
        .expect("caller should exist");

    let depends = caller_sym.depends.read().clone();
    println!("Caller dependencies: {:?}", depends);

    // Should have at least one dependency (to MyStruct::new or MyStruct)
    assert!(
        !depends.is_empty(),
        "caller should have dependencies (at least MyStruct or MyStruct::new)"
    );

    // Check if it's the new function or the struct
    let new_sym = new_func.first().expect("new should exist");
    let my_struct_sym = my_struct_sym.expect("MyStruct should exist");

    let has_new_dep = depends.iter().any(|&id| id == new_sym.id);
    let has_struct_dep = depends.iter().any(|&id| id == my_struct_sym.id);

    println!("has_new_dep: {}", has_new_dep);
    println!("has_struct_dep: {}", has_struct_dep);
    println!("new_sym.id: {:?}", new_sym.id);
    println!("my_struct_sym.id: {:?}", my_struct_sym.id);

    // With the fix, the new function depends on the struct
    // So caller -> new -> struct is the chain
    // But we only add direct deps in bind, so caller should have new as a dep
    assert!(
        has_new_dep || has_struct_dep,
        "caller should depend on either new function or MyStruct struct"
    );
}
