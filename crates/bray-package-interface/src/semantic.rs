mod codec;
mod interning;
mod model;
mod validation;

pub(crate) use codec::encode_validated_semantic_facts;
pub use codec::{EncodedSemanticSection, decode_semantic_facts, encode_semantic_facts};
pub use interning::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedConstraintFact,
    ImportedDeclarationTemplateFact, ImportedImplementationFact, ImportedSemanticFact,
    ImportedSemanticFacts, ImportedSourceProvenance, InterfaceSemanticInternError,
    InterfaceSymbolResolver,
};
pub use model::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceCallablePhaseBehavior, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
    InterfaceCheckedTemplateExecution, InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary, InterfaceCoherenceRecord,
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceConstraint, InterfaceDeclarationTemplate, InterfaceDependencyContract,
    InterfaceDependencyContractId, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceGenericBinding,
    InterfaceGenericSubstitution, InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceImplementationRecord,
    InterfaceImplementationReference, InterfacePredicateSummary, InterfaceSemanticFactEntry,
    InterfaceSemanticFactKind, InterfaceSemanticFacts, InterfaceSourceProvenance,
    InterfaceSupportEntity, InterfaceSupportImplementation, InterfaceTargetFactDependency,
    InterfaceTemplateReference, InterfaceTraitApplication, InterfaceTraitApplicationId,
    InterfaceTrustedCapabilityRequirement, InterfaceType, InterfaceTypeId,
};
