use bray_symbols::InterfaceSymbolId;

use super::super::{declaration as codec, facts};
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

    Ok(InterfaceSemanticFacts::new().with_declarations([], [], [], [definition]))
}

fn declaration_record(
    directory: &[InterfaceSemanticFactEntry],
    owner: &InterfaceSymbolReference,
    kind: InterfaceSemanticFactKind,
) -> Result<u32, InterfaceValidationError> {
    let mut entries = directory
        .iter()
        .filter(|entry| entry.owner() == owner && entry.kind() == kind);

    let Some(entry) = entries.next() else {
        return Err(InterfaceValidationError::Malformed);
    };

    if entries.next().is_some() || entry.section() != InterfaceSectionTag::DeclarationFacts {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(entry.record())
}
