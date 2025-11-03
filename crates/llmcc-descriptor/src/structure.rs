use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::types::TypeExpr;
use crate::visibility::Visibility;

/// Descriptor for structured types (structs, records, data classes).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDescriptor {
    pub origin: DescriptorOrigin,
    pub name: String,
    pub fqn: Option<String>,
    pub visibility: Visibility,
    pub kind: StructKind,
    pub generics: Option<String>,
    pub fields: Vec<StructField>,
    pub extras: DescriptorExtras,
}

impl StructDescriptor {
    pub fn new(origin: DescriptorOrigin, name: impl Into<String>) -> Self {
        Self {
            origin,
            name: name.into(),
            fqn: None,
            visibility: Visibility::Unspecified,
            kind: StructKind::Record,
            generics: None,
            fields: Vec::new(),
            extras: DescriptorExtras::default(),
        }
    }
}

/// Broad struct classifications.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StructKind {
    Record,
    Tuple,
    Unit,
    Class,
    Other,
}

/// Field metadata for structured types.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructField {
    pub name: Option<String>,
    pub type_annotation: Option<TypeExpr>,
    pub extras: DescriptorExtras,
}

impl StructField {
    pub fn new(name: Option<String>) -> Self {
        Self {
            name,
            type_annotation: None,
            extras: DescriptorExtras::default(),
        }
    }
}
