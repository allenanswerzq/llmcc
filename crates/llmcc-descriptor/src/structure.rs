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
    pub base_types: Vec<TypeExpr>,
    pub methods: Vec<String>,
    pub fields: Vec<StructField>,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    /// Optional fully-qualified name of the type targeted by an `impl` block.
    pub impl_target_fqn: Option<String>,
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
            base_types: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            decorators: Vec::new(),
            docstring: None,
            impl_target_fqn: None,
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
    pub fn new(name: impl IntoStructFieldName) -> Self {
        Self {
            name: name.into_struct_field_name(),
            type_annotation: None,
            extras: DescriptorExtras::default(),
        }
    }

    pub fn with_name(name: impl Into<String>) -> Self {
        Self::new(Some(name.into()))
    }

    pub fn unnamed() -> Self {
        Self::new(Option::<String>::None)
    }

    pub fn name_str(&self) -> Option<&str> {
        self.name.as_deref()
    }
}

pub trait IntoStructFieldName {
    fn into_struct_field_name(self) -> Option<String>;
}

impl IntoStructFieldName for Option<String> {
    fn into_struct_field_name(self) -> Option<String> {
        self
    }
}

impl IntoStructFieldName for Option<&str> {
    fn into_struct_field_name(self) -> Option<String> {
        self.map(|s| s.to_string())
    }
}

impl IntoStructFieldName for String {
    fn into_struct_field_name(self) -> Option<String> {
        Some(self)
    }
}

impl IntoStructFieldName for &str {
    fn into_struct_field_name(self) -> Option<String> {
        Some(self.to_string())
    }
}
