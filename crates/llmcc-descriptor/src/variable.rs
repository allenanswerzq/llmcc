use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::types::TypeExpr;
use crate::visibility::Visibility;

/// Description of a variable/binding declaration.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableDescriptor {
    pub origin: DescriptorOrigin,
    pub name: String,
    pub fqn: Option<String>,
    pub visibility: Visibility,
    pub kind: VariableKind,
    pub scope: VariableScope,
    pub is_mutable: Option<bool>,
    pub type_annotation: Option<TypeExpr>,
    pub value_repr: Option<String>,
    pub extra_binding_names: Option<Vec<String>>,
    pub extras: DescriptorExtras,
}

impl VariableDescriptor {
    pub fn new(origin: DescriptorOrigin, name: impl Into<String>) -> Self {
        Self {
            origin,
            name: name.into(),
            fqn: None,
            visibility: Visibility::Unspecified,
            kind: VariableKind::Binding,
            scope: VariableScope::Unknown,
            is_mutable: None,
            type_annotation: None,
            value_repr: None,
            extra_binding_names: None,
            extras: DescriptorExtras::default(),
        }
    }
}

/// High level classification for bindings.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariableKind {
    Binding,
    Constant,
    Static,
    Field,
    ClassAttribute,
    Global,
    Parameter,
    Destructured,
    Other(String),
}

/// Scope information for a binding.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariableScope {
    Unknown,
    Global,
    Module,
    Class,
    Function,
    Block,
}
