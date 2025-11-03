use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::types::TypeExpr;
use crate::visibility::Visibility;

/// Descriptor for a function-like declaration across languages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionDescriptor {
    pub origin: DescriptorOrigin,
    pub name: String,
    pub fqn: Option<String>,
    pub visibility: Visibility,
    pub qualifiers: FunctionQualifiers,
    pub generics: Option<String>,
    pub where_clause: Option<String>,
    pub parameters: Vec<FunctionParameter>,
    pub return_type: Option<TypeExpr>,
    pub signature: Option<String>,
    pub decorators: Vec<String>,
    pub docstring: Option<String>,
    pub extras: DescriptorExtras,
}

impl FunctionDescriptor {
    pub fn new(origin: DescriptorOrigin, name: impl Into<String>) -> Self {
        Self {
            origin,
            name: name.into(),
            fqn: None,
            visibility: Visibility::Unspecified,
            qualifiers: FunctionQualifiers::default(),
            generics: None,
            where_clause: None,
            parameters: Vec::new(),
            return_type: None,
            signature: None,
            decorators: Vec::new(),
            docstring: None,
            extras: DescriptorExtras::default(),
        }
    }
}

/// Language-specific function modifiers captured in a uniform shape.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FunctionQualifiers {
    pub is_async: bool,
    pub is_const: bool,
    pub is_unsafe: bool,
    pub is_static: bool,
    pub is_generator: bool,
}

/// Normalised parameter descriptor supporting both typed and typeless languages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionParameter {
    pub name: Option<String>,
    pub pattern: Option<String>,
    pub kind: ParameterKind,
    pub type_hint: Option<TypeExpr>,
    pub default_value: Option<String>,
}

impl FunctionParameter {
    pub fn new(name: Option<String>) -> Self {
        Self {
            name,
            pattern: None,
            kind: ParameterKind::Positional,
            type_hint: None,
            default_value: None,
        }
    }
}

/// Broad classification for parameters across languages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterKind {
    Positional,
    Receiver,
    VariadicPositional,
    VariadicKeyword,
    KeywordOnly,
    Destructured,
    Unknown,
}
