mod application;
mod bundle;
mod constant;
mod dependency;
mod fact;
mod id;
mod support;
mod template;
mod ty;

pub use application::{
    InterfaceCallableInstance, InterfaceGenericArgument, InterfaceGenericBinding,
    InterfaceGenericSubstitution, InterfaceImplementationInstance, InterfaceTraitApplication,
};
pub use bundle::InterfaceSemanticFacts;
pub use constant::{
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantValue,
    InterfaceConstantValueKind,
};
pub use dependency::{
    InterfaceDependencyContract, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot,
};
pub use fact::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCoherenceRecord, InterfaceConstraint, InterfaceImplementationRecord,
    InterfacePredicateSummary, InterfaceSemanticFactEntry, InterfaceSemanticFactKind,
    InterfaceSourceProvenance, InterfaceTargetFactDependency,
};
pub use id::{
    InterfaceCallableInstanceId, InterfaceCheckedTemplateId, InterfaceConstantTermId,
    InterfaceConstantValueId, InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceTraitApplicationId, InterfaceTypeId,
};
pub use support::{
    InterfaceImplementationReference, InterfaceSupportEntity, InterfaceSupportImplementation,
    InterfaceTemplateReference,
};
pub use template::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary,
    InterfaceDeclarationTemplate,
};
pub use ty::{InterfaceCallableParameter, InterfaceType};
