use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::RwLock;
use toml::Table;

// Note: Test attribute filtering is now done at the HIR building stage
// in ir_builder.rs via LanguageTrait::is_test_attribute(), implemented
// in token.rs for the Rust language.

/// Global cache for Cargo.toml -> crate name mappings.
/// Key: Cargo.toml directory path, Value: parsed crate name (or None if parsing failed)
static CARGO_TOML_CACHE: OnceLock<RwLock<HashMap<PathBuf, Option<String>>>> = OnceLock::new();

fn get_cache() -> &'static RwLock<HashMap<PathBuf, Option<String>>> {
    CARGO_TOML_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Parse crate name from a Cargo.toml file at the given directory.
/// Returns None if parsing fails or package.name is not found.
fn parse_cargo_toml(cargo_dir: &Path) -> Option<String> {
    let cargo_path = cargo_dir.join("Cargo.toml");
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
    None
}

/// Parse the crate name from Cargo.toml by walking up the directory tree.
/// Results are cached to avoid repeated file I/O and TOML parsing.
/// Also registers the crate root in the global package registry.
pub fn parse_crate_name(file_path: &str) -> Option<String> {
    let mut dir = Path::new(file_path).parent();

    while let Some(current_dir) = dir {
        let cargo_path = current_dir.join("Cargo.toml");
        if cargo_path.exists() {
            let dir_path = current_dir.to_path_buf();

            // Fast path: check read lock first
            {
                let cache = get_cache().read();
                if let Some(cached) = cache.get(&dir_path) {
                    return cached.clone();
                }
            }

            // Slow path: parse and cache with write lock
            let crate_name = parse_cargo_toml(current_dir);

            {
                let mut cache = get_cache().write();
                cache.insert(dir_path, crate_name.clone());
            }
            return crate_name;
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
