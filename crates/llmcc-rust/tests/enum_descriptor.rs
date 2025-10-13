use llmcc_rust::{
    build_llmcc_ir, collect_symbols, EnumDescriptor, EnumVariantKind, FnVisibility, CompileCtxt,
    LangRust,
};

fn collect_enums(source: &str) -> Vec<EnumDescriptor> {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    let tree = unit.tree();
    build_llmcc_ir::<LangRust>(&tree, unit).expect("build HIR");
    let root = unit.file_start_hir_id().expect("registered root id");
    let globals = cc.alloc_scope(root);
    collect_symbols(root, unit, globals).enums
}

#[test]
fn captures_enum_metadata_and_variants() {
    let source = r#"
        pub enum Message<T> {
            Quit,
            Write(String),
            Move { x: i32, y: i32 },
            ChangeColor(T, T, T),
        }
    "#;
    let enums = collect_enums(source);
    assert_eq!(enums.len(), 1);
    let desc = &enums[0];
    assert_eq!(desc.name, "Message");
    assert_eq!(desc.visibility, FnVisibility::Public);
    assert_eq!(desc.generics.as_deref(), Some("<T>"));
    assert_eq!(desc.variants.len(), 4);

    let quit = &desc.variants[0];
    assert_eq!(quit.name, "Quit");
    assert_eq!(quit.kind, EnumVariantKind::Unit);
    assert!(quit.fields.is_empty());
    assert!(quit.discriminant.is_none());

    let write = &desc.variants[1];
    assert_eq!(write.name, "Write");
    assert_eq!(write.kind, EnumVariantKind::Tuple);
    assert_eq!(write.fields.len(), 1);
    let write_ty = write.fields[0]
        .ty
        .as_ref()
        .and_then(|ty| ty.path_segments())
        .map(|segs| segs.join("::"));
    assert_eq!(write_ty.as_deref(), Some("String"));

    let mv = &desc.variants[2];
    assert_eq!(mv.name, "Move");
    assert_eq!(mv.kind, EnumVariantKind::Struct);
    assert_eq!(mv.fields.len(), 2);
    assert_eq!(mv.fields[0].name.as_deref(), Some("x"));
    assert_eq!(mv.fields[1].name.as_deref(), Some("y"));

    let change = &desc.variants[3];
    assert_eq!(change.name, "ChangeColor");
    assert_eq!(change.kind, EnumVariantKind::Tuple);
    assert_eq!(change.fields.len(), 3);
}

#[test]
fn captures_enum_variant_discriminant() {
    let source = r#"
        enum Status {
            Ok = 200,
            NotFound = 404,
        }
    "#;

    let enums = collect_enums(source);
    assert_eq!(enums.len(), 1);
    let status = &enums[0];
    assert_eq!(status.variants.len(), 2);
    assert_eq!(status.variants[0].discriminant.as_deref(), Some("200"));
    assert_eq!(status.variants[1].discriminant.as_deref(), Some("404"));
}
