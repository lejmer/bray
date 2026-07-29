mod core;
mod validation;

pub use core::{CodegenMappings, CodegenMappingsBuildError};

#[cfg(any(test, feature = "test-support"))]
pub(crate) use validation::{demanded_debug_sources, demanded_types};
