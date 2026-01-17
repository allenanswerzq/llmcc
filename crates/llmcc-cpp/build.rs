use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_path = manifest_dir.join("./src/token_map.toml");
    let node_types = manifest_dir.join("../../third_party/tree-sitter-cpp/src/node-types.json");

    println!("cargo:rerun-if-changed={}", config_path.display());
    println!("cargo:rerun-if-changed={}", node_types.display());

    let contents = llmcc_tree::generate_tokens(
        "Cpp",
        tree_sitter_cpp::LANGUAGE.into(),
        &node_types,
        &config_path,
    )
    .unwrap();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_file = out_dir.join("cpp_tokens.rs");
    fs::write(&out_file, contents).unwrap();
}
