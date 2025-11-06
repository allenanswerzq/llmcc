use crate::meta::DescriptorOrigin;
use crate::types::TyExpr;

/// Descriptor capturing metadata for Rust `impl` blocks.
///
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplDescriptor {
    pub origin: DescriptorOrigin,
    pub target_ty: TyExpr,
    pub trait_ty: Option<TyExpr>,
}

impl ImplDescriptor {
    pub fn new(origin: DescriptorOrigin, target_ty: TyExpr) -> Self {
        Self {
            origin,
            target_ty,
            trait_ty: None,
        }
    }
}
