mod codec;
mod construction;
mod interning;
mod model;
mod validation;

pub(crate) use codec::{
    COMPLETE_SEMANTIC_SECTIONS, SemanticDecodeContext, decode_inspection_records,
    decode_selected_semantic_graph, decode_semantic_graph, decode_template_payload,
    encode_template_payload, encode_validated_semantics, read_symbol_reference,
    selected_semantic_sections, validate_decode_allocation, write_symbol_reference,
};
pub use codec::{EncodedSemanticSection, decode_semantics, encode_semantics};
pub use construction::{
    InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
    commit_interface_semantic_fragments,
};
pub use interning::{
    ImportedAbiDependency, ImportedCallableContract, ImportedCallableParameterDefault,
    ImportedCallableSignature, ImportedConstraint, ImportedDeclarationTemplate,
    ImportedDeclaredType, ImportedGenericDeclaration, ImportedImplementation,
    ImportedPredicateDefinition, ImportedRuntimeRequirement, ImportedSemanticRecord,
    ImportedSemantics, ImportedSourceProvenance, ImportedTargetProperty,
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
    InterfaceSemanticRecord, InterfaceSemanticRecordKind, InterfaceSemantics,
    InterfaceSourceProvenance, InterfaceStorageMember, InterfaceStorageShape,
    InterfaceSupportEntity, InterfaceSupportImplementation, InterfaceTargetPropertyDependency,
    InterfaceTemplateReference, InterfaceTraitApplication, InterfaceTraitApplicationId,
    InterfaceTrustedCapabilityRequirement, InterfaceType, InterfaceTypeId,
    InterfaceTypeRepresentation, InterfaceUnionStorageVariant, InterfaceUnionTag,
};
pub(crate) use validation::validate_constraint_templates;
