use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_path = manifest_dir.join("./src/token_map.toml");

    println!("cargo:rerun-if-changed={}", config_path.display());

    // Use NODE_TYPES constant from tree-sitter-rust crate (no local file needed)
    let contents = llmcc_tree::generate_tokens_from_str(
        "Rust",
        tree_sitter_rust::LANGUAGE.into(),
        tree_sitter_rust::NODE_TYPES,
        &config_path,
    )
    .unwrap();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let out_file = out_dir.join("rust_tokens.rs");
    fs::write(&out_file, contents).unwrap();
}
