mod application;
mod bundle;
mod constant;
mod declaration;
mod dependency;
mod id;
mod record;
mod support;
mod template;
mod ty;

pub use bray_package_interface_model::InterfaceSemanticRecordKind;

pub use application::{
    InterfaceCallableInstance, InterfaceGenericArgument, InterfaceGenericBinding,
    InterfaceGenericSubstitution, InterfaceImplementationInstance, InterfaceTraitApplication,
};
pub use bundle::InterfaceSemantics;
pub use constant::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantValue,
    InterfaceConstantValueKind,
};
pub use declaration::{
    InterfaceCallableParameterDefault, InterfaceCallableReceiver, InterfaceCallableSignature,
    InterfaceDeclaredType, InterfaceGenericDeclaration, InterfacePredicateDefinition,
    InterfacePredicateDefinitionState, InterfaceStorageMember, InterfaceStorageShape,
    InterfaceTypeRepresentation, InterfaceUnionStorageVariant, InterfaceUnionTag,
};
pub use dependency::{
    InterfaceDependencyContract, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot,
};
pub use id::{
    InterfaceCallableInstanceId, InterfaceCheckedTemplateId, InterfaceConstantTermId,
    InterfaceConstantValueId, InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceTraitApplicationId, InterfaceTypeId,
};
pub use record::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableContractClauseValue, InterfaceCallablePhaseBehavior, InterfaceCoherenceRecord,
    InterfaceConstraint, InterfaceConstraintKind, InterfaceImplementationRecord,
    InterfacePredicateSummary, InterfaceRuntimeRequirement, InterfaceSemanticRecord,
    InterfaceSourceProvenance, InterfaceTargetPropertyDependency,
    InterfaceTrustedCapabilityRequirement,
};
pub use support::{
    InterfaceImplementationReference, InterfaceSupportEntity, InterfaceSupportImplementation,
    InterfaceTemplateReference,
};
pub use template::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary,
    InterfaceDeclarationTemplate,
};
pub use ty::{InterfaceCallableParameter, InterfaceType};
