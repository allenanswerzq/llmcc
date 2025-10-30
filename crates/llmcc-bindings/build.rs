fn main() {
    // On macOS, ensure proper Python library linking
    #[cfg(target_os = "macos")]
    {
        // Try to get Python library from environment or standard locations
        if let Ok(python_lib_dir) = std::env::var("PYTHON_LIB_DIR") {
            println!("cargo:rustc-link-search=native={}", python_lib_dir);
        } else {
            // Try common macOS Python locations
            let common_paths = vec![
                "/opt/homebrew/opt/python/Frameworks/Python.framework/Versions/3.11/lib",
                "/opt/homebrew/opt/python/Frameworks/Python.framework/Versions/3.12/lib",
                "/opt/homebrew/opt/python/Frameworks/Python.framework/Versions/3.10/lib",
                "/usr/local/opt/python/Frameworks/Python.framework/Versions/3.11/lib",
                "/usr/local/opt/python/Frameworks/Python.framework/Versions/3.12/lib",
                "/usr/local/opt/python/Frameworks/Python.framework/Versions/3.10/lib",
            ];

            for path in common_paths {
                if std::path::PathBuf::from(path).exists() {
                    println!("cargo:rustc-link-search=native={}", path);
                    break;
                }
            }
        }

        // If provided, link directly against the detected Python library
        fn sanitize_lib_name(raw: &str) -> Option<String> {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }

            let mut candidate = trimmed;
            if let Some(stripped) = candidate.strip_prefix("lib") {
                candidate = stripped;
            }

            let candidate = candidate
                .trim_end_matches(".dylib")
                .trim_end_matches(".tbd")
                .trim_end_matches(".a");

            if candidate.is_empty() {
                None
            } else {
                Some(candidate.to_string())
            }
        }

        let mut python_lib_name = std::env::var("PYTHON_LIB_NAME")
            .ok()
            .and_then(|name| sanitize_lib_name(&name));

        if python_lib_name.is_none() {
            if let Ok(python_lib_dir) = std::env::var("PYTHON_LIB_DIR") {
                if let Ok(entries) = std::fs::read_dir(&python_lib_dir) {
                    for entry in entries.flatten() {
                        let file_name = entry.file_name();
                        if let Some(name) = file_name.to_str() {
                            if let Some(parsed) = sanitize_lib_name(name) {
                                python_lib_name = Some(parsed);
                                break;
                            }
                        }
                    }
                }
            }
        }

        if let Some(lib) = python_lib_name {
            println!("cargo:rustc-link-lib=dylib={}", lib);
        }

        // Ensure MACOSX_DEPLOYMENT_TARGET is set if it's already in env
        if let Ok(deployment_target) = std::env::var("MACOSX_DEPLOYMENT_TARGET") {
            println!(
                "cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET={}",
                deployment_target
            );
        }
    }

    // Tell cargo to invalidate the built crate if this build script changes
    println!("cargo:rerun-if-changed=build.rs");
}
