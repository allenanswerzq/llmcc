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
    let has_struct = collection
        .structs
        .iter()
        .any(|s| s.name == "SandboxWorkspaceWrite");
    println!("Has struct: {}", has_struct);
    assert!(has_struct, "should find SandboxWorkspaceWrite struct");

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
