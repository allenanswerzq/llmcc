use std::path::Path;

const MAX_PYTHON_MODULE_DEPTH: usize = 2;

/// Determine a logical module or crate grouping for the given source location string.
///
/// The input may include line suffixes (e.g. `path/to/file.rs:42`) and this helper
/// will strip them before delegating to the path-based variant.
pub fn module_group_from_location(location: &str) -> String {
    let path = location.split(':').next().unwrap_or(location);
    module_group_from_path(Path::new(path))
}

/// Determine module groups for a collection of source locations.
pub fn module_groups_from_locations<I, S>(locations: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    locations
        .into_iter()
        .map(|location| module_group_from_location(location.as_ref()))
        .collect()
}

/// Determine a logical module or crate grouping for the provided filesystem path.
///
/// Currently supports Rust and Python sources. For other extensions we fall back to the
/// Rust-style crate inference which inspects the directory layout.
fn module_group_from_path(path: &Path) -> String {
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
    fn multiple_locations_collect_groups() {
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

        let groups =
            module_groups_from_locations([rust_location.as_str(), python_location.as_str()]);
        assert_eq!(groups, vec!["foo".to_string(), "bar".to_string()]);
    }
}
