use std::fs;
use std::path::{Path, PathBuf};

use llmcc_error::{Error, ErrorKind, Result};
use llmcc_format::{GraphDepth, GraphDocument};
use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumString, IntoStaticStr};
use walkdir::WalkDir;

pub const SUITE_SCHEMA: &str = "llmcc.test";
pub const SUITE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonSuite {
    #[serde(default = "default_suite_schema")]
    pub schema: String,
    #[serde(default = "default_suite_schema_version")]
    pub schema_version: u32,
    pub cases: Vec<JsonCase>,
}

impl JsonSuite {
    fn validate(&self, path: &Path) -> Result<()> {
        if self.schema != SUITE_SCHEMA {
            return Err(Error::new(
                ErrorKind::InvalidFormat,
                format!(
                    "{} uses unsupported test schema '{}'",
                    path.display(),
                    self.schema
                ),
            ));
        }

        if self.schema_version != SUITE_SCHEMA_VERSION {
            return Err(Error::new(
                ErrorKind::InvalidFormat,
                format!(
                    "{} uses unsupported test schema version {}",
                    path.display(),
                    self.schema_version
                ),
            ));
        }

        if self.cases.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidFormat,
                format!("{} does not contain any test cases", path.display()),
            ));
        }

        for case in &self.cases {
            if case.id.trim().is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidFormat,
                    format!("{} contains a case with an empty id", path.display()),
                ));
            }

            if case.files.is_empty() {
                return Err(Error::new(
                    ErrorKind::InvalidFormat,
                    format!("case '{}' in {} has no files", case.id, path.display()),
                ));
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonCase {
    pub id: String,
    pub language: CaseLanguage,
    #[serde(default)]
    pub depth: GraphDepth,
    pub files: Vec<SourceFile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expect: Option<GraphDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: String,
    #[serde(alias = "content")]
    pub contents: String,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, EnumString, IntoStaticStr,
)]
#[strum(ascii_case_insensitive)]
pub enum CaseLanguage {
    #[serde(rename = "rust")]
    #[strum(to_string = "rust")]
    Rust,
    #[serde(rename = "cpp", alias = "c++", alias = "c")]
    #[strum(to_string = "cpp", serialize = "c++", serialize = "c")]
    Cpp,
    #[serde(rename = "typescript", alias = "ts")]
    #[strum(to_string = "typescript", serialize = "ts")]
    TypeScript,
}

pub struct SuiteFile {
    pub path: PathBuf,
    pub suite: JsonSuite,
    pub(crate) dirty: bool,
}

impl SuiteFile {
    fn load(path: PathBuf) -> Result<Self> {
        let text = fs::read_to_string(&path).map_err(|error| {
            Error::new(
                ErrorKind::IoFailed,
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        let suite: JsonSuite = serde_json::from_str(&text).map_err(|error| {
            Error::new(
                ErrorKind::DeserializationFailed,
                format!("failed to parse {}: {error}", path.display()),
            )
        })?;

        suite.validate(&path)?;

        Ok(Self {
            path,
            suite,
            dirty: false,
        })
    }

    pub fn write_if_dirty(&self) -> Result<()> {
        if !self.dirty {
            return Ok(());
        }

        let text = serde_json::to_string_pretty(&self.suite).map_err(|error| {
            Error::new(
                ErrorKind::SerializationFailed,
                format!("failed to serialize {}: {error}", self.path.display()),
            )
        })?;

        fs::write(&self.path, format!("{text}\n")).map_err(|error| {
            Error::new(
                ErrorKind::IoFailed,
                format!("failed to write {}: {error}", self.path.display()),
            )
        })
    }
}

pub fn load_suite_files(path: &Path) -> Result<Vec<SuiteFile>> {
    let mut paths = if path.is_file() {
        vec![path.to_path_buf()]
    } else if path.is_dir() {
        discover_json_files(path)?
    } else {
        return Err(Error::new(
            ErrorKind::FileNotFound,
            format!("{} is not a file or directory", path.display()),
        ));
    };

    paths.sort();

    if paths.is_empty() {
        return Err(Error::new(
            ErrorKind::FileNotFound,
            format!("{} does not contain any JSON test suites", path.display()),
        ));
    }

    paths.into_iter().map(SuiteFile::load).collect()
}

fn discover_json_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for entry in WalkDir::new(root) {
        let entry = entry.map_err(|error| {
            Error::new(
                ErrorKind::TraversalFailed,
                format!("failed to walk {}: {error}", root.display()),
            )
        })?;

        if !entry.file_type().is_file() {
            continue;
        }

        let is_json = entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"));

        if is_json {
            paths.push(entry.path().to_path_buf());
        }
    }

    Ok(paths)
}

fn default_suite_schema() -> String {
    SUITE_SCHEMA.to_string()
}

fn default_suite_schema_version() -> u32 {
    SUITE_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_language_string_conversions_are_derived() {
        assert_eq!(CaseLanguage::Rust.to_string(), "rust");
        assert_eq!(CaseLanguage::Cpp.to_string(), "cpp");
        assert_eq!(CaseLanguage::TypeScript.to_string(), "typescript");

        assert_eq!("rust".parse::<CaseLanguage>(), Ok(CaseLanguage::Rust));
        assert_eq!("C++".parse::<CaseLanguage>(), Ok(CaseLanguage::Cpp));
        assert_eq!("ts".parse::<CaseLanguage>(), Ok(CaseLanguage::TypeScript));

        let value: &'static str = CaseLanguage::TypeScript.into();
        assert_eq!(value, "typescript");
    }
}
