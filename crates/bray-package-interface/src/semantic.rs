mod codec;
mod interning;
mod model;
mod validation;

pub(crate) use codec::{
    COMPLETE_FACT_SECTIONS, SemanticDecodeContext, decode_inspection_records,
    decode_selected_semantic_fact_graph, decode_semantic_fact_graph, decode_template_payload,
    encode_template_payload, encode_validated_semantic_facts, read_symbol_reference,
    selected_fact_sections, validate_decode_allocation, write_symbol_reference,
};
pub use codec::{EncodedSemanticSection, decode_semantic_facts, encode_semantic_facts};
pub use interning::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedCallableParameterDefaultFact,
    ImportedCallableSignatureFact, ImportedConstraintFact, ImportedDeclarationTemplateFact,
    ImportedDeclaredTypeFact, ImportedGenericDeclarationFact, ImportedImplementationFact,
    ImportedPredicateDefinitionFact, ImportedRuntimeRequirement, ImportedSemanticFact,
    ImportedSemanticFacts, ImportedSourceProvenance, ImportedTargetFact,
    InterfaceSemanticInternError, InterfaceSymbolResolver,
};
pub use model::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableContractClause,
    InterfaceCallableContractClauseValue, InterfaceCallableInstance, InterfaceCallableInstanceId,
    InterfaceCallableParameter, InterfaceCallableParameterDefault, InterfaceCallablePhaseBehavior,
    InterfaceCallableReceiver, InterfaceCallableSignature, InterfaceCheckedTemplate,
    InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind,
    InterfaceCheckedTemplateNode, InterfaceCheckedTemplateOperation,
    InterfaceCheckedTemplateTemporary, InterfaceCoherenceRecord, InterfaceConstantProjection,
    InterfaceConstantTerm, InterfaceConstantTermId, InterfaceConstantValue,
    InterfaceConstantValueId, InterfaceConstantValueKind, InterfaceConstraint,
    InterfaceConstraintKind, InterfaceDeclarationTemplate, InterfaceDeclaredType,
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
    InterfaceSourceProvenance, InterfaceStorageMember, InterfaceStorageShape,
    InterfaceSupportEntity, InterfaceSupportImplementation, InterfaceTargetFactDependency,
    InterfaceTemplateReference, InterfaceTraitApplication, InterfaceTraitApplicationId,
    InterfaceTrustedCapabilityRequirement, InterfaceType, InterfaceTypeId,
    InterfaceTypeRepresentation, InterfaceUnionStorageVariant, InterfaceUnionTag,
};
pub(crate) use validation::validate_constraint_templates;
