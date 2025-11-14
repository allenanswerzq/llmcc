//! Rust parsing utilities for extracting crate and module information.
//
//! This module provides functions to parse Cargo.toml and Rust source files
//! to extract crate names and module names according to Rust's module system.

use std::fs;
use std::path::Path;
use toml::Table;

/// Parse the crate name from Cargo.toml by walking up the directory tree.
///
/// This function searches for the nearest Cargo.toml file in the current directory
/// or any parent directories, then extracts the package name from the [package] section.
///
/// # Arguments
/// * `file_path` - The path to a source file to start the search from
///
/// # Returns
/// `Some(crate_name)` if a Cargo.toml with a package name is found,
/// `None` if no valid Cargo.toml is found or parsing fails
pub fn parse_crate_name(file_path: &str) -> Option<String> {
    let mut dir = Path::new(file_path).parent();
    while let Some(current_dir) = dir {
        let cargo_path = current_dir.join("Cargo.toml");
        if cargo_path.exists() {
            // Try to read and parse the Cargo.toml file using toml library
            if let Ok(content) = fs::read_to_string(&cargo_path) {
                if let Ok(table) = content.parse::<Table>() {
                    // Get the package name from [package] section
                    if let Some(package) = table.get("package") {
                        if let Some(name) = package.get("name") {
                            if let Some(name_str) = name.as_str() {
                                return Some(name_str.to_string());
                            }
                        }
                    }
                }
            }
            // If we found Cargo.toml but couldn't parse the name, return None
            return None;
        }
        dir = current_dir.parent();
    }
    None
}

/// Parse the module name from a Rust source file path.
///
/// This function extracts the module name based on Rust's module system conventions:
/// - `lib.rs` -> None (crate root, no module name)
/// - `mod.rs` -> parent directory name (inline module)
/// - Other files -> the file stem (filename without extension)
///
/// # Arguments
/// * `file_path` - The path to the Rust source file
///
/// # Returns
/// `Some(module_name)` for regular module files,
/// `None` for lib.rs or invalid paths
pub fn parse_module_name(file_path: &str) -> Option<String> {
    let file_stem = Path::new(file_path).file_stem().and_then(|n| n.to_str());

    let file_name = match file_stem {
        Some(name) => name,
        None => return None,
    };

    // Special case: if this is lib.rs, it's the crate root
    if file_name == "lib" {
        return None;
    }

    // If this is mod.rs, get the parent directory name
    if file_name == "mod" {
        return Path::new(file_path)
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
    }

    // For any other file, the module name is the file name itself
    Some(file_name.to_string())
}

/// Return the file name (without the `.rs` extension) for a Rust source path.
///
/// # Arguments
/// * `file_path` - The path to the Rust source file
///
/// # Returns
/// `Some("foo")` if the file path has a file stem, `None` otherwise.
pub fn parse_file_name(file_path: &str) -> Option<String> {
    Path::new(file_path)
        .file_stem()
        .and_then(|name| name.to_str())
        .map(|name| name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_module_name_lib() {
        assert_eq!(parse_module_name("src/lib.rs"), None);
    }

    #[test]
    fn test_parse_module_name_main() {
        assert_eq!(parse_module_name("src/main.rs"), Some("main".to_string()));
    }

    #[test]
    fn test_parse_module_name_mod_rs() {
        assert_eq!(
            parse_module_name("src/utils/mod.rs"),
            Some("utils".to_string())
        );
    }

    #[test]
    fn test_parse_module_name_regular() {
        assert_eq!(
            parse_module_name("src/utils/parser.rs"),
            Some("parser".to_string())
        );
    }

    #[test]
    fn test_parse_file_name() {
        assert_eq!(
            parse_file_name("src/utils/parser.rs"),
            Some("parser".to_string())
        );
    }
}
