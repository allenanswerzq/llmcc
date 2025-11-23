use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let config_path = manifest_dir.join("./src/token_map.toml");
    let node_types = manifest_dir.join("../../third_party/tree-sitter-rust/src/node-types.json");

    println!("cargo:rerun-if-changed={}", config_path.display());
    println!("cargo:rerun-if-changed={}", node_types.display());

    let contents = llmcc_tree::generate_tokens(
        "Rust",
        tree_sitter_rust::LANGUAGE.into(),
        &node_types,
        &config_path,
    )?;

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_file = out_dir.join("rust_tokens.rs");
    fs::write(&out_file, contents)?;

    Ok(())
}
