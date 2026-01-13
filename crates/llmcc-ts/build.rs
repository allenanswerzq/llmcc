use std::env;
use std::fs;
use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let config_path = manifest_dir.join("./src/token_map.toml");

    // tree-sitter-typescript provides node-types.json in its package
    // For now, we'll generate from the grammar at runtime
    // TODO: Extract node-types.json from tree-sitter-typescript package

    println!("cargo:rerun-if-changed={}", config_path.display());

    // Generate TypeScript token definitions
    let contents = llmcc_tree::generate_tokens(
        "TypeScript",
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        // Use the node-types.json bundled with this crate
        &manifest_dir.join("./src/node-types.json"),
        &config_path,
    )?;

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_file = out_dir.join("typescript_tokens.rs");
    fs::write(&out_file, contents)?;

    Ok(())
}
