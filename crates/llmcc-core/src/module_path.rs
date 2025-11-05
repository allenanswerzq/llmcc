use std::collections::HashMap;
use std::path::Path;

const MAX_PYTHON_MODULE_DEPTH: usize = 2;

/// Determine a logical module or crate grouping for the given source location string.
///
/// The input may include line suffixes (e.g. `path/to/file.rs:42`) and this helper
/// will strip them before delegating to the path-based variant.
pub fn module_group_from_location(location: &str) -> String {
    let path = strip_line_suffix(location);
    module_group_from_path(Path::new(path))
}

/// Determine a representative module group for a collection of source locations.
///
/// The result is the most frequently occurring module group across the provided locations.
/// Ties fall back to lexicographical order to keep the outcome stable.
pub fn module_group_from_locations<I, S>(locations: I) -> String
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut counts: HashMap<String, usize> = HashMap::new();

    for location in locations {
        let group = module_group_from_location(location.as_ref());
        *counts.entry(group).or_default() += 1;
    }

    if counts.is_empty() {
        return "unknown".to_string();
    }

    let mut best: Option<(String, usize)> = None;

    for (group, count) in counts {
        match &mut best {
            None => best = Some((group, count)),
            Some((best_group, best_count)) => {
                if count > *best_count || (count == *best_count && group < *best_group) {
                    *best_group = group;
                    *best_count = count;
                }
            }
        }
    }

    best.map(|(group, _)| group)
        .unwrap_or_else(|| "unknown".to_string())
}

/// Determine a logical module or crate grouping for the provided filesystem path.
///
/// Currently supports Rust and Python sources. For other extensions we fall back to the
/// Rust-style crate inference which inspects the directory layout.
pub(crate) fn module_group_from_path(path: &Path) -> String {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some(ext) if is_python_extension(ext) => python_module_from_path(path),
        _ => rust_crate_from_path(path),
    }
}

/// Best-effort crate identifier inference for Rust-style layouts.
fn rust_crate_from_path(path: &Path) -> String {
    let components: Vec<&str> = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect();

    if let Some(src_idx) = components.iter().position(|&component| component == "src") {
        if src_idx > 0 {
            return components[src_idx - 1].to_string();
        }
    }

    fallback_name(path)
}

/// Best-effort Python module path inference supporting package layouts.
fn python_module_from_path(path: &Path) -> String {
    if !path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(is_python_extension)
        .unwrap_or(false)
    {
        return rust_crate_from_path(path);
    }

    let file_stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.to_string());

    let mut packages: Vec<String> = Vec::new();
    let mut current = path.parent();

    while let Some(dir) = current {
        let dir_name = match dir.file_name().and_then(|name| name.to_str()) {
            Some(name) if !name.is_empty() => name.to_string(),
            _ => break,
        };

        let has_init = dir.join("__init__.py").exists() || dir.join("__init__.pyi").exists();

        if has_init {
            packages.push(dir_name);
        }

        current = dir.parent();
    }

    if packages.is_empty() {
        if let Some(stem) = file_stem
            .as_ref()
            .filter(|stem| stem.as_str() != "__init__" && !stem.is_empty())
        {
            return stem.clone();
        }

        if let Some(parent_name) = path
            .parent()
            .and_then(|dir| dir.file_name().and_then(|name| name.to_str()))
            .filter(|name| !name.is_empty())
            .map(|name| name.to_string())
        {
            return parent_name;
        }

        return "unknown".to_string();
    }

    packages.reverse();
    if packages.len() > MAX_PYTHON_MODULE_DEPTH {
        packages.truncate(MAX_PYTHON_MODULE_DEPTH);
    }

    packages.join(".")
}

fn is_python_extension(ext: &str) -> bool {
    let lower = ext.to_ascii_lowercase();
    matches!(lower.as_str(), "py" | "pyi")
}

fn fallback_name(path: &Path) -> String {
    path.file_stem()
        .or_else(|| path.file_name())
        .and_then(|stem| stem.to_str())
        .filter(|name| !name.is_empty())
        .map(|name| name.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn strip_line_suffix(location: &str) -> &str {
    let mut end = location.len();

    while let Some(idx) = location[..end].rfind(':') {
        let suffix = &location[idx + 1..end];

        if suffix.chars().all(|ch| ch.is_ascii_digit()) && !suffix.is_empty() {
            end = idx;
        } else {
            break;
        }
    }

    &location[..end]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn rust_module_group_uses_src_parent() {
        let path = Path::new("/tmp/project/src/lib.rs");
        let group = module_group_from_location(path.to_string_lossy().as_ref());
        assert_eq!(group, "project");
    }

    #[test]
    fn rust_module_group_falls_back_to_filename() {
        let path = Path::new("/tmp/some_crate/main.rs:27");
        let group = module_group_from_location(&path.to_string_lossy());
        assert_eq!(group, "main");
    }

    #[test]
    fn python_module_group_respects_packages() {
        let temp = tempdir().expect("create temp dir");
        let pkg_dir = temp.path().join("pkg");
        let sub_pkg_dir = pkg_dir.join("subpkg");
        fs::create_dir_all(&sub_pkg_dir).expect("create package dirs");
        fs::write(pkg_dir.join("__init__.py"), b"").expect("create pkg __init__");
        fs::write(sub_pkg_dir.join("__init__.py"), b"").expect("create subpkg __init__");
        let module_path = sub_pkg_dir.join("module.py");
        fs::write(&module_path, b"").expect("create module file");

        let group = module_group_from_location(&module_path.to_string_lossy());
        assert_eq!(group, "pkg.subpkg");
    }

    #[test]
    fn multiple_locations_choose_majority_group() {
        let base = tempdir().expect("create base temp dir");

        let rust_file = base.path().join("foo").join("src").join("lib.rs");
        fs::create_dir_all(rust_file.parent().unwrap()).expect("create rust directories");
        fs::write(&rust_file, b"").expect("create rust file");

        let python_pkg = base.path().join("bar");
        fs::create_dir_all(&python_pkg).expect("create python package directory");
        fs::write(python_pkg.join("__init__.py"), b"").expect("create python __init__");

        let rust_location = rust_file.to_string_lossy().into_owned();
        let python_location = python_pkg
            .join("__init__.py")
            .to_string_lossy()
            .into_owned();

        let group = module_group_from_locations([
            rust_location.as_str(),
            python_location.as_str(),
            rust_location.as_str(),
        ]);
        assert_eq!(group, "foo");
    }

    #[test]
    fn tied_groups_choose_lexicographically_smallest() {
        let group =
            module_group_from_locations(["/workspace/foo/src/lib.rs", "/workspace/bar/src/lib.rs"]);
        assert_eq!(group, "bar");
    }

    #[test]
    fn strip_line_suffix_preserves_windows_drive() {
        let location = r"C:\workspace\foo\src\lib.rs";
        assert_eq!(strip_line_suffix(location), location);

        let with_line = format!("{}:128", location);
        assert_eq!(strip_line_suffix(&with_line), location);

        let with_line_col = format!("{}:128:7", location);
        assert_eq!(strip_line_suffix(&with_line_col), location);
    }

    #[test]
    fn strip_line_suffix_handles_unix_paths() {
        let location = "/workspace/foo/src/lib.rs";
        assert_eq!(strip_line_suffix(location), location);

        let with_line = format!("{}:42", location);
        assert_eq!(strip_line_suffix(&with_line), location);
    }
}
