//! Tree-sitter node type handling and token generation.

pub mod config;
mod node_types;

use std::fmt::Write;
use std::path::Path;

use anyhow::Result;
use tree_sitter::Language;

use config::TokenConfig;
use node_types::NodeTypes;

/// A token entry mapping tree-sitter node to HIR kind.
#[derive(Debug, Clone)]
pub struct TokenEntry {
    pub name: String,
    pub kind_id: u16,
    pub repr: String,
    pub hir_kind: String,
    pub block_kind: Option<String>,
}

/// Collection of token entries by category.
#[derive(Debug, Clone, Default)]
pub struct TokenSet {
    pub text_tokens: Vec<TokenEntry>,
    pub node_tokens: Vec<TokenEntry>,
    pub field_tokens: Vec<TokenEntry>,
}

impl TokenSet {
    pub fn is_empty(&self) -> bool {
        self.text_tokens.is_empty() && self.node_tokens.is_empty() && self.field_tokens.is_empty()
    }

    /// Render the token set as a `define_lang!` macro invocation.
    pub fn render(&self, language_ident: &str) -> String {
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

/// Generate tokens from file paths.
pub fn generate_tokens(
    language_ident: &str,
    language: Language,
    node_types_path: &Path,
    config_path: &Path,
) -> Result<String> {
    let config = TokenConfig::from_path(config_path)?;
    let node_types = NodeTypes::from_path(node_types_path)?;
    let set = generate(language, &node_types, &config)?;
    Ok(set.render(language_ident))
}

/// Generate tokens from embedded node-types JSON string.
pub fn generate_tokens_from_str(
    language_ident: &str,
    language: Language,
    node_types_json: &str,
    config_path: &Path,
) -> Result<String> {
    let config = TokenConfig::from_path(config_path)?;
    let node_types = NodeTypes::from_str(node_types_json)?;
    let set = generate(language, &node_types, &config)?;
    Ok(set.render(language_ident))
}

/// Core generation logic.
pub fn generate(
    language: Language,
    node_types: &NodeTypes,
    config: &TokenConfig,
) -> Result<TokenSet> {
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

pub(crate) fn format_hir(kind: &str) -> String {
    if kind.starts_with("HirKind::") {
        kind.to_string()
    } else {
        format!("HirKind::{kind}")
    }
}

pub(crate) fn format_block(kind: &str) -> String {
    if kind.starts_with("BlockKind::") {
        kind.to_string().replacen("BlockKind::", "", 1)
    } else {
        kind.to_string()
    }
}

pub(crate) fn resolve_kind_id(language: Language, name: &str, named: bool) -> Result<u16> {
    let id = language.id_for_node_kind(name, named);
    if id == u16::MAX {
        anyhow::bail!("node kind '{name}' (named={named}) not found");
    }
    Ok(id)
}

pub(crate) fn resolve_field_id(language: Language, field_name: &str) -> Result<u16> {
    let Some(field_id) = language.field_id_for_name(field_name.as_bytes()) else {
        anyhow::bail!("field '{field_name}' not found");
    };
    Ok(field_id.get())
}
