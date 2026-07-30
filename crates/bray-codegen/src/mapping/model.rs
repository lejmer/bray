mod core;
mod validation;

pub use core::{CodegenMappings, CodegenMappingsBuildError};
pub use validation::demanded_runtime_references;

#[cfg(any(test, feature = "test-support"))]
pub(crate) use validation::{demanded_debug_sources, demanded_types};
