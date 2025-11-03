use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::types::TypeExpr;

/// Captured metadata for a dynamic call/expression.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallDescriptor {
    pub origin: DescriptorOrigin,
    pub enclosing: Option<String>,
    pub target: CallTarget,
    pub arguments: Vec<CallArgument>,
    pub extras: DescriptorExtras,
}

impl CallDescriptor {
    pub fn new(origin: DescriptorOrigin, target: CallTarget) -> Self {
        Self {
            origin,
            enclosing: None,
            target,
            arguments: Vec::new(),
            extras: DescriptorExtras::default(),
        }
    }
}

/// The shape of the entity being invoked.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTarget {
    Symbol(CallSymbol),
    Chain(CallChain),
    Dynamic { repr: String },
}

/// A symbol-style call (functions, free-standing methods, constructors).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSymbol {
    pub qualifiers: Vec<String>,
    pub name: String,
    pub kind: CallKind,
    pub type_arguments: Vec<TypeExpr>,
}

impl CallSymbol {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            qualifiers: Vec::new(),
            name: name.into(),
            kind: CallKind::Function,
            type_arguments: Vec::new(),
        }
    }
}

/// Chain of method calls (e.g. `foo.bar().baz()`).
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallChain {
    pub root: String,
    pub segments: Vec<CallSegment>,
}

impl CallChain {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            segments: Vec::new(),
        }
    }
}

/// One segment of a chained call.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallSegment {
    pub name: String,
    pub kind: CallKind,
    pub type_arguments: Vec<TypeExpr>,
    pub arguments: Vec<CallArgument>,
}

/// Broad classification for call targets.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallKind {
    Function,
    Method,
    Constructor,
    Macro,
    Unknown,
}

/// Represent a single argument in a call expression.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallArgument {
    pub name: Option<String>,
    pub value: String,
    pub type_hint: Option<TypeExpr>,
}

impl CallArgument {
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            name: None,
            value: value.into(),
            type_hint: None,
        }
    }
}
