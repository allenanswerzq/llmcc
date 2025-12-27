use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::Deserialize;
use tree_sitter::Language;

use crate::node_types::NodeTypes;
use crate::{TokenEntry, format_block, format_hir, resolve_field_id, resolve_kind_id};

#[derive(Debug, Deserialize)]
pub struct TokenConfig {
    #[serde(default = "TokenConfig::default_hir_kind")]
    pub default_hir_kind: String,
    #[serde(default)]
    pub text_tokens: Vec<TextTokenConfig>,
    #[serde(default)]
    pub nodes: Vec<NodeTokenConfig>,
    #[serde(default)]
    pub fields: Vec<FieldTokenConfig>,
}

impl TokenConfig {
    fn default_hir_kind() -> String {
        "Internal".to_string()
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read token config {}", path.display()))?;
        let config: TokenConfig =
            toml::from_str(&text).with_context(|| format!("invalid TOML in {}", path.display()))?;
        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
pub struct TextTokenConfig {
    pub name: String,
    pub literal: String,
    #[serde(default)]
    pub hir_kind: Option<String>,
}

impl TextTokenConfig {
    pub fn to_token(&self, language: Language, config: &TokenConfig) -> Result<TokenEntry> {
        let kind_id = resolve_kind_id(language, &self.literal, false)?;
        let hir_kind = self.hir_kind.as_deref().unwrap_or(&config.default_hir_kind);
        Ok(TokenEntry {
            name: self.name.clone(),
            kind_id,
            repr: self.literal.clone(),
            hir_kind: format_hir(hir_kind),
            block_kind: None,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct NodeTokenConfig {
    pub ts_name: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hir_kind: Option<String>,
    #[serde(default)]
    pub block_kind: Option<String>,
    #[serde(default)]
    pub named: Option<bool>,
}

impl NodeTokenConfig {
    pub fn to_token(
        &self,
        language: Language,
        node_types: &NodeTypes,
        config: &TokenConfig,
    ) -> Result<TokenEntry> {
        let named = self
            .named
            .unwrap_or_else(|| node_types.is_named(&self.ts_name).unwrap_or(true));
        let kind_id = resolve_kind_id(language, &self.ts_name, named)?;
        let hir_kind = self.hir_kind.as_deref().unwrap_or(&config.default_hir_kind);
        let block_kind = self.block_kind.as_deref().map(format_block);
        Ok(TokenEntry {
            name: self.name.clone().unwrap_or_else(|| self.ts_name.clone()),
            kind_id,
            repr: self.ts_name.clone(),
            hir_kind: format_hir(hir_kind),
            block_kind,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct FieldTokenConfig {
    pub name: String,
    pub field_name: String,
    #[serde(default)]
    pub hir_kind: Option<String>,
    #[serde(default)]
    pub block_kind: Option<String>,
}

impl FieldTokenConfig {
    pub fn to_token(&self, language: Language, config: &TokenConfig) -> Result<TokenEntry> {
        let id = resolve_field_id(language, &self.field_name)?;
        let hir_kind = self.hir_kind.as_deref().unwrap_or(&config.default_hir_kind);
        let block_kind = self.block_kind.as_deref().map(format_block);
        Ok(TokenEntry {
            name: self.name.clone(),
            kind_id: id,
            repr: self.field_name.clone(),
            hir_kind: format_hir(hir_kind),
            block_kind,
        })
    }
}
