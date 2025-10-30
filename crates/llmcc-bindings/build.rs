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
