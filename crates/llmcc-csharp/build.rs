use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let config_path = manifest_dir.join("./src/token_map.toml");

    println!("cargo:rerun-if-changed={}", config_path.display());

    let contents = llmcc_tree::generate_tokens(
        "CSharp",
        tree_sitter_c_sharp::LANGUAGE.into(),
        llmcc_tree::NodeTypesSource::Embedded(tree_sitter_c_sharp::NODE_TYPES),
        &config_path,
    )?;

    let out_dir = PathBuf::from(env::var("OUT_DIR")?);
    let out_file = out_dir.join("csharp_tokens.rs");
    fs::write(&out_file, contents)?;
    Ok(())
}
