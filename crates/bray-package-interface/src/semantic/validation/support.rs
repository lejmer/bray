use std::collections::BTreeSet;

use bray_symbols::SymbolKind;

use crate::semantic::model::{InterfaceSemanticFacts, InterfaceSupportEntity};
use crate::validation::is_strictly_sorted;
use crate::{InterfaceValidationError, semantic::validation::fact::validate_index};

use super::checked_index;

pub(super) fn validate_support_entities(
    facts: &InterfaceSemanticFacts,
) -> Result<Vec<(usize, usize)>, InterfaceValidationError> {
    if !is_strictly_sorted(&facts.support_entities) {
        return Err(InterfaceValidationError::Malformed);
    }

    let mut template_ids = BTreeSet::new();
    let mut declaration_keys = BTreeSet::new();
    let mut template_entities = Vec::new();

    for (entity_index, entity) in facts.support_entities.iter().enumerate() {
        match entity {
            InterfaceSupportEntity::CheckedTemplate(template) => {
                let template_index =
                    checked_index(template.to_index(), facts.checked_templates.len())?;

                if !template_ids.insert(template_index) {
                    return Err(InterfaceValidationError::Malformed);
                }

                template_entities.push((entity_index, template_index));
            }
            InterfaceSupportEntity::Declaration(declaration) => {
                if !is_support_declaration_kind(declaration.kind())
                    || !declaration_keys.insert(declaration)
                {
                    return Err(InterfaceValidationError::Malformed);
                }
            }
            InterfaceSupportEntity::Implementation(implementation) => {
                if !is_implementation_kind(implementation.declaration().kind())
                    || !declaration_keys.insert(implementation.declaration())
                {
                    return Err(InterfaceValidationError::Malformed);
                }

                validate_index(implementation.subject().to_index(), facts.types.len())?;

                match (
                    implementation.declaration().kind(),
                    implementation.trait_application(),
                ) {
                    (SymbolKind::InherentImplementation, None) => {}
                    (
                        SymbolKind::UnnamedTraitImplementation
                        | SymbolKind::NamedTraitImplementation,
                        Some(application),
                    ) => validate_index(application.to_index(), facts.trait_applications.len())?,
                    _ => return Err(InterfaceValidationError::Malformed),
                }
            }
        }
    }

    Ok(template_entities)
}

fn is_support_declaration_kind(kind: SymbolKind) -> bool {
    !matches!(
        kind,
        SymbolKind::CompilerKnownEnvironment
            | SymbolKind::Package
            | SymbolKind::Module
            | SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::NamedTraitImplementation
    )
}

fn is_implementation_kind(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::InherentImplementation
            | SymbolKind::UnnamedTraitImplementation
            | SymbolKind::NamedTraitImplementation
    )
}
