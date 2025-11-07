use crate::meta::LanguageKey;

/// Normalised representation of type annotations across languages.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExpr {
    /// A simple or qualified identifier with optional generic arguments.
    ///
    /// ```text
    /// # Rust
    /// Vec<String>
    /// # Python (normalized)
    /// typing.List[int]
    /// ```
    /// becomes `TypeExpr::Path { parts: ["Vec"], generics: [TypeExpr::Path { parts: ["String"], .. }] }`.
    Path {
        parts: Vec<String>,
        generics: Vec<TypeExpr>,
    },
    /// Reference-style types (e.g. pointers, borrowed references).
    ///
    /// ```text
    /// &mut T
    /// ```
    /// becomes `TypeExpr::Reference { is_mut: true, lifetime: None, inner: Box::new(TypeExpr::Path { parts: ["T"], .. }) }`.
    Reference {
        is_mut: bool,
        lifetime: Option<String>,
        inner: Box<TypeExpr>,
    },
    /// Tuple or record types expressed positionally.
    ///
    /// ```text
    /// (usize, String)
    /// ```
    /// becomes `TypeExpr::Tuple([TypeExpr::Path { parts: ["usize"], .. }, TypeExpr::Path { parts: ["String"], .. }])`.
    Tuple(Vec<TypeExpr>),
    /// Callable or function types.
    ///
    /// ```text
    /// (i32, i32) -> i32
    /// ```
    /// becomes `TypeExpr::Callable { parameters: [TypeExpr::Path { parts: ["i32"], .. }, TypeExpr::Path { parts: ["i32"], .. }], result: Some(Box::new(TypeExpr::Path { parts: ["i32"], .. })) }`.
    Callable {
        parameters: Vec<TypeExpr>,
        result: Option<Box<TypeExpr>>,
    },
    /// `impl Trait` style opaque bounds.
    ///
    /// ```text
    /// impl Display + Debug
    /// ```
    /// becomes `TypeExpr::ImplTrait { bounds: "Display + Debug".into() }`.
    ImplTrait { bounds: String },
    /// Type information provided verbatim for languages without structured parsing support.
    ///
    /// ```text
    /// language = python, repr = "Callable[[int], str]"
    /// ```
    /// becomes `TypeExpr::Opaque { language, repr }`.
    Opaque { language: LanguageKey, repr: String },
    /// Fallback bucket for anything not yet modelled.
    ///
    /// ```text
    /// "unknown raw type"
    /// ```
    /// becomes `TypeExpr::Unknown("unknown raw type".into())`.
    Unknown(String),
}

impl TypeExpr {
    pub fn path_segments(&self) -> Option<&[String]> {
        match self {
            TypeExpr::Path { parts, .. } => Some(parts),
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
