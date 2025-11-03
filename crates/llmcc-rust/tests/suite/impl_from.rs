use llmcc_core::IrBuildConfig;
use llmcc_rust::{build_llmcc_ir, collect_symbols, CompileCtxt, LangRust};

#[test]
fn test_impl_from_with_qualified_type() {
    let source = r#"
pub struct SandboxWorkspaceWrite;

impl From<SandboxWorkspaceWrite> for codex_app_server_protocol::SandboxSettings {
    fn from(_: SandboxWorkspaceWrite) -> Self {
        todo!()
    }
}

mod codex_app_server_protocol {
    pub struct SandboxSettings;
}
"#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&sources)));
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(cc, IrBuildConfig).expect("build failed");

    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);

    // Find the struct SandboxWorkspaceWrite
    assert!(
        collection
            .structs
            .iter()
            .any(|s| s.name == "SandboxWorkspaceWrite"),
        "should find SandboxWorkspaceWrite struct"
    );

    // The impl descriptor should carry the fully-qualified target type.
    let impl_target_fqn = collection
        .impls
        .iter()
        .find_map(|desc| desc.impl_target_fqn.as_deref());
    assert_eq!(
        impl_target_fqn,
        Some("codex_app_server_protocol::SandboxSettings"),
        "impl should target the fully-qualified type"
    );
}

#[test]
fn test_impl_target_fqn_for_module_scoped_type() {
    let source = r#"
mod outer {
    pub struct Widget;
}

impl outer::Widget {
    pub fn new() -> Self {
        Self
    }
}
"#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&sources)));
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(cc, IrBuildConfig).expect("build failed");

    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);

    let impl_fqns: Vec<String> = collection
        .impls
        .iter()
        .filter_map(|desc| desc.impl_target_fqn.clone())
        .collect();
    assert_eq!(
        impl_fqns,
        vec!["outer::Widget".to_string()],
        "incorrect impl target FQN"
    );

    let has_new_method = collection.functions.iter().any(|desc| desc.name == "new");
    assert!(has_new_method, "expected inherent method `Widget::new`");
}

#[test]
fn test_impl_target_fqn_for_trait_impls_and_crate_paths() {
    let source = r#"
mod outer {
    pub trait Greeter {
        fn greet(&self) -> String;
    }

    pub trait Loud {
        fn shout(&self) -> String;
    }

    pub struct Widget;

    impl Greeter for Widget {
        fn greet(&self) -> String {
            "hello".to_string()
        }
    }
}

impl crate::outer::Loud for crate::outer::Widget {
    fn shout(&self) -> String {
        "HELLO".to_string()
    }
}

pub struct Foo;

impl crate::outer::Widget {
    pub fn new() -> Self {
        Self
    }
}

impl crate::Foo {
    pub fn build() -> Self {
        Foo
    }
}
"#;

    let sources = vec![source.as_bytes().to_vec()];
    let cc = Box::leak(Box::new(CompileCtxt::from_sources::<LangRust>(&sources)));
    let unit = cc.compile_unit(0);
    build_llmcc_ir::<LangRust>(cc, IrBuildConfig).expect("build failed");

    let globals = cc.create_globals();
    let collection = collect_symbols(unit, globals);

    let mut target_counts = std::collections::HashMap::new();
    for desc in &collection.impls {
        *target_counts
            .entry(desc.impl_target_fqn.clone())
            .or_insert(0usize) += 1;
    }
    assert_eq!(target_counts.get(&Some("Widget".into())), Some(&1));
    assert_eq!(
        target_counts.get(&Some("crate::outer::Widget".into())),
        Some(&2)
    );
    assert_eq!(target_counts.get(&Some("crate::Foo".into())), Some(&1));

    let trait_impl = collection
        .impls
        .iter()
        .find(|desc| desc.base_types.len() == 1)
        .expect("expected trait impl descriptor");
    let trait_segments: Vec<String> = trait_impl.base_types[0]
        .path_segments()
        .unwrap_or(&[])
        .to_vec();
    assert_eq!(
        trait_segments.last().map(|s| s.as_str()),
        Some("Greeter"),
        "trait impl should capture the trait name"
    );

    let has_new_method = collection.functions.iter().any(|desc| {
        desc.name == "new"
            && desc
                .fqn
                .as_deref()
                .unwrap_or_default()
                .ends_with("Widget::new")
    });
    assert!(has_new_method, "expected inherent method `Widget::new`");

    let loud_trait_paths: Vec<String> = collection
        .impls
        .iter()
        .flat_map(|desc| desc.base_types.iter())
        .map(|ty| {
            let segments = ty.path_segments().unwrap_or(&[]);
            segments
                .iter()
                .map(|segment| segment.as_str())
                .collect::<Vec<_>>()
                .join("::")
        })
        .collect();
    assert!(
        loud_trait_paths
            .iter()
            .any(|path| path == "crate::outer::Loud"),
        "expected trait impl for `Loud`, found {:?}",
        loud_trait_paths
    );

    let foo_builder = collection.functions.iter().any(|desc| {
        desc.name == "build"
            && desc
                .fqn
                .as_deref()
                .unwrap_or_default()
                .ends_with("Foo::build")
    });
    assert!(foo_builder, "expected inherent method `Foo::build`");
}
