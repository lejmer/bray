mod core;
mod validation;

pub use core::{
    CodegenMappings, CodegenMappingsBuildError, demanded_callable_references,
    demanded_callable_references_for_mir,
};
pub use validation::{
    demanded_debug_sources, demanded_runtime_references, demanded_types,
};
