//! File discovery and filtering for llmcc.

use std::collections::HashSet;
use std::io;
use std::time::Instant;

use ignore::WalkBuilder;
use tracing::{info, warn};

use llmcc_core::Result;

use crate::LlmccOptions;

/// Directories to skip during file discovery.
fn should_skip_dir(name: &str) -> bool {
    matches!(
        name,
        "test"
            | "tests"
            | "testing"
            | "example"
            | "examples"
            | "doc"
            | "docs"
            | "bench"
            | "benches"
            | "benchmark"
            | "benchmarks"
            // Build output directories
            | "target"
            | "build"
            | "dist"
            | "out"
            // Vendor/dependency directories
            | "vendor"
            | "node_modules"
            | "third_party"
    )
}

/// Check if a file should be skipped (e.g., due to size).
/// Returns Some(reason) if skipped, None otherwise.
fn should_skip_file(path: &std::path::Path, opts: &LlmccOptions) -> Option<String> {
    let path_text = path.to_string_lossy();
    if opts.collapse_tests && is_test_path(&path_text) {
        return Some("test file collapsed".to_string());
    }
    if opts
        .exclude
        .iter()
        .any(|pattern| wildcard_match(pattern, &path_text))
    {
        return Some("matched --exclude".to_string());
    }
    None
}

fn is_test_path(path: &str) -> bool {
    path.contains("/tests/")
        || path.contains("\\tests\\")
        || path.ends_with("_test.rs")
        || path.ends_with("_test.go")
        || path.ends_with(".test.ts")
        || path.ends_with(".spec.ts")
        || path.contains("/__tests__/")
        || path.contains("\\__tests__\\")
}

fn wildcard_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return text == pattern || text.ends_with(pattern);
    }

    let mut remaining = text;
    if let Some(first) = parts.first()
        && !first.is_empty()
    {
        if !remaining.starts_with(first) && !remaining.contains(first) {
            return false;
        }
        let Some(pos) = remaining.find(first) else {
            return false;
        };
        remaining = &remaining[pos + first.len()..];
    }

    for part in parts.iter().skip(1).take(parts.len().saturating_sub(2)) {
        if part.is_empty() {
            continue;
        }
        let Some(pos) = remaining.find(part) else {
            return false;
        };
        remaining = &remaining[pos + part.len()..];
    }

    if let Some(last) = parts.last()
        && !last.is_empty()
    {
        return remaining.ends_with(last) || remaining.contains(last);
    }
    true
}

/// Discover files matching any of the given extensions.
///
/// Walks `opts.dirs` and collects files with matching extensions,
/// plus any explicit `opts.files`.
pub fn discover_files(opts: &LlmccOptions, extensions: &HashSet<&str>) -> Result<Vec<String>> {
    let discovery_start = Instant::now();

    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut skipped_count = 0usize;

    // Helper to add a path if not seen and not skipped
    let mut add_path = |path: &str| {
        if seen.contains(path) {
            return;
        }
        if let Some(reason) = should_skip_file(std::path::Path::new(path), opts) {
            warn!("Skipping {}: {}", path, reason);
            skipped_count += 1;
            return;
        }
        seen.insert(path.to_string());
        files.push(path.to_string());
    };

    // Add explicit files
    for file in &opts.files {
        add_path(file);
    }

    // Walk directories
    if !opts.dirs.is_empty() {
        let walker_threads = std::thread::available_parallelism()
            .map(|v| v.get())
            .unwrap_or(1);

        for dir in &opts.dirs {
            let mut builder = WalkBuilder::new(dir);
            builder
                .standard_filters(true)
                .follow_links(false)
                .threads(walker_threads)
                .filter_entry(|entry| {
                    // Always include root
                    if entry.depth() == 0 {
                        return true;
                    }
                    // Non-directories pass through
                    let Some(file_type) = entry.file_type() else {
                        return true;
                    };
                    if !file_type.is_dir() {
                        return true;
                    }
                    // Filter directories by name
                    let Some(name) = entry.file_name().to_str() else {
                        return true;
                    };
                    !should_skip_dir(&name.to_ascii_lowercase())
                });

            for entry in builder.build() {
                let entry = entry.map_err(|e| {
                    io::Error::other(format!("Failed to walk directory {dir}: {e}"))
                })?;

                // Only process files
                if !entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
                    continue;
                }

                let path = entry.path();
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    continue;
                };

                if extensions.contains(ext) {
                    add_path(&path.to_string_lossy());
                }
            }
        }
    }

    if skipped_count > 0 {
        info!("Skipped {} files due to size limits", skipped_count);
    }

    info!(
        "File discovery: {:.2}s ({} files)",
        discovery_start.elapsed().as_secs_f64(),
        files.len()
    );

    if files.is_empty() {
        return Err(
            "No input files found. Check that the directory contains supported file types.".into(),
        );
    }

    Ok(files)
}
