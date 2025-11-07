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
///
/// # Examples
///
/// a direct function call where the function like: `crate::math::sqrt`:
/// a fluent-style chain like `foo.bar().baz::<T>()`:
/// a raw textual representation when the callee cannot be resolved:
/// ```rust
/// use llmcc_descriptor::CallTarget;
///
/// let dynamic = CallTarget::Dynamic {
///     repr: "some_macro!(expr)".into(),
/// };
/// assert!(matches!(dynamic, CallTarget::Dynamic { .. }));
/// ```
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallTarget {
    Symbol(CallSymbol),
    Chain(CallChain),
    Dynamic { repr: String },
}

/// Representation of the starting point for a fluent call chain.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallChainRoot {
    /// Raw expression used as the receiver (e.g. `value` or `self.items()[0]`).
    Expr(String),
    /// An invocation that feeds its result into the next segment (e.g. `foo()`).
    Invocation(CallInvocation),
}

impl From<String> for CallChainRoot {
    fn from(value: String) -> Self {
        CallChainRoot::Expr(value)
    }
}

impl From<&str> for CallChainRoot {
    fn from(value: &str) -> Self {
        CallChainRoot::Expr(value.to_string())
    }
}

impl From<CallInvocation> for CallChainRoot {
    fn from(value: CallInvocation) -> Self {
        CallChainRoot::Invocation(value)
    }
}

/// Captures an invocation inside a call chain.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallInvocation {
    pub target: Box<CallTarget>,
    pub type_arguments: Vec<TypeExpr>,
    pub arguments: Vec<CallArgument>,
}

impl CallInvocation {
    pub fn new(
        target: CallTarget,
        type_arguments: Vec<TypeExpr>,
        arguments: Vec<CallArgument>,
    ) -> Self {
        Self {
            target: Box::new(target),
            type_arguments,
            arguments,
        }
    }
}

/// A resolved symbol-style call (free functions, inherent methods, constructors).
///
/// * `qualifiers` keeps the namespace path (e.g. `vec!["crate", "math"]`).
/// * `name` is the final identifier (`"sqrt"`).
/// * `kind` distinguishes between `CallKind::Function`, `CallKind::Method`, etc.
/// * `type_arguments` carries any generic arguments (`Vec<TypeExpr>` for `foo::<T>()`).
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
    pub root: CallChainRoot,
    pub parts: Vec<CallSegment>,
}

impl CallChain {
    pub fn new(root: impl Into<CallChainRoot>) -> Self {
        Self {
            root: root.into(),
            parts: Vec::new(),
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
