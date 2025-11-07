use llmcc_core::IrBuildConfig;
use llmcc_descriptor::{EnumVariantKind, TypeExpr, Visibility};
use llmcc_rust::{build_llmcc_ir, collect_symbols, CompileCtxt, EnumCollection, LangRust};

fn collect_enums(source: &str) -> EnumCollection {
    let sources = vec![source.as_bytes().to_vec()];
    let cc = CompileCtxt::from_sources::<LangRust>(&sources);
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(&cc, IrBuildConfig).unwrap();
    let collection = collect_symbols(unit).result;
    collection.enums
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
    assert_eq!(desc.visibility, Visibility::Public);
    assert_eq!(desc.generics.as_deref(), Some("<T>"));
    assert_eq!(desc.variants.len(), 4);

    let quit = &desc.variants[0];
    assert_eq!(quit.name, "Quit");
    assert_eq!(quit.kind, EnumVariantKind::Unit);
    assert!(quit.fields.is_empty());
    assert!(!quit.extras.contains_key("rust.discriminant"));

    let write = &desc.variants[1];
    assert_eq!(write.name, "Write");
    assert_eq!(write.kind, EnumVariantKind::Tuple);
    assert_eq!(write.fields.len(), 1);
    let write_ty = write.fields[0].type_annotation.as_ref().unwrap();
    assert_path(write_ty, &["String"]);

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
    assert_eq!(
        status.variants[0]
            .extras
            .get("rust.discriminant")
            .map(String::as_str),
        Some("200")
    );
    assert_eq!(
        status.variants[1]
            .extras
            .get("rust.discriminant")
            .map(String::as_str),
        Some("404")
    );
}

fn assert_path(expr: &TypeExpr, expected: &[&str]) {
    if let TypeExpr::Path { parts, .. } = expr {
        let expected_vec: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
        assert_eq!(parts, &expected_vec);
    } else {
        panic!("expected path type");
    }
}
