mod codec;
mod interning;
mod model;
mod validation;

pub use codec::{EncodedSemanticSection, decode_semantic_facts, encode_semantic_facts};
pub(crate) use codec::{
    decode_inspection_records, decode_semantic_fact_graph, decode_template_payload,
    encode_template_payload, encode_validated_semantic_facts, validate_decode_allocation,
};
pub use interning::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedCallableParameterDefaultFact,
    ImportedCallableSignatureFact, ImportedConstraintFact, ImportedDeclarationTemplateFact,
    ImportedDeclaredTypeFact, ImportedGenericDeclarationFact, ImportedImplementationFact,
    ImportedPredicateDefinitionFact,
    ImportedRuntimeRequirement, ImportedSemanticFact, ImportedSemanticFacts,
    ImportedSourceProvenance, ImportedTargetFact, InterfaceSemanticInternError,
    InterfaceSymbolResolver,
};
pub use model::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableInstance, InterfaceCallableInstanceId, InterfaceCallableParameter,
    InterfaceCallableParameterDefault, InterfaceCallablePhaseBehavior, InterfaceCallableReceiver,
    InterfaceCallableSignature, InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior,
    InterfaceCheckedTemplateExecution, InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary, InterfaceCoherenceRecord,
    InterfaceConstantProjection, InterfaceConstantTerm, InterfaceConstantTermId,
    InterfaceConstantValue, InterfaceConstantValueId, InterfaceConstantValueKind,
    InterfaceConstraint, InterfaceConstraintKind, InterfaceDeclarationTemplate,
    InterfaceDeclaredType,
    InterfaceDependencyContract, InterfaceDependencyContractId, InterfaceDependencyGuard,
    InterfaceDependencyProjection, InterfaceDependencyRequirement,
    InterfaceDependencyRequirementKind, InterfaceDependencyRequirementValue,
    InterfaceDependencySubject, InterfaceDependencySubjectRoot, InterfaceGenericArgument,
    InterfaceGenericBinding, InterfaceGenericDeclaration, InterfaceGenericSubstitution,
    InterfaceGenericSubstitutionId, InterfaceImplementationInstance,
    InterfaceImplementationInstanceId, InterfaceImplementationRecord,
    InterfaceImplementationReference, InterfacePredicateDefinition,
    InterfacePredicateDefinitionState, InterfacePredicateSummary, InterfaceRuntimeRequirement,
    InterfaceSemanticFactEntry, InterfaceSemanticFactKind, InterfaceSemanticFacts,
    InterfaceSourceProvenance, InterfaceSupportEntity, InterfaceSupportImplementation,
    InterfaceTargetFactDependency, InterfaceTemplateReference, InterfaceTraitApplication,
    InterfaceTraitApplicationId, InterfaceTrustedCapabilityRequirement, InterfaceType,
    InterfaceTypeId, InterfaceTypeRepresentation, InterfaceUnionTag,
};
pub(crate) use validation::validate_constraint_templates;
