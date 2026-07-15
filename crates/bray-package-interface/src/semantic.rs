mod codec;
mod interning;
mod model;
mod validation;

pub use codec::{EncodedSemanticSection, decode_semantic_facts, encode_semantic_facts};
pub use interning::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedCoherenceFact,
    ImportedConstraintFact, ImportedDeclarationTemplateFact, ImportedImplementationFact,
    ImportedSemanticFact, ImportedSemanticFacts, ImportedSourceProvenance,
    ImportedTargetFactDependency, InterfaceSemanticInternError, InterfaceSymbolResolver,
};
pub use model::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateId,
    InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
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
    InterfaceType, InterfaceTypeId,
};
