use std::collections::BTreeMap;

use bray_bound_tree::CheckedTemplateKind;
use bray_symbols::{SymbolKind, SymbolOrdinal};

use super::surface::{local_symbol, validate_symbol_kind};
use crate::{
    InterfaceDeclarationTemplate, InterfacePredicateDefinition, InterfacePredicateDefinitionState,
    InterfaceSemantics, InterfaceValidationError, PackageInterfaceSurface,
};

pub(crate) fn validate_predicate_definition(
    definition: &InterfacePredicateDefinition,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    local_symbol(definition.owner())?;

    let owner_kind = validate_symbol_kind(definition.owner(), surface)?;

    let is_valid = match definition.state() {
        InterfacePredicateDefinitionState::Defined => matches!(
            owner_kind,
            SymbolKind::Predicate
                | SymbolKind::TraitPredicateMember
                | SymbolKind::TraitPredicateFulfillment
        ),
        InterfacePredicateDefinitionState::Required => {
            owner_kind == SymbolKind::TraitPredicateMember
        }
        InterfacePredicateDefinitionState::OpaqueTrusted => owner_kind == SymbolKind::Predicate,
    };

    if !is_valid {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        ));
    }

    Ok(())
}

pub(crate) fn validate_predicate_template(
    template: &InterfaceDeclarationTemplate,
) -> Result<bool, InterfaceValidationError> {
    if template.kind() != CheckedTemplateKind::PredicateDefinition {
        return Ok(false);
    }

    if template.ordinal() != SymbolOrdinal::new(0) {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        ));
    }

    Ok(true)
}

pub(crate) fn validate_predicate_template_count(
    definition: &InterfacePredicateDefinition,
    actual: usize,
) -> Result<(), InterfaceValidationError> {
    let expected = usize::from(definition.state() == InterfacePredicateDefinitionState::Defined);

    if actual != expected {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        ));
    }

    Ok(())
}

pub(super) fn validate_predicate_templates(
    semantics: &InterfaceSemantics,
) -> Result<(), InterfaceValidationError> {
    let mut template_counts = BTreeMap::new();

    for template in &*semantics.declaration_templates {
        if validate_predicate_template(template)? {
            *template_counts.entry(template.owner()).or_insert(0_usize) += 1;
        }
    }

    for definition in &*semantics.predicate_definitions {
        let actual = template_counts.remove(definition.owner()).unwrap_or(0);

        validate_predicate_template_count(definition, actual)?;
    }

    if !template_counts.is_empty() {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Declaration,
        ));
    }

    Ok(())
}
