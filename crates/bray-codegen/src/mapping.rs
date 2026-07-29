mod debug;
mod model;
mod symbol;
mod ty;

pub use debug::{CodegenDebugLocation, CodegenSourceFile};
pub use model::{CodegenMappings, CodegenMappingsBuildError};
pub use symbol::{CodegenSymbolKey, CodegenSymbolMapping};
pub use ty::{
    CodegenCallableSignature, CodegenFieldLayout, CodegenTypeKind, CodegenTypeMapping,
    CodegenUnionVariantLayout,
};

#[cfg(any(test, feature = "test-support"))]
pub(crate) use model::{demanded_debug_sources, demanded_types};
