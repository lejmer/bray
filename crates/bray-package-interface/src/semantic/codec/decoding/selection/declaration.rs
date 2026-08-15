use bray_symbols::InterfaceSymbolId;

use super::super::{declaration as codec, bundle, template as template_codec};
use crate::semantic::codec::common::SemanticDecodeContext;
use crate::{
    InterfaceSectionTag, InterfaceSemanticRecord, InterfaceSemanticRecordKind,
    InterfaceSemantics, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface, ValidatedInterfaceSection,
};

pub(super) fn decode_callable_parameter_default(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    limits: InterfaceValidationLimits,
    mut context: SemanticDecodeContext,
    directory: &[InterfaceSemanticRecord],
) -> Result<InterfaceSemantics, InterfaceValidationError> {
    let owner = InterfaceSymbolReference::Local(owner);

    let record = declaration_record(
        directory,
        &owner,
        InterfaceSemanticRecordKind::CallableParameterDefault,
    )?;

    let section = bundle::required_section(sections, InterfaceSectionTag::DeclarationSemantics)?;
    let tables = codec::decode_declaration_tables(section, &mut context)?;

    let default = tables.callable_parameter_defaults.decode(
        record,
        &mut context,
        codec::decode_callable_parameter_default,
    )?;

    if default.parameter() != &owner {
        return Err(InterfaceValidationError::Malformed);
    }

    let semantics = InterfaceSemantics::new().with_declarations([], [], [default], []);

    semantics.validate(surface, limits)?;

    Ok(semantics)
}

pub(super) fn decode_predicate_definition(
    sections: &[ValidatedInterfaceSection<'_>],
    surface: &PackageInterfaceSurface,
    owner: InterfaceSymbolId,
    _limits: InterfaceValidationLimits,
    mut context: SemanticDecodeContext,
    directory: &[InterfaceSemanticRecord],
) -> Result<InterfaceSemantics, InterfaceValidationError> {
    let owner = InterfaceSymbolReference::Local(owner);

    let record = declaration_record(
        directory,
        &owner,
        InterfaceSemanticRecordKind::PredicateDefinition,
    )?;

    let section = bundle::required_section(sections, InterfaceSectionTag::DeclarationSemantics)?;
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

    let section = bundle::required_section(sections, InterfaceSectionTag::DeclarationTemplates)?;
    let tables = template_codec::decode_template_tables(section, &mut context)?;

    validate_declaration_template_directory(directory, tables.declaration_templates.len())?;

    let template_records = directory.iter().filter(|entry| {
        entry.owner() == &owner && entry.kind() == InterfaceSemanticRecordKind::DeclarationTemplate
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

    Ok(InterfaceSemantics::new().with_declarations([], [], [], [definition]))
}

fn validate_declaration_template_directory(
    directory: &[InterfaceSemanticRecord],
    record_count: usize,
) -> Result<(), InterfaceValidationError> {
    let mut records = directory
        .iter()
        .filter(|entry| entry.kind() == InterfaceSemanticRecordKind::DeclarationTemplate)
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
    directory: &[InterfaceSemanticRecord],
    owner: &InterfaceSymbolReference,
    kind: InterfaceSemanticRecordKind,
) -> Result<u32, InterfaceValidationError> {
    let mut entries = directory.iter().filter(|entry| {
        entry.owner() == owner
            && entry.kind() == kind
            && entry.section() == InterfaceSectionTag::DeclarationSemantics
    });

    let Some(entry) = entries.next() else {
        return Err(InterfaceValidationError::Malformed);
    };

    if entries.next().is_some() {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(entry.record())
}
