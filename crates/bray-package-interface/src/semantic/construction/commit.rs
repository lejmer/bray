use super::super::model::{
    InterfaceCallableContract, InterfaceDeclarationTemplate, InterfaceSemanticRecordKind,
    InterfaceSemantics,
};
use super::model::{
    InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
};
use super::remap::{commit_fragment, extend_records, normalize_records};

/// Commits declaration-owned semantic fragments onto one package-wide value table.
///
/// The caller supplies fragments in stable symbol order. Package-level implementation,
/// compatibility, runtime, and provenance records belong in `base`.
pub fn commit_interface_semantic_fragments(
    mut base: InterfaceSemantics,
    fragments: impl IntoIterator<Item = (InterfaceSemantics, InterfaceSemanticIdRemap)>,
) -> Result<InterfaceSemantics, InterfaceSemanticCommitError> {
    let mut checked_template_count = base.checked_templates.len();
    let mut support_entity_count = base.support_entities.len();
    let mut committed = Vec::new();

    for (fragment, remap) in fragments {
        let fragment = commit_fragment(
            fragment,
            &remap,
            checked_template_count,
            support_entity_count,
        )?;

        checked_template_count = checked_template_count
            .checked_add(fragment.checked_templates.len())
            .ok_or(InterfaceSemanticCommitError::IdentityOverflow(
                InterfaceSemanticTableKind::CheckedTemplate,
            ))?;

        support_entity_count = support_entity_count
            .checked_add(fragment.support_entities.len())
            .ok_or(InterfaceSemanticCommitError::IdentityOverflow(
                InterfaceSemanticTableKind::SupportEntity,
            ))?;

        committed.push(fragment);
    }

    extend_records(
        &mut base.constraints,
        committed
            .iter()
            .flat_map(|fragment| fragment.constraints.iter().cloned()),
    );

    extend_records(
        &mut base.callable_contracts,
        committed
            .iter()
            .flat_map(|fragment| fragment.callable_contracts.iter().cloned()),
    );

    extend_records(
        &mut base.callable_signatures,
        committed
            .iter()
            .flat_map(|fragment| fragment.callable_signatures.iter().cloned()),
    );

    extend_records(
        &mut base.generic_declarations,
        committed
            .iter()
            .flat_map(|fragment| fragment.generic_declarations.iter().cloned()),
    );

    extend_records(
        &mut base.callable_parameter_defaults,
        committed
            .iter()
            .flat_map(|fragment| fragment.callable_parameter_defaults.iter().cloned()),
    );

    extend_records(
        &mut base.predicate_definitions,
        committed
            .iter()
            .flat_map(|fragment| fragment.predicate_definitions.iter().cloned()),
    );

    extend_records(
        &mut base.declared_types,
        committed
            .iter()
            .flat_map(|fragment| fragment.declared_types.iter().cloned()),
    );

    extend_records(
        &mut base.type_representations,
        committed
            .iter()
            .flat_map(|fragment| fragment.type_representations.iter().cloned()),
    );

    extend_records(
        &mut base.checked_templates,
        committed
            .iter()
            .flat_map(|fragment| fragment.checked_templates.iter().cloned()),
    );

    extend_records(
        &mut base.support_entities,
        committed
            .iter()
            .flat_map(|fragment| fragment.support_entities.iter().cloned()),
    );

    extend_records(
        &mut base.declaration_templates,
        committed
            .iter()
            .flat_map(|fragment| fragment.declaration_templates.iter().cloned()),
    );

    normalize_records(
        &mut base.constraints,
        InterfaceSemanticRecordKind::GenericConstraint,
        |left, right| left.owner == right.owner && left.ordinal == right.ordinal,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.callable_contracts,
        InterfaceSemanticRecordKind::CallableContracts,
        |left, right| left.owner == right.owner,
        InterfaceCallableContract::owner,
    )?;

    normalize_records(
        &mut base.callable_signatures,
        InterfaceSemanticRecordKind::CallableSignature,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.generic_declarations,
        InterfaceSemanticRecordKind::GenericDeclaration,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.callable_parameter_defaults,
        InterfaceSemanticRecordKind::CallableParameterDefault,
        |left, right| left.parameter == right.parameter,
        |record| &record.parameter,
    )?;

    normalize_records(
        &mut base.predicate_definitions,
        InterfaceSemanticRecordKind::PredicateDefinition,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.declared_types,
        InterfaceSemanticRecordKind::DeclaredType,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.type_representations,
        InterfaceSemanticRecordKind::TypeRepresentation,
        |left, right| left.owner == right.owner,
        |record| &record.owner,
    )?;

    normalize_records(
        &mut base.declaration_templates,
        InterfaceSemanticRecordKind::DeclarationTemplate,
        |left, right| {
            left.owner() == right.owner()
                && left.kind() == right.kind()
                && left.ordinal() == right.ordinal()
        },
        InterfaceDeclarationTemplate::owner,
    )?;

    Ok(base)
}
