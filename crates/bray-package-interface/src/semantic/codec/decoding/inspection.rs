use crate::inspection::{InterfaceInspectionRecord, InterfaceInspectionRecordKind};
use crate::semantic::codec::common::SemanticDecodeContext;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFacts, InterfaceValidationError,
    InterfaceValidationLimits, ValidatedInterfaceSection,
};

use super::{contract, declaration, directory, facts, support, surface, template, value};

pub(crate) fn decode_inspection_records(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    facts::validate_decode_allocation(&[section], limits)?;

    let mut context = SemanticDecodeContext::new(limits);
    let mut decoded = InterfaceSemanticFacts::new();

    match section.tag() {
        InterfaceSectionTag::SymbolFactDirectory => {
            inspect_fact_directory(section, limits, &mut context)
        }
        InterfaceSectionTag::SemanticTypes => inspect_types(section, limits, &mut context),
        InterfaceSectionTag::Constants => {
            inspect_constants(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::Contracts => {
            inspect_contracts(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::DeclarationFacts => {
            inspect_declarations(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::DeclarationTemplates => {
            inspect_templates(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::Implementations => {
            inspect_implementations(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::TargetDependencies => {
            inspect_target_dependencies(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::SourceProvenance => {
            inspect_provenance(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::SupportGraph => {
            inspect_support_graph(section, limits, &mut context, &mut decoded)
        }
        InterfaceSectionTag::Strings
        | InterfaceSectionTag::PackageMetadata
        | InterfaceSectionTag::Dependencies
        | InterfaceSectionTag::SymbolIdentities
        | InterfaceSectionTag::Relationships
        | InterfaceSectionTag::ExportedLookup => Err(InterfaceValidationError::Malformed),
    }
}

fn inspect_fact_directory(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    let directory = directory::decode_fact_directory(section, limits, context)?;

    Ok(vec![record(
        InterfaceInspectionRecordKind::SymbolFacts,
        directory.len(),
    )])
}

fn inspect_types(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    let decoded = value::decode_types(section, limits, context)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::GenericSubstitutions,
            decoded.substitutions.len(),
        ),
        record(
            InterfaceInspectionRecordKind::TraitApplications,
            decoded.trait_applications.len(),
        ),
        record(
            InterfaceInspectionRecordKind::CallableInstances,
            decoded.callable_instances.len(),
        ),
        record(
            InterfaceInspectionRecordKind::ImplementationInstances,
            decoded.implementation_instances.len(),
        ),
        record(
            InterfaceInspectionRecordKind::SemanticTypes,
            decoded.types.len(),
        ),
    ])
}

fn inspect_constants(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    value::decode_constants(section, limits, context, decoded)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::ConstantValues,
            decoded.constant_values.len(),
        ),
        record(
            InterfaceInspectionRecordKind::ConstantTerms,
            decoded.constant_terms.len(),
        ),
    ])
}

fn inspect_contracts(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    contract::decode_contracts(section, limits, context, decoded)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::DependencyContracts,
            decoded.dependency_contracts.len(),
        ),
        record(
            InterfaceInspectionRecordKind::Constraints,
            decoded.constraints.len(),
        ),
        record(
            InterfaceInspectionRecordKind::CallableContracts,
            decoded.callable_contracts.len(),
        ),
    ])
}

fn inspect_declarations(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    declaration::decode_declarations(section, limits, context, decoded)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::CallableSignatures,
            decoded.callable_signatures.len(),
        ),
        record(
            InterfaceInspectionRecordKind::GenericDeclarations,
            decoded.generic_declarations.len(),
        ),
        record(
            InterfaceInspectionRecordKind::CallableParameterDefaults,
            decoded.callable_parameter_defaults.len(),
        ),
    ])
}

fn inspect_templates(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    template::decode_templates(section, limits, context, decoded)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::CheckedTemplates,
            decoded.checked_templates.len(),
        ),
        record(
            InterfaceInspectionRecordKind::DeclarationTemplates,
            decoded.declaration_templates.len(),
        ),
    ])
}

fn inspect_implementations(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    surface::decode_implementations(section, limits, context, decoded)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::Implementations,
            decoded.implementations.len(),
        ),
        record(
            InterfaceInspectionRecordKind::Coherence,
            decoded.coherence.len(),
        ),
    ])
}

fn inspect_target_dependencies(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    surface::decode_target_dependencies(section, limits, context, decoded)?;

    Ok(vec![
        record(
            InterfaceInspectionRecordKind::TargetFacts,
            decoded.target_dependencies.len(),
        ),
        record(
            InterfaceInspectionRecordKind::AbiDependencies,
            decoded.abi_dependencies.len(),
        ),
    ])
}

fn inspect_provenance(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    surface::decode_provenance(section, limits, context, decoded)?;

    Ok(vec![record(
        InterfaceInspectionRecordKind::SourceProvenance,
        decoded.provenance.len(),
    )])
}

fn inspect_support_graph(
    section: ValidatedInterfaceSection<'_>,
    limits: InterfaceValidationLimits,
    context: &mut SemanticDecodeContext,
    decoded: &mut InterfaceSemanticFacts,
) -> Result<Vec<InterfaceInspectionRecord>, InterfaceValidationError> {
    support::decode_support_graph(section, limits, context, decoded)?;

    Ok(vec![record(
        InterfaceInspectionRecordKind::SupportEntities,
        decoded.support_entities.len(),
    )])
}

fn record(kind: InterfaceInspectionRecordKind, count: usize) -> InterfaceInspectionRecord {
    InterfaceInspectionRecord::new(kind, u64::try_from(count).unwrap_or(u64::MAX))
}
