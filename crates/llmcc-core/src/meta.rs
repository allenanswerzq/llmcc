//! Source-file metadata for project, package, module, and file architecture levels.
//!
//! Packages come from language manifests. Modules are inferred from package-relative
//! directory structure after ignoring container directories such as `src`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use strum::IntoEnumIterator;
use strum_macros::{EnumIter, FromRepr};

const MIN_MODULE_SIBLING_RATIO: f64 = 0.05;
const MIN_MODULE_SIBLING_FILES: usize = 1;
const MAX_MODULE_DOMINANCE_RATIO: f64 = 0.80;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumIter, FromRepr)]
#[repr(u8)]
pub enum ArchitectureLevel {
    Project = 0,
    Package = 1,
    Module = 2,
    File = 3,
}

impl ArchitectureLevel {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(level: u8) -> Option<Self> {
        Self::from_repr(level)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitMeta {
    pub project_name: Option<String>,
    pub project_root: Option<PathBuf>,
    pub package_name: Option<String>,
    pub package_root: Option<PathBuf>,
    pub module_name: Option<String>,
    pub module_root: Option<PathBuf>,
    pub file_name: Option<String>,
    pub file_path: Option<PathBuf>,
    pub crate_index: usize,
}

impl UnitMeta {
    fn for_file(project_name: &str, project_root: &Path, file: &Path) -> Self {
        Self {
            project_name: Some(project_name.to_owned()),
            project_root: Some(project_root.to_path_buf()),
            file_path: Some(file.to_path_buf()),
            file_name: file
                .file_stem()
                .and_then(|name| name.to_str())
                .map(String::from),
            ..Default::default()
        }
    }

    fn with_package(mut self, package: &PackageLayout) -> Self {
        if package.has_manifest() {
            self.package_name = Some(package.name.clone());
            self.package_root = Some(package.root.clone());
        }
        self
    }

    fn with_module(mut self, module: ModuleSelection<'_>, root: Option<PathBuf>) -> Self {
        self.module_name = Some(module.name.to_owned());
        self.module_root = root;
        self
    }

    pub fn name_at_level(&self, level: ArchitectureLevel) -> Option<&str> {
        match level {
            ArchitectureLevel::Project => self.project_name.as_deref(),
            ArchitectureLevel::Package => self.package_name.as_deref(),
            ArchitectureLevel::Module => self.module_name.as_deref(),
            ArchitectureLevel::File => self.file_name.as_deref(),
        }
    }

    pub fn root_at_level(&self, level: ArchitectureLevel) -> Option<&Path> {
        match level {
            ArchitectureLevel::Project => self.project_root.as_deref(),
            ArchitectureLevel::Package => self.package_root.as_deref(),
            ArchitectureLevel::Module => self.module_root.as_deref(),
            ArchitectureLevel::File => self.file_path.as_deref(),
        }
    }

    pub fn qualified_name(&self, level: ArchitectureLevel) -> String {
        ArchitectureLevel::iter()
            .take_while(|current| current.as_u8() <= level.as_u8())
            .filter_map(|current| self.name_at_level(current))
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[derive(Debug, Clone, Default)]
struct ModuleDirectoryTree {
    direct_files: usize,
    children: HashMap<String, ModuleDirectoryTree>,
}

impl ModuleDirectoryTree {
    fn add_file(&mut self, relative_file: &Path, ignored_dirs: &[&str]) {
        let mut node = self;
        for component in semantic_components(relative_file.parent(), ignored_dirs) {
            node = node.children.entry(component.to_owned()).or_default();
        }
        node.direct_files += 1;
    }

    fn total_files(&self) -> usize {
        self.direct_files + self.children.values().map(Self::total_files).sum::<usize>()
    }

    fn sibling_files_excluding(&self, child_name: &str) -> usize {
        self.children
            .iter()
            .filter(|(name, _)| name.as_str() != child_name)
            .map(|(_, child)| child.total_files())
            .sum()
    }
}

#[derive(Debug, Clone)]
struct ProjectLayout {
    name: String,
    root: PathBuf,
}

impl ProjectLayout {
    fn from_files(files: &[PathBuf]) -> Self {
        let root = common_parent_dir(files);
        let name = path_name(&root).unwrap_or_else(|| "project".to_string());
        Self { name, root }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageOrigin {
    Manifest,
    Fallback,
}

#[derive(Debug, Clone)]
struct PackageLayout {
    name: String,
    root: PathBuf,
    modules: ModuleDirectoryTree,
    file_count: usize,
    origin: PackageOrigin,
}

impl PackageLayout {
    fn discover(files: &[PathBuf], manifest_name: &'static str) -> Vec<Self> {
        let mut roots = HashSet::new();
        let mut packages = Vec::new();

        for file in files {
            for dir in file.ancestors().skip(1) {
                if !dir.join(manifest_name).exists() {
                    continue;
                }
                if roots.insert(dir.to_path_buf()) {
                    packages.push(Self::manifest(
                        dir.to_path_buf(),
                        manifest_package_name(dir, manifest_name),
                    ));
                }
                break;
            }
        }

        packages.sort_by_key(|package| std::cmp::Reverse(package.root.components().count()));
        packages
    }

    fn with_indexed_modules(
        mut packages: Vec<Self>,
        files: &[PathBuf],
        ignored_dirs: &[&str],
    ) -> Vec<Self> {
        let roots: Vec<_> = packages
            .iter()
            .map(|package| package.root.clone())
            .collect();

        for package in &mut packages {
            for file in files {
                if package_owns_file(file, package, &roots) {
                    package.add_file(file, ignored_dirs);
                }
            }
        }

        packages
    }

    fn manifest(root: PathBuf, name: String) -> Self {
        Self::new(root, name, PackageOrigin::Manifest)
    }

    fn fallback(root: PathBuf, name: String) -> Self {
        Self::new(root, name, PackageOrigin::Fallback)
    }

    fn new(root: PathBuf, name: String, origin: PackageOrigin) -> Self {
        Self {
            name,
            root,
            modules: ModuleDirectoryTree::default(),
            file_count: 0,
            origin,
        }
    }

    fn has_manifest(&self) -> bool {
        self.origin == PackageOrigin::Manifest
    }

    fn add_file(&mut self, file: &Path, ignored_dirs: &[&str]) -> bool {
        let Ok(relative_file) = file.strip_prefix(&self.root) else {
            return false;
        };
        self.modules.add_file(relative_file, ignored_dirs);
        self.file_count += 1;
        true
    }
}

#[derive(Debug, Clone, Copy)]
struct ModuleSelection<'a> {
    name: &'a str,
    depth: usize,
}

#[derive(Debug, Clone, Copy)]
struct ModulePathStep<'a> {
    name: &'a str,
    tree: &'a ModuleDirectoryTree,
    sibling_files: usize,
}

impl<'a> ModulePathStep<'a> {
    fn as_boundary(self, depth: usize, min_sibling_files: usize) -> Option<ModuleSelection<'a>> {
        if self.sibling_files < min_sibling_files {
            return None;
        }

        let files_here = self.tree.total_files();
        let total_files = files_here + self.sibling_files;
        let dominance = files_here as f64 / total_files as f64;

        (dominance <= MAX_MODULE_DOMINANCE_RATIO).then_some(ModuleSelection {
            name: self.name,
            depth,
        })
    }

    fn fallback_boundary(self) -> ModuleSelection<'a> {
        ModuleSelection {
            name: self.name,
            depth: 0,
        }
    }
}

pub struct UnitMetaIndex {
    ignored_dirs: &'static [&'static str],
    project: ProjectLayout,
    packages: Vec<PackageLayout>,
}

impl UnitMetaIndex {
    pub fn from_language<L: crate::lang_def::Language>(files: &[PathBuf]) -> Self {
        Self::from_language_config(files, L::manifest_name(), L::container_dirs())
    }

    pub fn from_language_config(
        files: &[PathBuf],
        manifest_name: &'static str,
        ignored_dirs: &'static [&'static str],
    ) -> Self {
        let project = ProjectLayout::from_files(files);
        let mut packages = PackageLayout::discover(files, manifest_name);

        if packages.is_empty() {
            packages.push(PackageLayout::fallback(
                project.root.clone(),
                project.name.clone(),
            ));
        }

        let packages = PackageLayout::with_indexed_modules(packages, files, ignored_dirs);

        Self {
            ignored_dirs,
            project,
            packages,
        }
    }

    pub fn metadata_for(&self, file: &Path) -> UnitMeta {
        let meta = UnitMeta::for_file(&self.project.name, &self.project.root, file);
        let Some(package) = self.package_for(file) else {
            return meta;
        };

        let meta = meta.with_package(package);
        let Ok(relative_file) = file.strip_prefix(&package.root) else {
            return meta;
        };
        let Some(parent) = relative_file.parent() else {
            return meta;
        };

        let components: Vec<_> = semantic_components(Some(parent), self.ignored_dirs).collect();
        match choose_module(&components, package) {
            Some(module) => meta.with_module(
                module,
                module_root_for(parent, &package.root, self.ignored_dirs, module.depth),
            ),
            None => meta,
        }
    }

    fn package_for(&self, file: &Path) -> Option<&PackageLayout> {
        self.packages
            .iter()
            .filter(|package| file.starts_with(&package.root))
            .max_by_key(|package| package.root.components().count())
    }
}

fn package_owns_file(file: &Path, package: &PackageLayout, package_roots: &[PathBuf]) -> bool {
    if !file.starts_with(&package.root) {
        return false;
    }

    !package_roots.iter().any(|other| {
        *other != package.root && other.starts_with(&package.root) && file.starts_with(other)
    })
}

fn choose_module<'a>(
    components: &[&'a str],
    package: &'a PackageLayout,
) -> Option<ModuleSelection<'a>> {
    let steps = module_steps(components, &package.modules);
    let min_sibling_files = (package.file_count as f64 * MIN_MODULE_SIBLING_RATIO)
        .max(MIN_MODULE_SIBLING_FILES as f64) as usize;

    steps
        .iter()
        .enumerate()
        .find_map(|(depth, step)| step.as_boundary(depth, min_sibling_files))
        .or_else(|| steps.first().map(|step| step.fallback_boundary()))
}

fn module_steps<'a>(
    components: &[&'a str],
    modules: &'a ModuleDirectoryTree,
) -> Vec<ModulePathStep<'a>> {
    let mut node = modules;
    let mut steps = Vec::new();

    for component in components {
        let Some(child) = node.children.get(*component) else {
            break;
        };

        steps.push(ModulePathStep {
            name: component,
            tree: child,
            sibling_files: node.sibling_files_excluding(component),
        });
        node = child;
    }

    steps
}

fn manifest_package_name(dir: &Path, manifest_name: &'static str) -> String {
    std::fs::read_to_string(dir.join(manifest_name))
        .ok()
        .and_then(|content| parse_manifest_name(&content, manifest_name))
        .unwrap_or_else(|| path_name(dir).unwrap_or_else(|| "package".to_string()))
}

fn parse_manifest_name(content: &str, manifest_name: &str) -> Option<String> {
    match manifest_name {
        "package.json" => parse_package_json_name(content),
        "Cargo.toml" => parse_cargo_toml_name(content),
        _ => None,
    }
}

fn parse_package_json_name(content: &str) -> Option<String> {
    let manifest: serde_json::Value = serde_json::from_str(content).ok()?;
    manifest
        .get("name")
        .and_then(|name| name.as_str())
        .map(sanitize_package_name)
}

fn parse_cargo_toml_name(content: &str) -> Option<String> {
    let manifest: toml::Value = toml::from_str(content).ok()?;
    manifest
        .get("package")
        .and_then(|package| package.get("name"))
        .and_then(|name| name.as_str())
        .map(ToOwned::to_owned)
}

fn sanitize_package_name(name: &str) -> String {
    name.trim_start_matches('@').replace('/', "_")
}

fn common_parent_dir(paths: &[PathBuf]) -> PathBuf {
    let Some(first_parent) = paths.first().and_then(|path| path.parent()) else {
        return PathBuf::new();
    };

    let mut common: Vec<_> = first_parent.components().collect();
    for path in &paths[1..] {
        let parent = path.parent().unwrap_or(path);
        let common_len = common
            .iter()
            .zip(parent.components())
            .take_while(|(left, right)| **left == *right)
            .count();
        common.truncate(common_len);
        if common.is_empty() {
            break;
        }
    }

    common.iter().collect()
}

fn path_name(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
}

fn semantic_components<'a>(
    path: Option<&'a Path>,
    ignored_dirs: &'a [&str],
) -> impl Iterator<Item = &'a str> {
    path.into_iter()
        .flat_map(|path| path.components())
        .filter_map(|component| component.as_os_str().to_str())
        .filter(move |component| !ignored_dirs.contains(component))
}

fn module_root_for(
    relative_parent: &Path,
    package_root: &Path,
    ignored_dirs: &[&str],
    depth: usize,
) -> Option<PathBuf> {
    let mut root = package_root.to_path_buf();
    let mut semantic_depth = 0;

    for component in relative_parent
        .components()
        .filter_map(|component| component.as_os_str().to_str())
    {
        root = root.join(component);
        if ignored_dirs.contains(&component) {
            continue;
        }
        if semantic_depth == depth {
            return Some(root);
        }
        semantic_depth += 1;
    }

    None
}
