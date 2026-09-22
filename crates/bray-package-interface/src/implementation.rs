mod artifact;
mod artifact_decoding;
mod artifact_encoding;
mod codec;
mod executable;
mod family;
mod hash;
mod identity;
mod model;
mod payload;
mod specialization;

pub(crate) use codec::{invalid_value, map_wire_error};
pub(crate) use family::invalid_executable_template_family;

pub use artifact::{PackageImplementationArtifact, PackageNativeArtifactError};
pub use executable::{
    ExecutableTemplateDecodeError, ExecutableTemplateEncodeContext, ExecutableTemplateEncodeError,
    decode_executable_template, encode_executable_template, encode_pre_specialized_mir,
};
pub use identity::{
    CURRENT_TEMPLATE_SCHEMA_REVISION, ImplementationTemplateSchemaRevision,
    PackageImplementationConfiguration, PackageImplementationIdentity,
    PackageImplementationTargetProperties, PackageImplementationTargetProperty,
    PackageImplementationTargetPropertyValue,
};
pub use model::{
    InterfaceConstantCallableBody, InterfaceExecutableTemplate, InterfaceNativeBinding, InterfaceNativeBoundary,
    InterfaceNativeBoundaryKind, PackageImplementationArtifactBuildError,
};
pub use specialization::{
    CURRENT_MIR_SCHEMA_REVISION, ImplementationExternalSymbolIdentity,
    ImplementationMirSchemaRevision, ImplementationSpecializationArgument,
    ImplementationSpecializationArgumentKind, ImplementationSpecializationWitness,
    InterfacePreSpecializedMir, PackageImplementationSpecializationKey,
    PreSpecializedMirDecodeError,
};
