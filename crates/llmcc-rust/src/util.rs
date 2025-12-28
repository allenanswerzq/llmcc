use std::fs;
use std::path::Path;
use toml::Table;

// Note: Test attribute filtering is now done at the HIR building stage
// in ir_builder.rs via LanguageTrait::is_test_attribute(), implemented
// in token.rs for the Rust language.

/// Parse the crate name from Cargo.toml by walking up the directory tree.
pub fn parse_crate_name(file_path: &str) -> Option<String> {
    let mut dir = Path::new(file_path).parent();
    while let Some(current_dir) = dir {
        let cargo_path = current_dir.join("Cargo.toml");
        if cargo_path.exists() {
            // Try to read and parse the Cargo.toml file using toml library
            if let Ok(content) = fs::read_to_string(&cargo_path)
                && let Ok(table) = content.parse::<Table>()
            {
                // Get the package name from [package] section
                if let Some(package) = table.get("package")
                    && let Some(name) = package.get("name")
                    && let Some(name_str) = name.as_str()
                {
                    return Some(name_str.to_string());
                }
            }
            // If we found Cargo.toml but couldn't parse the name, return None
            return None;
        }
        dir = current_dir.parent();
    }
    Some("_c".to_string())
}

/// Parse the module name from a Rust source file path.
///
/// This function determines the module name for a Rust source file:
/// - `src/lib.rs` or `src/main.rs` → None (crate root, no module)
/// - `src/data/mod.rs` → Some("data") (module directory)
/// - `src/data/entity.rs` → Some("data") (file in module directory inherits parent)
/// - `src/foo.rs` (sibling to lib.rs) → None (top-level module, handled separately)
pub fn parse_module_name(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let file_stem = path.file_stem().and_then(|n| n.to_str())?;

    // Special case: lib.rs and main.rs are crate roots, no module
    if file_stem == "lib" || file_stem == "main" {
        return None;
    }

    let parent = path.parent()?;
    let parent_name = parent.file_name().and_then(|n| n.to_str())?;

    // If parent is "src", this is a top-level file (e.g., src/foo.rs)
    // These are handled as top-level modules, not nested modules
    if parent_name == "src" {
        return None;
    }

    // If this is mod.rs, the module name is the parent directory
    // If this is another file (e.g., entity.rs in src/data/), the module is also the parent directory
    // Both cases: return the parent directory name as the module
    Some(parent_name.to_string())
}

/// Return the file name (without the `.rs` extension) for a Rust source path.
/// Also strips numeric prefixes like "001_" that may be added for ordering.
pub fn parse_file_name(file_path: &str) -> Option<String> {
    let file_stem = Path::new(file_path)
        .file_stem()
        .and_then(|name| name.to_str())?;

    // Strip numeric prefix (e.g., "001_lib" -> "lib", "002_models" -> "models")
    Some(strip_numeric_prefix(file_stem))
}

/// Strip numeric prefix from a name (e.g., "001_lib" -> "lib").
/// Returns the original string if no prefix is found.
fn strip_numeric_prefix(name: &str) -> String {
    // Check for pattern: digits followed by underscore
    if let Some(pos) = name.find('_') {
        let prefix = &name[..pos];
        if prefix.chars().all(|c| c.is_ascii_digit()) && !prefix.is_empty() {
            return name[pos + 1..].to_string();
        }
    }
    name.to_string()
}
