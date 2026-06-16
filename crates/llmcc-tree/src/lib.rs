//! Tree-sitter token-map code generation for llmcc language crates.
//!
//! The public API intentionally exposes only generation entry points. TOML
//! config parsing and node-types indexing stay internal so build scripts cannot
//! depend on partial intermediate representations.

use std::collections::{HashMap, HashSet};
use std::fmt::Write;
use std::fs;
use std::path::Path;

use llmcc_error::{Error, ErrorKind};
use serde::Deserialize;
use tree_sitter::Language;

/// Result type for this crate.
pub type Result<T> = llmcc_error::Result<T>;

/// Source for tree-sitter `node-types.json` metadata.
pub enum NodeTypesSource<'a> {
    /// Load `node-types.json` from a file path.
    Path(&'a Path),
    /// Use embedded `node-types.json` contents.
    Embedded(&'a str),
}

impl<'a> NodeTypesSource<'a> {
    fn load(self) -> Result<NodeTypes> {
        match self {
            Self::Path(path) => NodeTypes::from_path(path),
            Self::Embedded(contents) => NodeTypes::from_str(contents),
        }
    }
}

/// Generate Rust token-map source.
pub fn generate_tokens(
    language_ident: &str,
    language: Language,
    node_types: NodeTypesSource<'_>,
    config_path: &Path,
) -> Result<String> {
    let config = TokenConfig::from_path(config_path)?;
    let node_types = node_types.load()?;
    let set = build_token_set(language, &node_types, &config)?;
    Ok(set.render(language_ident))
}

#[derive(Debug, Deserialize)]
struct TokenConfig {
    #[serde(default = "TokenConfig::default_hir_kind")]
    default_hir_kind: String,
    #[serde(default)]
    text_tokens: Vec<TextTokenConfig>,
    #[serde(default)]
    nodes: Vec<NodeTokenConfig>,
    #[serde(default)]
    fields: Vec<FieldTokenConfig>,
}

impl TokenConfig {
    fn default_hir_kind() -> String {
        "Internal".to_string()
    }

    fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|error| {
            Error::new(
                ErrorKind::FileNotFound,
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        toml::from_str(&text).map_err(|error| {
            Error::new(
                ErrorKind::ConfigInvalid,
                format!("invalid TOML in {}: {error}", path.display()),
            )
        })
    }

    fn validate_names(&self) -> Result<()> {
        let mut names = HashSet::new();

        for token in &self.text_tokens {
            insert_unique_name(&mut names, &token.name)?;
        }
        for token in &self.nodes {
            insert_unique_name(&mut names, token.generated_name())?;
        }
        for token in &self.fields {
            insert_unique_name(&mut names, &token.name)?;
        }

        Ok(())
    }
}

fn insert_unique_name<'a>(names: &mut HashSet<&'a str>, name: &'a str) -> Result<()> {
    if names.insert(name) {
        return Ok(());
    }

    Err(Error::new(
        ErrorKind::ConfigInvalid,
        format!("duplicate generated token name '{name}'"),
    ))
}

#[derive(Debug, Deserialize)]
struct TextTokenConfig {
    name: String,
    literal: String,
    #[serde(default)]
    hir_kind: Option<String>,
}

impl TextTokenConfig {
    fn to_token(&self, language: Language, config: &TokenConfig) -> Result<TokenEntry> {
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
struct NodeTokenConfig {
    ts_name: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    hir_kind: Option<String>,
    #[serde(default)]
    block_kind: Option<String>,
    #[serde(default)]
    named: Option<bool>,
}

impl NodeTokenConfig {
    fn generated_name(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.ts_name)
    }

    fn to_token(
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
struct FieldTokenConfig {
    name: String,
    field_name: String,
    #[serde(default)]
    hir_kind: Option<String>,
    #[serde(default)]
    block_kind: Option<String>,
}

impl FieldTokenConfig {
    fn to_token(&self, language: Language, config: &TokenConfig) -> Result<TokenEntry> {
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

#[derive(Debug, Clone)]
struct TokenEntry {
    name: String,
    kind_id: u16,
    repr: String,
    hir_kind: String,
    block_kind: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct TokenSet {
    text_tokens: Vec<TokenEntry>,
    node_tokens: Vec<TokenEntry>,
    field_tokens: Vec<TokenEntry>,
}

impl TokenSet {
    fn render(&self, language_ident: &str) -> String {
        let mut out = String::new();
        out.push_str("define_lang! {\n");
        out.push_str(&format!("    {language_ident},\n"));

        if !self.text_tokens.is_empty() {
            out.push_str("    // Text tokens\n");
            for entry in &self.text_tokens {
                render_entry(&mut out, entry);
            }
            out.push('\n');
        }

        if !self.node_tokens.is_empty() {
            out.push_str("    // Node tokens\n");
            for entry in &self.node_tokens {
                render_entry(&mut out, entry);
            }
            out.push('\n');
        }

        if !self.field_tokens.is_empty() {
            out.push_str("    // Field tokens\n");
            for entry in &self.field_tokens {
                render_entry(&mut out, entry);
            }
        }

        out.push_str("}\n");
        out
    }
}

fn render_entry(out: &mut String, entry: &TokenEntry) {
    let repr = format!("{:?}", entry.repr);
    if let Some(block) = &entry.block_kind {
        let _ = writeln!(
            out,
            "    ({}, {:>4}, {:<20}, {}, BlockKind::{}),",
            entry.name, entry.kind_id, repr, entry.hir_kind, block
        );
    } else {
        let _ = writeln!(
            out,
            "    ({}, {:>4}, {:<20}, {}),",
            entry.name, entry.kind_id, repr, entry.hir_kind
        );
    }
}

fn build_token_set(
    language: Language,
    node_types: &NodeTypes,
    config: &TokenConfig,
) -> Result<TokenSet> {
    config.validate_names()?;

    let mut set = TokenSet::default();
    for entry in &config.text_tokens {
        set.text_tokens
            .push(entry.to_token(language.clone(), config)?);
    }
    for entry in &config.nodes {
        set.node_tokens
            .push(entry.to_token(language.clone(), node_types, config)?);
    }
    for entry in &config.fields {
        set.field_tokens
            .push(entry.to_token(language.clone(), config)?);
    }
    Ok(set)
}

fn format_hir(kind: &str) -> String {
    if kind.starts_with("HirKind::") {
        kind.to_string()
    } else {
        format!("HirKind::{kind}")
    }
}

fn format_block(kind: &str) -> String {
    if kind.starts_with("BlockKind::") {
        kind.to_string().replacen("BlockKind::", "", 1)
    } else {
        kind.to_string()
    }
}

fn resolve_kind_id(language: Language, name: &str, named: bool) -> Result<u16> {
    let id = language.id_for_node_kind(name, named);
    if id == u16::MAX {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            format!("node kind '{name}' (named={named}) not found"),
        ));
    }
    Ok(id)
}

fn resolve_field_id(language: Language, field_name: &str) -> Result<u16> {
    let Some(field_id) = language.field_id_for_name(field_name.as_bytes()) else {
        return Err(Error::new(
            ErrorKind::InvalidArgument,
            format!("field '{field_name}' not found"),
        ));
    };
    Ok(field_id.get())
}

#[derive(Debug, Default)]
struct NodeTypes {
    named: HashMap<String, bool>,
}

impl NodeTypes {
    fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let contents = fs::read_to_string(path).map_err(|error| {
            Error::new(
                ErrorKind::FileNotFound,
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        Self::from_str(&contents)
    }

    fn from_str(contents: &str) -> Result<Self> {
        let entries: Vec<NodeTypeEntry> = serde_json::from_str(contents).map_err(|error| {
            Error::new(
                ErrorKind::DeserializationFailed,
                format!("invalid node-types JSON: {error}"),
            )
        })?;
        let mut named = HashMap::new();
        for entry in entries {
            named.entry(entry.kind).or_insert(entry.named);
        }
        Ok(Self { named })
    }

    fn is_named(&self, name: &str) -> Option<bool> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_hir_and_block_kinds() {
        assert_eq!(format_hir("Identifier"), "HirKind::Identifier");
        assert_eq!(format_hir("HirKind::Text"), "HirKind::Text");
        assert_eq!(format_block("Func"), "Func");
        assert_eq!(format_block("BlockKind::Class"), "Class");
    }

    #[test]
    fn token_set_render_preserves_sections() {
        let set = TokenSet {
            text_tokens: vec![TokenEntry {
                name: "Text_plus".to_string(),
                kind_id: 1,
                repr: "+".to_string(),
                hir_kind: "HirKind::Text".to_string(),
                block_kind: None,
            }],
            node_tokens: vec![TokenEntry {
                name: "function_item".to_string(),
                kind_id: 2,
                repr: "function_item".to_string(),
                hir_kind: "HirKind::Scope".to_string(),
                block_kind: Some("Func".to_string()),
            }],
            field_tokens: Vec::new(),
        };

        let rendered = set.render("LangTest");
        assert!(rendered.contains("define_lang!"));
        assert!(rendered.contains("LangTest"));
        assert!(rendered.contains("(Text_plus,"));
        assert!(rendered.contains("BlockKind::Func"));
    }

    #[test]
    fn default_hir_kind_is_internal() {
        let config: TokenConfig = toml::from_str("").unwrap();
        assert_eq!(config.default_hir_kind, "Internal");
        assert!(config.validate_names().is_ok());
    }

    #[test]
    fn validates_duplicate_names_across_sections() {
        let config: TokenConfig = toml::from_str(
            r#"
[[nodes]]
ts_name = "function_item"
name = "dup"

[[fields]]
name = "dup"
field_name = "name"
"#,
        )
        .unwrap();

        let error = config.validate_names().unwrap_err();
        assert!(
            error
                .to_string()
                .contains("duplicate generated token name 'dup'")
        );
    }

    #[test]
    fn validates_duplicate_default_node_names() {
        let config: TokenConfig = toml::from_str(
            r#"
[[nodes]]
ts_name = "identifier"

[[text_tokens]]
name = "identifier"
literal = "identifier"
"#,
        )
        .unwrap();

        assert!(config.validate_names().is_err());
    }

    #[test]
    fn parses_named_flags_by_node_type() {
        let node_types = NodeTypes::from_str(
            r#"[
                {"type":"identifier","named":true},
                {"type":"+","named":false}
            ]"#,
        )
        .unwrap();

        assert_eq!(node_types.is_named("identifier"), Some(true));
        assert_eq!(node_types.is_named("+"), Some(false));
        assert_eq!(node_types.is_named("missing"), None);
    }

    #[test]
    fn keeps_first_duplicate_node_type() {
        let node_types = NodeTypes::from_str(
            r#"[
                {"type":"identifier","named":true},
                {"type":"identifier","named":false}
            ]"#,
        )
        .unwrap();

        assert_eq!(node_types.is_named("identifier"), Some(true));
    }
}
