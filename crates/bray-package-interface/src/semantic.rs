mod codec;
mod interning;
mod model;
mod validation;

pub use codec::{EncodedSemanticSection, decode_semantic_facts, encode_semantic_facts};
pub use interning::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedCoherenceFact,
    ImportedConstraintFact, ImportedImplementationFact, ImportedSemanticFacts,
    ImportedSourceProvenance, ImportedTargetFactDependency, InterfaceSemanticInternError,
    InterfaceSymbolResolver,
};
pub use model::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceCoherenceRecord, InterfaceConstantProjection, InterfaceConstantTerm,
    InterfaceConstantTermId, InterfaceConstantValue, InterfaceConstantValueId,
    InterfaceConstantValueKind, InterfaceConstraint, InterfaceDependencyContract,
    InterfaceDependencyContractId, InterfaceDependencyGuard, InterfaceDependencyProjection,
    InterfaceDependencyRequirement, InterfaceDependencyRequirementKind,
    InterfaceDependencyRequirementValue, InterfaceDependencySubject,
    InterfaceDependencySubjectRoot, InterfaceGenericArgument, InterfaceGenericBinding,
    InterfaceGenericSubstitution, InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceImplementationRecord, InterfacePredicateSummary,
    InterfaceSemanticFactEntry, InterfaceSemanticFactKind, InterfaceSemanticFacts,
    InterfaceSourceProvenance, InterfaceTargetFactDependency, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceType, InterfaceTypeId,
};
