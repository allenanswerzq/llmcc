use crate::meta::LanguageKey;

/// Leading qualifier for a language-agnostic path expression.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathQualifier {
    /// No explicit qualifier; interpret parts relative to the surrounding scope.
    Relative { parts: Vec<String> },
    /// Path anchored at the compilation unit root (Rust `crate::` equivalent).
    Crate { parts: Vec<String> },
    /// Path that begins at the language's absolute root (Rust leading `::`, C# `global::`).
    Absolute { parts: Vec<String> },
    /// Path referring to the current self type (Rust `self::` or `Self::`).
    SelfType { parts: Vec<String> },
    /// Path walking up from the current scope (Rust `super::`).
    Super { levels: u32, parts: Vec<String> },
    /// Fallback for qualifiers not yet modelled; stored as raw text for round-tripping.
    Raw { raw: String, parts: Vec<String> },
}

impl PathQualifier {
    pub fn relative(parts: Vec<String>) -> Self {
        Self::Relative { parts }
    }

    pub fn crate_root(parts: Vec<String>) -> Self {
        Self::Crate { parts }
    }

    pub fn absolute(parts: Vec<String>) -> Self {
        Self::Absolute { parts }
    }

    pub fn self_type(parts: Vec<String>) -> Self {
        Self::SelfType { parts }
    }

    pub fn super_level(levels: u32) -> Self {
        Self::Super {
            levels,
            parts: Vec::new(),
        }
    }

    pub fn super_with_segments(levels: u32, parts: Vec<String>) -> Self {
        Self::Super { levels, parts }
    }

    pub fn raw(raw: impl Into<String>, parts: Vec<String>) -> Self {
        Self::Raw {
            raw: raw.into(),
            parts,
        }
    }

    pub fn parts(&self) -> &[String] {
        match self {
            PathQualifier::Relative { parts }
            | PathQualifier::Crate { parts }
            | PathQualifier::Absolute { parts }
            | PathQualifier::SelfType { parts }
            | PathQualifier::Super { parts, .. }
            | PathQualifier::Raw { parts, .. } => parts,
        }
    }

    pub fn segments_mut(&mut self) -> &mut Vec<String> {
        match self {
            PathQualifier::Relative { parts }
            | PathQualifier::Crate { parts }
            | PathQualifier::Absolute { parts }
            | PathQualifier::SelfType { parts }
            | PathQualifier::Super { parts, .. }
            | PathQualifier::Raw { parts, .. } => parts,
        }
    }

    pub fn into_segments(self) -> Vec<String> {
        match self {
            PathQualifier::Relative { parts }
            | PathQualifier::Crate { parts }
            | PathQualifier::Absolute { parts }
            | PathQualifier::SelfType { parts }
            | PathQualifier::Super { parts, .. }
            | PathQualifier::Raw { parts, .. } => parts,
        }
    }

    /// Prefix parts implied by the qualifier kind (e.g. `crate`, `self`, repeated `super`).
    pub fn prefix_segments(&self) -> Vec<String> {
        match self {
            PathQualifier::Relative { .. } | PathQualifier::Absolute { .. } => Vec::new(),
            PathQualifier::Crate { .. } => vec!["crate".to_string()],
            PathQualifier::SelfType { .. } => vec!["self".to_string()],
            PathQualifier::Super { levels, .. } => {
                let mut parts = Vec::with_capacity(*levels as usize);
                for _ in 0..*levels {
                    parts.push("super".to_string());
                }
                parts
            }
            PathQualifier::Raw { raw, .. } => vec![raw.clone()],
        }
    }
}

impl Default for PathQualifier {
    fn default() -> Self {
        PathQualifier::Relative {
            parts: Vec::new(),
        }
    }
}

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
    /// becomes a `TypeExpr::Path` whose qualifier records the parts `"Vec"`
    /// and whose single generic parameter is a path with parts `"String"`.
    Path {
        qualifier: PathQualifier,
        generics: Vec<TypeExpr>,
    },
    /// Reference-style types (e.g. pointers, borrowed references).
    ///
    /// ```text
    /// &mut T
    /// ```
    /// becomes `TypeExpr::Reference { is_mut: true, lifetime: None, inner: Box::new(TypeExpr::Path { .. }) }`
    /// where the inner path qualifier carries the parts `"T"`.
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
    /// becomes a `TypeExpr::Tuple` with items that are `Path` variants carrying the
    /// parts `"usize"` and `"String"` respectively.
    Tuple(Vec<TypeExpr>),
    /// Callable or function types.
    ///
    /// ```text
    /// (i32, i32) -> i32
    /// ```
    /// becomes a `TypeExpr::Callable` containing parameter paths with qualifier
    /// parts `"i32"` and a result path with the same parts.
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
            TypeExpr::Path { qualifier, .. } => Some(qualifier.parts()),
            _ => None,
        }
    }

    pub fn generics(&self) -> Option<&[TypeExpr]> {
        match self {
            TypeExpr::Path { generics, .. } => Some(generics),
            _ => None,
        }
    }

    pub fn path_qualifier(&self) -> Option<&PathQualifier> {
        match self {
            TypeExpr::Path { qualifier, .. } => Some(qualifier),
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
