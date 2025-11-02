use crate::meta::{DescriptorExtras, DescriptorOrigin};

/// Metadata for import/use/include style statements.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportDescriptor {
    pub origin: DescriptorOrigin,
    pub source: String,
    pub alias: Option<String>,
    pub kind: ImportKind,
    pub extras: DescriptorExtras,
}

impl ImportDescriptor {
    pub fn new(origin: DescriptorOrigin, source: impl Into<String>) -> Self {
        Self {
            origin,
            source: source.into(),
            alias: None,
            kind: ImportKind::Module,
            extras: DescriptorExtras::default(),
        }
    }
}

/// Coarse classification for import statements.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    Module,
    Item,
    Wildcard,
    SideEffect,
    Unknown,
}
