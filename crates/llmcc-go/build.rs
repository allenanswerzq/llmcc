use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let config_path = manifest_dir.join("./src/token_map.toml");

    println!("cargo:rerun-if-changed={}", config_path.display());

    let contents = llmcc_tree::generate_tokens_from_str(
        "Go",
        tree_sitter_go::LANGUAGE.into(),
        tree_sitter_go::NODE_TYPES,
        &config_path,
    )
    .unwrap();

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("go_tokens.rs"), contents).unwrap();
}
