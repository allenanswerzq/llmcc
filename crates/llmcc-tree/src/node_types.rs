//! Node types parsing from tree-sitter JSON.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use llmcc_error::{Error, ErrorKind};
use serde::Deserialize;

use crate::Result;

/// Parsed node types from tree-sitter's node-types.json.
#[derive(Debug, Default)]
pub struct NodeTypes {
    named: HashMap<String, bool>,
}

impl NodeTypes {
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|e| {
            Error::new(
                ErrorKind::FileNotFound,
                format!("failed to read {}: {}", path.display(), e),
            )
        })?;
        Self::from_str(&contents)
    }

    pub fn from_str(contents: &str) -> Result<Self> {
        let entries: Vec<NodeTypeEntry> = serde_json::from_str(contents).map_err(|e| {
            Error::new(
                ErrorKind::DeserializationFailed,
                format!("invalid node-types JSON: {}", e),
            )
        })?;
        let mut named = HashMap::new();
        for entry in entries {
            named.entry(entry.kind).or_insert(entry.named);
        }
        Ok(Self { named })
    }

    pub fn is_named(&self, name: &str) -> Option<bool> {
        self.named.get(name).copied()
    }
}

#[derive(Debug, Deserialize)]
struct NodeTypeEntry {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    named: bool,
}
