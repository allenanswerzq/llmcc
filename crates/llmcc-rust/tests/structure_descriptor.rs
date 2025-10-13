use llmcc_rust::{
    build_llmcc_ir, collect_symbols, GlobalCtxt, HirId, LangRust, StructDescriptor, StructKind,
    SymbolRegistry,
};

fn collect_structs(source: &str) -> Vec<StructDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let gcx = GlobalCtxt::from_sources::<LangRust>(&sources);
    let ctx = gcx.file_context(0);
    let tree = ctx.tree();
    build_llmcc_ir::<LangRust>(&tree, ctx).expect("build HIR");
    let mut registry = SymbolRegistry::default();
    collect_symbols(HirId(0), ctx, &mut registry).structs
}

#[test]
fn captures_named_struct() {
    let source = r#"
        pub struct Point<T> {
            pub x: T,
            y: T,
        }
    "#;
    let structs = collect_structs(source);
    assert_eq!(structs.len(), 1);
    let desc = &structs[0];
    assert_eq!(desc.name, "Point");
    assert_eq!(desc.visibility, llmcc_rust::FnVisibility::Public);
    assert_eq!(desc.generics.as_deref(), Some("<T>"));
    assert_eq!(desc.kind, StructKind::Named);
    assert_eq!(desc.fields.len(), 2);
    assert_eq!(desc.fields[0].name.as_deref(), Some("x"));
    assert_eq!(desc.fields[1].name.as_deref(), Some("y"));
}

#[test]
fn captures_tuple_struct() {
    let source = "struct Wrapper(usize, String);";
    let structs = collect_structs(source);
    let desc = &structs[0];
    assert_eq!(desc.kind, StructKind::Tuple);
    assert_eq!(desc.fields.len(), 2);
    assert!(desc.fields[0].name.is_none());
}

#[test]
fn captures_unit_struct() {
    let structs = collect_structs("struct Marker;\n");
    let desc = &structs[0];
    assert_eq!(desc.kind, StructKind::Unit);
    assert!(desc.fields.is_empty());
}
