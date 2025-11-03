/// Language-agnostic visibility levels for declarations.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Visibility {
    /// No explicit visibility information.
    Unspecified,
    /// Publicly exported from the declaring module/object.
    Public,
    /// Limited to the declaring scope/block.
    Private,
    /// Restricted to a named scope (e.g. `crate`, `module::submodule`).
    Restricted { scope: String },
}

impl Visibility {
    pub fn restricted(scope: impl Into<String>) -> Self {
        Visibility::Restricted {
            scope: scope.into(),
        }
    }
}
