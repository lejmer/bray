use bray_symbols::InterfaceSymbolId;

use super::super::{declaration as codec, facts, template as template_codec};
use crate::semantic::codec::common::SemanticDecodeContext;
use crate::{
    InterfaceSectionTag, InterfaceSemanticFactEntry, InterfaceSemanticFactKind,
    InterfaceSemanticFacts, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface, ValidatedInterfaceSection,
};

pub(super) fn decode_callable_parameter_default(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    limits: InterfaceValidationLimits,
    mut context: SemanticDecodeContext,
    directory: &[InterfaceSemanticFactEntry],
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let owner = InterfaceSymbolReference::Local(owner);

    let record = declaration_record(
        directory,
        &owner,
        InterfaceSemanticFactKind::CallableParameterDefault,
    )?;

    let section = facts::required_section(sections, InterfaceSectionTag::DeclarationFacts)?;
    let tables = codec::decode_declaration_tables(section, &mut context)?;

    let default = tables.callable_parameter_defaults.decode(
        record,
        &mut context,
        codec::decode_callable_parameter_default,
    )?;

    if default.parameter() != &owner {
        return Err(InterfaceValidationError::Malformed);
    }

    let facts = InterfaceSemanticFacts::new().with_declarations([], [], [default], []);

    facts.validate(surface, limits)?;

    Ok(facts)
}

pub(super) fn decode_predicate_definition(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    _limits: InterfaceValidationLimits,
    mut context: SemanticDecodeContext,
    directory: &[InterfaceSemanticFactEntry],
) -> Result<InterfaceSemanticFacts, InterfaceValidationError> {
    let owner = InterfaceSymbolReference::Local(owner);

    let record = declaration_record(
        directory,
        &owner,
        InterfaceSemanticFactKind::PredicateDefinition,
    )?;

    let section = facts::required_section(sections, InterfaceSectionTag::DeclarationFacts)?;
    let tables = codec::decode_declaration_tables(section, &mut context)?;

    let definition = tables.predicate_definitions.decode(
        record,
        &mut context,
        codec::decode_predicate_definition,
    )?;

    if definition.owner() != &owner {
        return Err(InterfaceValidationError::Malformed);
    }

    crate::semantic::validation::validate_predicate_definition(&definition, surface)?;

    let section = facts::required_section(sections, InterfaceSectionTag::DeclarationTemplates)?;
    let tables = template_codec::decode_template_tables(section, &mut context)?;

    validate_declaration_template_directory(directory, tables.declaration_templates.len())?;

    let template_records = directory.iter().filter(|entry| {
        entry.owner() == &owner && entry.kind() == InterfaceSemanticFactKind::DeclarationTemplate
    });

    let mut predicate_template_count = 0_usize;

    for entry in template_records {
        let template = tables.declaration_templates.decode(
            entry.record(),
            &mut context,
            template_codec::decode_declaration_template,
        )?;

        if template.owner() != &owner {
            return Err(InterfaceValidationError::Malformed);
        }

        if crate::semantic::validation::validate_predicate_template(&template)? {
            predicate_template_count += 1;
        }
    }

    crate::semantic::validation::validate_predicate_template_count(
        &definition,
        predicate_template_count,
    )?;

    Ok(InterfaceSemanticFacts::new().with_declarations([], [], [], [definition]))
}

fn validate_declaration_template_directory(
    directory: &[InterfaceSemanticFactEntry],
    record_count: usize,
) -> Result<(), InterfaceValidationError> {
    let mut records = directory
        .iter()
        .filter(|entry| entry.kind() == InterfaceSemanticFactKind::DeclarationTemplate)
        .map(|entry| {
            if entry.section() != InterfaceSectionTag::DeclarationTemplates {
                return Err(InterfaceValidationError::Malformed);
            }

            usize::try_from(entry.record()).map_err(|_| InterfaceValidationError::Malformed)
        })
        .collect::<Result<Vec<_>, _>>()?;

    records.sort_unstable();

    if records.len() != record_count
        || !records
            .into_iter()
            .enumerate()
            .all(|(expected, actual)| expected == actual)
    {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn declaration_record(
    directory: &[InterfaceSemanticFactEntry],
    owner: &InterfaceSymbolReference,
    kind: InterfaceSemanticFactKind,
) -> Result<u32, InterfaceValidationError> {
    let mut entries = directory.iter().filter(|entry| {
        entry.owner() == owner
            && entry.kind() == kind
            && entry.section() == InterfaceSectionTag::DeclarationFacts
    });

    let Some(entry) = entries.next() else {
        return Err(InterfaceValidationError::Malformed);
    };

    if entries.next().is_some() {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(entry.record())
}
