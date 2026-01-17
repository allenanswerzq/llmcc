//! File discovery and filtering for llmcc.

use std::collections::HashSet;
use std::io;
use std::time::Instant;

use ignore::WalkBuilder;
use tracing::info;

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

/// Check if a file is auto-generated code that should be skipped.
/// These files are typically generated from .proto files or other schema definitions.
fn is_generated_file(path: &std::path::Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    // Protobuf generated files
    if file_name.ends_with(".pb.h")
        || file_name.ends_with(".pb.cc")
        || file_name.ends_with(".pb.c")
        || file_name.ends_with(".pb.go")
    {
        return true;
    }

    // gRPC UPB (micro protobuf) generated files
    if file_name.ends_with(".upb.h")
        || file_name.ends_with(".upb.c")
        || file_name.ends_with(".upbdefs.h")
        || file_name.ends_with(".upbdefs.c")
        || file_name.ends_with(".upb_minitable.h")
        || file_name.ends_with(".upb_minitable.c")
    {
        return true;
    }

    // FlatBuffers generated files
    if file_name.ends_with("_generated.h") {
        return true;
    }

    // Thrift generated files
    if file_name.ends_with("_types.h")
        && path.to_string_lossy().contains("gen-cpp")
    {
        return true;
    }

    // gRPC generated files
    if file_name.ends_with(".grpc.pb.h") || file_name.ends_with(".grpc.pb.cc") {
        return true;
    }

    false
}

/// Check if a file should be skipped (e.g., generated code).
/// Returns Some(reason) if skipped, None otherwise.
fn should_skip_file(path: &std::path::Path) -> Option<String> {
    if is_generated_file(path) {
        return Some("auto-generated file".to_string());
    }
    None
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
        if should_skip_file(std::path::Path::new(path)).is_some() {
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
        info!("Skipped {} auto-generated files (protobuf, flatbuffers, etc.)", skipped_count);
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
