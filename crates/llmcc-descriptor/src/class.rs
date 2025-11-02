use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::types::TypeExpr;
use crate::visibility::Visibility;

/// Descriptor used for class/object oriented declarations.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDescriptor {
    pub origin: DescriptorOrigin,
    pub name: String,
    pub fqn: Option<String>,
    pub visibility: Visibility,
    pub base_types: Vec<TypeExpr>,
    pub methods: Vec<String>,
    pub fields: Vec<ClassField>,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    pub extras: DescriptorExtras,
}

impl ClassDescriptor {
    pub fn new(origin: DescriptorOrigin, name: impl Into<String>) -> Self {
        Self {
            origin,
            name: name.into(),
            fqn: None,
            visibility: Visibility::Unspecified,
            base_types: Vec::new(),
            methods: Vec::new(),
            fields: Vec::new(),
            decorators: Vec::new(),
            docstring: None,
            extras: DescriptorExtras::default(),
        }
    }
}

/// Field descriptor attached to a class-like type.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassField {
    pub name: String,
    pub type_annotation: Option<TypeExpr>,
    pub extras: DescriptorExtras,
}

impl ClassField {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            type_annotation: None,
            extras: DescriptorExtras::default(),
        }
    }
}
