use crate::meta::DescriptorOrigin;
use crate::types::TypeExpr;

/// Descriptor capturing metadata for Rust `impl` blocks.
///
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplDescriptor {
    pub origin: DescriptorOrigin,
    pub target_ty: TypeExpr,
    pub trait_ty: Option<TypeExpr>,
}

impl ImplDescriptor {
    pub fn new(origin: DescriptorOrigin, target_ty: TypeExpr) -> Self {
        Self {
            origin,
            target_ty,
            trait_ty: None,
        }
    }
}
