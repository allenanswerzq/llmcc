use crate::meta::{DescriptorExtras, DescriptorOrigin};
use crate::visibility::Visibility;

/// Descriptor capturing metadata for Rust modules and similar namespace constructs.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleDescriptor {
    pub origin: DescriptorOrigin,
    pub name: String,
    pub fqn: Option<String>,
    pub visibility: Visibility,
    pub is_inline: bool,
    pub docstring: Option<String>,
    pub extras: DescriptorExtras,
}

impl ModuleDescriptor {
    pub fn new(origin: DescriptorOrigin, name: impl Into<String>) -> Self {
        Self {
            origin,
            name: name.into(),
            fqn: None,
            visibility: Visibility::Unspecified,
            is_inline: false,
            docstring: None,
            extras: DescriptorExtras::default(),
        }
    }
}
