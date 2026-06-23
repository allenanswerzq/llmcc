use std::fs;
use std::path::{Path, PathBuf};

use llmcc_error::{Error, ErrorKind, Result};
use tempfile::TempDir;

use crate::corpus::CorpusCase;

pub(crate) struct MaterializedProject {
    #[allow(dead_code)]
    temp_dir: Option<TempDir>,
    root_path: PathBuf,
}

impl MaterializedProject {
    pub(crate) fn root(&self) -> &Path {
        &self.root_path
    }

    pub(crate) fn is_persistent(&self) -> bool {
        self.temp_dir.is_none()
    }
}

pub(crate) fn materialize_case(case: &CorpusCase, keep_temps: bool) -> Result<MaterializedProject> {
    let temp_dir = tempfile::tempdir().map_err(|e| {
        Error::new(
            ErrorKind::IoFailed,
            format!("failed to create temp dir for llmcc-test: {e}"),
        )
    })?;
    let root_path = temp_dir.path().to_path_buf();

    for (idx, file) in case.files.iter().enumerate() {
        let original_path = Path::new(&file.path);
        let file_name_str = original_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        let final_path = if file_name_str == "Cargo.toml" || file_name_str == "package.json" {
            original_path.to_path_buf()
        } else {
            let prefixed_filename = format!("{idx:03}_{file_name_str}");
            original_path
                .parent()
                .map(|parent| parent.join(&prefixed_filename))
                .unwrap_or_else(|| PathBuf::from(&prefixed_filename))
        };

        let abs_path = root_path.join(&final_path);
        if let Some(parent) = abs_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Error::new(
                    ErrorKind::IoFailed,
                    format!("failed to create {}: {}", parent.display(), e),
                )
            })?;
        }
        fs::write(&abs_path, file.contents.as_bytes()).map_err(|e| {
            Error::new(
                ErrorKind::IoFailed,
                format!(
                    "failed to write virtual file {} for {}: {}",
                    abs_path.display(),
                    case.id(),
                    e
                ),
            )
        })?;
    }

    if keep_temps {
        let preserved = temp_dir.keep();
        return Ok(MaterializedProject {
            temp_dir: None,
            root_path: preserved,
        });
    }

    Ok(MaterializedProject {
        temp_dir: Some(temp_dir),
        root_path,
    })
}
