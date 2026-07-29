mod constant;
mod demand;
mod debug;
mod model;
mod reference;
mod symbol;
mod ty;

pub use constant::{CodegenConstantMapping, CodegenConstantTermMapping};
pub use debug::{CodegenDebugLocation, CodegenSourceFile};
pub use model::{CodegenMappings, CodegenMappingsBuildError};
pub use reference::{
    CodegenCallableMapping, CodegenOperationMapping, CodegenTerminatorMapping,
};
pub use symbol::{CodegenSymbolKey, CodegenSymbolMapping};
pub use ty::{
    CodegenCallableSignature, CodegenFieldLayout, CodegenIndirectParameterKind,
    CodegenIntegerExtension, CodegenParameterMapping, CodegenResultMapping, CodegenTypeKind,
    CodegenTypeMapping, CodegenUnionVariantLayout, CodegenValueAttribute,
};

#[cfg(any(test, feature = "test-support"))]
pub(crate) use model::{demanded_debug_sources, demanded_types};
