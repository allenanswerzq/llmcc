use crate::meta::LanguageKey;

/// Normalised representation of type annotations across languages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// A simple or qualified identifier with optional generic arguments.
    Path {
        segments: Vec<String>,
        generics: Vec<TypeExpr>,
    },
    /// Reference-style types (e.g. pointers, borrowed references).
    Reference {
        is_mut: bool,
        lifetime: Option<String>,
        inner: Box<TypeExpr>,
    },
    /// Tuple or record types expressed positionally.
    Tuple(Vec<TypeExpr>),
    /// Callable or function types.
    Callable {
        parameters: Vec<TypeExpr>,
        result: Option<Box<TypeExpr>>,
    },
    /// `impl Trait` style opaque bounds.
    ImplTrait { bounds: String },
    /// Type information provided verbatim for languages without structured parsing support.
    Opaque { language: LanguageKey, repr: String },
    /// Fallback bucket for anything not yet modelled.
    Unknown(String),
}

impl TypeExpr {
    pub fn path_segments(&self) -> Option<&[String]> {
        match self {
            TypeExpr::Path { segments, .. } => Some(segments),
            _ => None,
        }
    }

    pub fn generics(&self) -> Option<&[TypeExpr]> {
        match self {
            TypeExpr::Path { generics, .. } => Some(generics),
            _ => None,
        }
    }

    pub fn as_reference(&self) -> Option<(&TypeExpr, bool)> {
        match self {
            TypeExpr::Reference { inner, is_mut, .. } => Some((inner, *is_mut)),
            _ => None,
        }
    }

    pub fn tuple_items(&self) -> Option<&[TypeExpr]> {
        match self {
            TypeExpr::Tuple(items) => Some(items),
            _ => None,
        }
    }

    pub fn callable_signature(&self) -> Option<(&[TypeExpr], Option<&TypeExpr>)> {
        match self {
            TypeExpr::Callable { parameters, result } => Some((parameters, result.as_deref())),
            _ => None,
        }
    }

    pub fn opaque(language: LanguageKey, repr: impl Into<String>) -> Self {
        TypeExpr::Opaque {
            language,
            repr: repr.into(),
        }
    }
}
