use std::collections::BTreeMap;

/// Canonical language identifiers used across descriptors.
pub type LanguageKey = &'static str;

/// Built-in language keys for the primary llmcc frontends.
pub const LANGUAGE_RUST: LanguageKey = "rust";
pub const LANGUAGE_PYTHON: LanguageKey = "python";

/// Flexible identifier that can map to language-specific node ids.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DescriptorId {
    /// Numeric identifiers (e.g. Tree-Sitter or HIR node ids).
    U64(u64),
    /// String-based identifiers when numbers are not available.
    Text(String),
}

impl From<u64> for DescriptorId {
    fn from(value: u64) -> Self {
        DescriptorId::U64(value)
    }
}

impl From<&str> for DescriptorId {
    fn from(value: &str) -> Self {
        DescriptorId::Text(value.to_string())
    }
}

impl From<String> for DescriptorId {
    fn from(value: String) -> Self {
        DescriptorId::Text(value)
    }
}

/// Byte span inside a source file.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: u32,
    pub end: u32,
}

impl SourceSpan {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
}

/// Location metadata associated with a descriptor.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: Option<String>,
    pub span: Option<SourceSpan>,
}

impl SourceLocation {
    pub fn new(file: Option<String>, span: Option<SourceSpan>) -> Self {
        Self { file, span }
    }
}

/// Shared origin data carried by every descriptor instance.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DescriptorOrigin {
    pub language: LanguageKey,
    pub id: Option<DescriptorId>,
    pub location: Option<SourceLocation>,
}

impl DescriptorOrigin {
    pub fn new(language: LanguageKey) -> Self {
        Self {
            language,
            id: None,
            location: None,
        }
    }

    pub fn with_id(mut self, id: impl Into<DescriptorId>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn with_location(mut self, location: SourceLocation) -> Self {
        self.location = Some(location);
        self
    }
}

/// Extensible metadata bag for language-specific add-ons.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub type DescriptorExtras = BTreeMap<String, String>;
