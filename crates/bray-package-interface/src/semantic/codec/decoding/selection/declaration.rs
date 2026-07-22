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

    let entries = directory
        .iter()
        .filter(|entry| {
            entry.owner() == &owner
                && entry.kind() == InterfaceSemanticFactKind::CallableParameterDefault
        })
        .collect::<Vec<_>>();

    let [entry] = entries.as_slice() else {
        return Err(InterfaceValidationError::Malformed);
    };

    if entry.section() != InterfaceSectionTag::DeclarationFacts {
        return Err(InterfaceValidationError::Malformed);
    }

    let section = facts::required_section(sections, InterfaceSectionTag::DeclarationFacts)?;
    let tables = codec::decode_declaration_tables(section, &mut context)?;

    let default = tables.callable_parameter_defaults.decode(
        entry.record(),
        &mut context,
        codec::decode_callable_parameter_default,
    )?;

    if default.parameter() != &owner {
        return Err(InterfaceValidationError::Malformed);
    }

    let facts = InterfaceSemanticFacts::new().with_declarations([], [], [default]);

    facts.validate(surface, limits)?;

    Ok(facts)
}
