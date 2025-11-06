#![forbid(unsafe_code)]

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub mod builder;
pub mod call;
pub mod class;
pub mod enumeration;
pub mod function;
pub mod import;
pub mod meta;
pub mod module;
pub mod structure;
pub mod types;
pub mod variable;
pub mod visibility;

pub use builder::*;
pub use call::*;
pub use class::*;
pub use enumeration::*;
pub use function::*;
pub use import::*;
pub use meta::*;
pub use module::*;
pub use structure::*;
pub use types::*;
pub use variable::*;
pub use visibility::*;
