use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::types::TypeExpr;
use crate::visibility::Visibility;

/// Descriptor for enumeration-like constructs across languages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDescriptor {
    pub origin: DescriptorOrigin,
    pub name: String,
    pub fqn: Option<String>,
    pub visibility: Visibility,
    pub generics: Option<String>,
    pub variants: Vec<EnumVariant>,
    pub extras: DescriptorExtras,
}

impl EnumDescriptor {
    pub fn new(origin: DescriptorOrigin, name: impl Into<String>) -> Self {
        Self {
            origin,
            name: name.into(),
            fqn: None,
            visibility: Visibility::Unspecified,
            generics: None,
            variants: Vec::new(),
            extras: DescriptorExtras::default(),
        }
    }
}

/// Metadata for a single enum variant.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariant {
    pub name: String,
    pub kind: EnumVariantKind,
    pub fields: Vec<EnumVariantField>,
    pub extras: DescriptorExtras,
}

impl EnumVariant {
    pub fn new(name: impl Into<String>, kind: EnumVariantKind) -> Self {
        Self {
            name: name.into(),
            kind,
            fields: Vec::new(),
            extras: DescriptorExtras::default(),
        }
    }
}

/// Classification of variant shapes.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnumVariantKind {
    Unit,
    Tuple,
    Struct,
    Other,
}

/// Field metadata associated with tuple/struct variants.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumVariantField {
    pub name: Option<String>,
    pub type_annotation: Option<TypeExpr>,
    pub extras: DescriptorExtras,
}

impl EnumVariantField {
    pub fn new(name: Option<String>) -> Self {
        Self {
            name,
            type_annotation: None,
            extras: DescriptorExtras::default(),
        }
    }
}
