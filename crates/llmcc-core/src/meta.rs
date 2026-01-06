//! Module detection using a Patricia trie to compress file paths into 4 architecture levels.
//!
//! # The 4-Level Architecture
//!
//! Every source file is mapped to exactly 4 semantic levels:
//! - **Level 0 (Project)**: The entire repository/workspace
//! - **Level 1 (Package)**: A distributable unit (npm package, Rust crate) - DEVELOPER DEFINED
//! - **Level 2 (Module)**: A major subsystem within a package - INFERRED
//! - **Level 3 (File)**: The individual source file
//!
//! # Philosophy
//!
//! Packages (Cargo.toml, package.json) are the **real semantic boundaries** - developers
//! explicitly created them. We respect these as-is.
//!
//! For modules, we use a per-file bottom-up approach: walk up from each file toward the
//! package root, finding the first directory that represents a meaningful grouping.
//!
//! # Algorithm: Per-File Bottom-Up Module Detection
//!
//! For each file:
//! 1. Get path components from package root (excluding containers like `src/`)
//! 2. Walk UP from deepest to shallowest
//! 3. Find the first ancestor where going "deeper" provides meaningful subdivision
//!
//! A directory is a good module boundary if:
//! - It has siblings (alternatives at the same level)
//! - Its siblings collectively represent >20% of the package's files
//!
//! This naturally handles variable depths - different subtrees can have different module levels.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ============================================================================
// Public Types
// ============================================================================

/// The four fixed architecture levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArchDepth {
    Project = 0,
    Package = 1,
    Module = 2,
    File = 3,
}

impl ArchDepth {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(depth: u8) -> Option<Self> {
        match depth {
            0 => Some(Self::Project),
            1 => Some(Self::Package),
            2 => Some(Self::Module),
            3 => Some(Self::File),
            _ => None,
        }
    }
}

/// Complete location info for a source file at all 4 depths.
#[derive(Debug, Clone, Default)]
pub struct UnitMeta {
    pub project_name: Option<String>,
    pub project_root: Option<PathBuf>,
    pub package_name: Option<String>,
    pub package_root: Option<PathBuf>,
    pub module_name: Option<String>,
    pub module_root: Option<PathBuf>,
    pub file_name: Option<String>,
    pub file_path: Option<PathBuf>,
}

impl UnitMeta {
    pub fn name_at_depth(&self, depth: ArchDepth) -> Option<&str> {
        match depth {
            ArchDepth::Project => self.project_name.as_deref(),
            ArchDepth::Package => self.package_name.as_deref(),
            ArchDepth::Module => self.module_name.as_deref(),
            ArchDepth::File => self.file_name.as_deref(),
        }
    }

    pub fn root_at_depth(&self, depth: ArchDepth) -> Option<&Path> {
        match depth {
            ArchDepth::Project => self.project_root.as_deref(),
            ArchDepth::Package => self.package_root.as_deref(),
            ArchDepth::Module => self.module_root.as_deref(),
            ArchDepth::File => self.file_path.as_deref(),
        }
    }

    pub fn qualified_name(&self, depth: ArchDepth) -> String {
        let mut parts = Vec::new();
        for d in 0..=depth.as_u8() {
            if let Some(arch_depth) = ArchDepth::from_u8(d)
                && let Some(name) = self.name_at_depth(arch_depth)
            {
                parts.push(name);
            }
        }
        parts.join(".")
    }
}

// ============================================================================
// Trie Node
// ============================================================================

/// A node in the Patricia trie.
///
/// Each node represents a semantic directory (containers are skipped).
/// file_count tracks files directly at this node's level.
#[derive(Debug, Clone, Default)]
struct TrieNode {
    file_count: usize,
    children: HashMap<String, TrieNode>,
}

impl TrieNode {
    fn new() -> Self {
        Self::default()
    }

    /// Total files in this subtree (recursive).
    fn total_files(&self) -> usize {
        self.file_count
            + self
                .children
                .values()
                .map(|c| c.total_files())
                .sum::<usize>()
    }
}

// ============================================================================
// Package Info
// ============================================================================

#[derive(Debug, Clone)]
struct PackageInfo {
    name: String,
    root: PathBuf,
    trie: TrieNode,
    total_files: usize,
}

// ============================================================================
// Module Detector
// ============================================================================

/// Detects and caches module structure for a project.
pub struct UnitMetaBuilder {
    manifest_name: &'static str,
    container_dirs: &'static [&'static str],
    project_root: PathBuf,
    project_name: String,
    packages: Vec<PackageInfo>,
}

impl UnitMetaBuilder {
    /// Create a detector using LanguageTrait configuration.
    /// This is the preferred way to create a UnitMetaBuilder from a language type.
    pub fn from_lang_trait<L: crate::lang_def::LanguageTrait>(
        files: &[PathBuf],
        project_root: &Path,
    ) -> Self {
        Self::with_lang_config(files, project_root, L::manifest_name(), L::container_dirs())
    }

    /// Create a detector with explicit language configuration.
    pub fn with_lang_config(
        files: &[PathBuf],
        project_root: &Path,
        manifest_name: &'static str,
        container_dirs: &'static [&'static str],
    ) -> Self {
        let project_name = project_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("project")
            .to_string();

        let mut detector = Self {
            manifest_name,
            container_dirs,
            project_root: project_root.to_path_buf(),
            project_name,
            packages: Vec::new(),
        };

        detector.detect_packages(files);
        detector.build_tries(files);

        detector
    }

    fn is_container(&self, name: &str) -> bool {
        self.container_dirs.contains(&name)
    }

    /// Get module info for a file path.
    pub fn get_module_info(&self, file: &Path) -> UnitMeta {
        self.compute_module_info(file)
    }

    // ========================================================================
    // Step 1: Detect Packages
    // ========================================================================

    fn detect_packages(&mut self, files: &[PathBuf]) {
        let mut seen = std::collections::HashSet::new();

        for file in files {
            let mut dir = file.parent();
            while let Some(current) = dir {
                if !current.starts_with(&self.project_root) {
                    break;
                }

                let manifest = current.join(self.manifest_name);
                if manifest.exists() && !seen.contains(current) {
                    seen.insert(current.to_path_buf());

                    if let Some(name) = self.parse_manifest_name(current) {
                        self.packages.push(PackageInfo {
                            name,
                            root: current.to_path_buf(),
                            trie: TrieNode::new(),
                            total_files: 0,
                        });
                    }
                    break;
                }
                dir = current.parent();
            }
        }

        // Sort by depth (deepest first) for nested package detection
        self.packages.sort_by(|a, b| {
            b.root
                .components()
                .count()
                .cmp(&a.root.components().count())
        });
    }

    fn parse_manifest_name(&self, dir: &Path) -> Option<String> {
        let content = std::fs::read_to_string(dir.join(self.manifest_name)).ok()?;

        // Try JSON format first (package.json)
        if self.manifest_name == "package.json" {
            // Parse "name": "value" from JSON
            let name_pos = content.find("\"name\"")?;
            let after_name = &content[name_pos + 6..];
            let colon_pos = after_name.find(':')?;
            let after_colon = &after_name[colon_pos + 1..];
            let start_quote = after_colon.find('"')?;
            let value_start = &after_colon[start_quote + 1..];
            let end_quote = value_start.find('"')?;
            let value = &value_start[..end_quote];
            Some(value.replace('@', "").replace('/', "_"))
        } else if self.manifest_name == "Cargo.toml" {
            // Parse TOML format (Cargo.toml)
            let mut in_package = false;
            for line in content.lines() {
                let line = line.trim();
                if line == "[package]" {
                    in_package = true;
                } else if line.starts_with('[') {
                    in_package = false;
                } else if in_package && line.starts_with("name") {
                    return line
                        .find('=')
                        .map(|pos| line[pos + 1..].trim().trim_matches('"').to_string());
                }
            }
            None
        } else {
            // Unknown manifest format - use directory name
            dir.file_name().and_then(|n| n.to_str()).map(|s| s.to_string())
        }
    }

    // ========================================================================
    // Step 2: Build Tries
    // ========================================================================

    fn build_tries(&mut self, files: &[PathBuf]) {
        let all_roots: Vec<PathBuf> = self.packages.iter().map(|p| p.root.clone()).collect();

        for pkg in &mut self.packages {
            for file in files {
                // Skip files not in this package
                if !file.starts_with(&pkg.root) {
                    continue;
                }

                // Skip files belonging to nested packages
                let in_nested = all_roots.iter().any(|other| {
                    *other != pkg.root && other.starts_with(&pkg.root) && file.starts_with(other)
                });
                if in_nested {
                    continue;
                }

                Self::insert_file(&mut pkg.trie, file, &pkg.root, self.container_dirs);
                pkg.total_files += 1;
            }

            tracing::debug!("Package '{}': {} files in trie", pkg.name, pkg.total_files);
        }
    }

    fn insert_file(
        trie: &mut TrieNode,
        file: &Path,
        pkg_root: &Path,
        container_dirs: &[&str],
    ) {
        let rel_path = match file.strip_prefix(pkg_root) {
            Ok(p) => p,
            Err(_) => return,
        };

        // Get directory components, skipping containers
        let mut current = trie;
        for comp in rel_path
            .parent()
            .into_iter()
            .flat_map(|p| p.components())
            .filter_map(|c| c.as_os_str().to_str())
        {
            if container_dirs.contains(&comp) {
                continue; // Skip container directories
            }
            current = current.children.entry(comp.to_string()).or_default();
        }

        current.file_count += 1;
    }

    // ========================================================================
    // Per-File Module Detection
    // ========================================================================

    /// Find the module for a file by walking up from the file to the package root.
    ///
    /// Strategy: Find the first ancestor directory that represents a meaningful grouping.
    /// A directory is "meaningful" if:
    /// 1. It has siblings (alternatives at the same level), AND
    /// 2. Those siblings collectively have significant file counts
    ///
    /// This naturally handles variable depths for different subtrees.
    fn find_module_for_file<'a>(
        &self,
        components: &[&'a str],
        pkg: &PackageInfo,
    ) -> Option<(usize, &'a str)> {
        if components.is_empty() {
            return None;
        }

        // Walk the trie along the file's path, collecting nodes and checking siblings
        let mut current = &pkg.trie;
        let mut path_nodes: Vec<(&str, &TrieNode, usize)> = Vec::new(); // (name, node, sibling_files)

        for comp in components.iter() {
            if let Some(child) = current.children.get(*comp) {
                // Calculate sibling file count (files in siblings, not in this subtree)
                let sibling_files: usize = current
                    .children
                    .iter()
                    .filter(|(name, _)| *name != *comp)
                    .map(|(_, node)| node.total_files())
                    .sum();

                path_nodes.push((*comp, child, sibling_files));
                current = child;
            } else {
                // Component not in trie (shouldn't happen normally)
                break;
            }
        }

        // Walk from ROOT to LEAF looking for the best module boundary
        // A good boundary has:
        // 1. Significant siblings (not alone)
        // 2. Balanced distribution (no sibling dominates >80%)
        //
        // If we find a significant but imbalanced level, keep looking deeper
        let significance_threshold = (pkg.total_files as f64 * 0.05).max(1.0) as usize;
        const DOMINANCE_THRESHOLD: f64 = 0.80;

        for (i, (name, node, sibling_files)) in path_nodes.iter().enumerate() {
            if *sibling_files >= significance_threshold {
                // Significant siblings - check balance
                let my_files = node.total_files();
                let total = my_files + sibling_files;
                let dominance = my_files as f64 / total as f64;

                if dominance <= DOMINANCE_THRESHOLD {
                    // Balanced - use this level
                    return Some((i, *name));
                }
                // Imbalanced - we dominate, keep looking for a better split deeper
            }
        }

        // No balanced split found - use the first component
        path_nodes.first().map(|(name, _, _)| (0, *name))
    }

    // ========================================================================
    // Module Info Lookup
    // ========================================================================

    fn compute_module_info(&self, file: &Path) -> UnitMeta {
        let mut info = UnitMeta {
            project_name: Some(self.project_name.clone()),
            project_root: Some(self.project_root.clone()),
            file_path: Some(file.to_path_buf()),
            file_name: file.file_stem().and_then(|s| s.to_str()).map(String::from),
            ..Default::default()
        };

        // Find package
        let pkg = self
            .packages
            .iter()
            .filter(|p| file.starts_with(&p.root))
            .max_by_key(|p| p.root.components().count());

        let Some(pkg) = pkg else {
            return info;
        };

        info.package_name = Some(pkg.name.clone());
        info.package_root = Some(pkg.root.clone());

        // Get path components (excluding containers)
        let rel_path = match file.strip_prefix(&pkg.root) {
            Ok(p) => p,
            Err(_) => return info,
        };

        let components: Vec<&str> = rel_path
            .parent()
            .into_iter()
            .flat_map(|p| p.components())
            .filter_map(|c| c.as_os_str().to_str())
            .filter(|c| !self.is_container(c))
            .collect();

        // Find module using per-file bottom-up detection
        if let Some((depth, module_name)) = self.find_module_for_file(&components, pkg) {
            info.module_name = Some(module_name.to_string());

            // Reconstruct module root path
            let mut root = pkg.root.clone();
            let mut non_container_count = 0;
            for comp in rel_path
                .parent()
                .into_iter()
                .flat_map(|p| p.components())
                .filter_map(|c| c.as_os_str().to_str())
            {
                root = root.join(comp);
                if !self.is_container(comp) {
                    if non_container_count == depth {
                        info.module_root = Some(root);
                        break;
                    }
                    non_container_count += 1;
                }
            }
        }
        // If no module found (file at package root), module_name stays None
        // and the graph generator will use the file name

        info
    }
}
