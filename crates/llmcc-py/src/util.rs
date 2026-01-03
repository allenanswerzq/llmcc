use std::fs;
use std::path::Path;
use toml::Table;

/// Parse the package name from pyproject.toml or setup.py by walking up the directory tree.
pub fn parse_package_name(file_path: &str) -> Option<String> {
    let mut dir = Path::new(file_path).parent();
    while let Some(current_dir) = dir {
        // Try pyproject.toml first
        let pyproject_path = current_dir.join("pyproject.toml");
        if pyproject_path.exists() {
            if let Ok(content) = fs::read_to_string(&pyproject_path)
                && let Ok(table) = content.parse::<Table>()
            {
                // Try [project] section first (PEP 621)
                if let Some(project) = table.get("project")
                    && let Some(name) = project.get("name")
                    && let Some(name_str) = name.as_str()
                {
                    return Some(name_str.to_string());
                }
                // Try [tool.poetry] section
                if let Some(tool) = table.get("tool")
                    && let Some(poetry) = tool.get("poetry")
                    && let Some(name) = poetry.get("name")
                    && let Some(name_str) = name.as_str()
                {
                    return Some(name_str.to_string());
                }
            }
        }

        // Try setup.py
        let setup_path = current_dir.join("setup.py");
        if setup_path.exists() {
            // For setup.py, we'd need to parse Python which is complex
            // Return the directory name as a fallback
            return current_dir
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }

        dir = current_dir.parent();
    }
    None
}

/// Parse the module name from a Python source file path.
///
/// This function determines the module name for a Python source file:
/// - `pkg/__init__.py` → Some("pkg") (package init)
/// - `pkg/subpkg/__init__.py` → Some("subpkg")
/// - `pkg/module.py` → Some("module")
/// - `script.py` (standalone) → Some("script")
pub fn parse_module_name(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let file_stem = path.file_stem().and_then(|n| n.to_str())?;

    // If this is __init__.py, the module name is the parent directory
    if file_stem == "__init__" {
        return path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(|s| s.to_string());
    }

    // Otherwise, the module name is the file stem
    Some(file_stem.to_string())
}

/// Return the file name (without the `.py` extension) for a Python source path.
/// Also strips numeric prefixes like "001_" that may be added for ordering.
#[allow(dead_code)]
pub fn parse_file_name(file_path: &str) -> Option<String> {
    let file_stem = Path::new(file_path)
        .file_stem()
        .and_then(|name| name.to_str())?;

    // Strip numeric prefix (e.g., "001_main" -> "main")
    Some(strip_numeric_prefix(file_stem))
}

/// Strip numeric prefix from a name (e.g., "001_main" -> "main").
/// Returns the original string if no prefix is found.
#[allow(dead_code)]
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

/// Check if a file is part of a Python package (has __init__.py in parent)
#[allow(dead_code)]
pub fn is_package_member(file_path: &str) -> bool {
    let path = Path::new(file_path);
    if let Some(parent) = path.parent() {
        let init_path = parent.join("__init__.py");
        return init_path.exists();
    }
    false
}

/// Get the full module path for a Python file (e.g., "pkg.subpkg.module")
#[allow(dead_code)]
pub fn get_full_module_path(file_path: &str) -> Option<String> {
    let path = Path::new(file_path);
    let mut components = Vec::new();

    // Walk up to find the package root (where there's no __init__.py in parent)
    let mut current = path;
    loop {
        if let Some(parent) = current.parent() {
            let init_path = parent.join("__init__.py");
            if init_path.exists() {
                if let Some(name) = current.file_stem().and_then(|n| n.to_str()) {
                    if name != "__init__" {
                        components.push(name.to_string());
                    }
                }
                current = parent;
            } else {
                // Add the final component
                if let Some(name) = current.file_stem().and_then(|n| n.to_str()) {
                    if name != "__init__" {
                        components.push(name.to_string());
                    } else if let Some(dir_name) = current.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
                        components.push(dir_name.to_string());
                    }
                }
                break;
            }
        } else {
            break;
        }
    }

    if components.is_empty() {
        return None;
    }

    components.reverse();
    Some(components.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_module_name() {
        assert_eq!(parse_module_name("/foo/bar/module.py"), Some("module".to_string()));
        assert_eq!(parse_module_name("/foo/bar/__init__.py"), Some("bar".to_string()));
        assert_eq!(parse_module_name("script.py"), Some("script".to_string()));
    }

    #[test]
    fn test_parse_file_name() {
        assert_eq!(parse_file_name("/foo/bar/module.py"), Some("module".to_string()));
        assert_eq!(parse_file_name("/foo/bar/001_module.py"), Some("module".to_string()));
    }

    #[test]
    fn test_strip_numeric_prefix() {
        assert_eq!(strip_numeric_prefix("001_main"), "main");
        assert_eq!(strip_numeric_prefix("main"), "main");
        assert_eq!(strip_numeric_prefix("test_001"), "test_001");
    }
}
