mod constant;
mod debug;
mod demand;
mod incident;
mod model;
mod native_static;
mod product_host;
mod reference;
mod static_storage;
mod symbol;
mod ty;

pub use constant::{CodegenConstantMapping, CodegenConstantTermMapping};
pub use debug::{CodegenDebugLocation, CodegenSourceFile};
pub use demand::{ConstantDemands, child_constants, demanded_constant_terms, demanded_constants};
pub use incident::CodegenCleanupIncident;
pub use model::{
    CodegenMappings, CodegenMappingsBuildError, DemandedCallableInstance,
    demanded_callable_instance_for_call, demanded_callable_instances,
    demanded_callable_instances_for_mir, demanded_callable_references,
    demanded_callable_references_for_mir, demanded_debug_sources, demanded_runtime_references,
    demanded_runtime_references_for_mir, demanded_types, mapped_runtime_references,
};
pub use native_static::CodegenNativeStaticMapping;
pub use product_host::{
    CodegenProductHostMapping, CodegenProductHostStatic, static_host_section_name,
};
pub use reference::{
    CodegenCallSite, CodegenCallableMapping, CodegenCallableTarget, CodegenHelperMapping,
    CodegenOperationMapping, CodegenTerminatorMapping, IntrinsicCall,
};
pub use static_storage::{
    CodegenStaticFinalization, CodegenStaticInstanceKey, CodegenStaticRelocation,
    CodegenStaticStorageMapping, CodegenStaticWitness,
};
pub use symbol::{
    CLEANUP_RUNTIME_ROLES, CodegenNativeEntryMapping, CodegenSymbolKey, CodegenSymbolMapping,
    FOREIGN_CALLBACK_RUNTIME_ROLES,
};
pub use ty::{
    CodegenCallableSignature, CodegenFieldLayout, CodegenIndirectParameterKind,
    CodegenInstanceTypeMapping, CodegenIntegerExtension, CodegenParameterMapping,
    CodegenResultMapping, CodegenTypeBehavior, CodegenTypeKind, CodegenTypeMapping,
    CodegenUnionVariantLayout, CodegenValueAttribute,
};
