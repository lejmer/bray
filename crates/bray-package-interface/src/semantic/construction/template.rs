use bray_symbols::InterfaceSupportEntityId;

use super::super::model::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateId, InterfaceCheckedTemplateInput, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceCheckedTemplateTemporary,
    InterfaceImplementationReference, InterfaceSupportEntity, InterfaceSupportImplementation,
    InterfaceTemplateReference,
};
use super::model::{
    InterfaceSemanticCommitError, InterfaceSemanticIdRemap, InterfaceSemanticTableKind,
};

pub(super) fn remap_checked_template(
    template: &InterfaceCheckedTemplate,
    remap: &InterfaceSemanticIdRemap,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceCheckedTemplate, InterfaceSemanticCommitError> {
    let inputs = template
        .inputs()
        .iter()
        .map(|input| {
            Ok(InterfaceCheckedTemplateInput::new(
                input.kind().clone(),
                remap.ty(input.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?;

    let nodes = template
        .nodes()
        .iter()
        .map(|node| {
            Ok(InterfaceCheckedTemplateNode::new(
                remap_checked_operation(node.operation(), remap, support_base, support_count)?,
                remap.ty(node.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?;

    let temporaries = template
        .temporaries()
        .iter()
        .copied()
        .map(|temporary| {
            Ok(InterfaceCheckedTemplateTemporary::new(
                temporary.initializer(),
                remap.ty(temporary.ty())?,
                remap.dependency_contract(temporary.dependency_contract())?,
            ))
        })
        .collect::<Result<Vec<_>, InterfaceSemanticCommitError>>()?;

    let behavior = remap_checked_behavior(template.behavior(), remap, support_base, support_count)?;

    Ok(InterfaceCheckedTemplate::new(
        template.kind(),
        inputs,
        nodes,
        temporaries,
        template.result(),
        behavior,
    ))
}

fn remap_checked_behavior(
    behavior: &InterfaceCheckedTemplateBehavior,
    remap: &InterfaceSemanticIdRemap,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceCheckedTemplateBehavior, InterfaceSemanticCommitError> {
    let execution = InterfaceCheckedTemplateExecution::new(
        behavior.execution_requirements().iter().cloned(),
        behavior.current_run_cancellation(),
    );

    let witnesses = behavior
        .witnesses()
        .iter()
        .map(|reference| remap_implementation_reference(reference, support_base, support_count))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(InterfaceCheckedTemplateBehavior::new(
        behavior.effects().iter().cloned(),
        behavior.capabilities().iter().cloned(),
        behavior.trusted_obligations().iter().cloned(),
        execution,
        behavior.lifecycle_obligations().iter().copied(),
        remap.dependency_contract(behavior.dependency_contract())?,
        remap.dependency_contract(behavior.result_dependencies())?,
        witnesses,
    ))
}

fn remap_checked_operation(
    operation: &InterfaceCheckedTemplateOperation,
    remap: &InterfaceSemanticIdRemap,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceCheckedTemplateOperation, InterfaceSemanticCommitError> {
    operation.try_map_references(
        |term| remap.constant_term(*term),
        |ty| remap.ty(*ty),
        |reference| remap_template_reference(reference, support_base, support_count),
        |substitution| remap.substitution(*substitution),
        |reference| remap_implementation_reference(reference, support_base, support_count),
    )
}

fn remap_template_reference(
    reference: &InterfaceTemplateReference,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceTemplateReference, InterfaceSemanticCommitError> {
    match reference {
        InterfaceTemplateReference::Symbol(symbol) => {
            Ok(InterfaceTemplateReference::Symbol(symbol.clone()))
        }
        InterfaceTemplateReference::Support(entity) => {
            offset_support_entity(*entity, support_base, support_count)
                .map(InterfaceTemplateReference::Support)
        }
    }
}

fn remap_implementation_reference(
    reference: &InterfaceImplementationReference,
    support_base: u32,
    support_count: usize,
) -> Result<InterfaceImplementationReference, InterfaceSemanticCommitError> {
    match reference {
        InterfaceImplementationReference::Symbol(symbol) => {
            Ok(InterfaceImplementationReference::Symbol(symbol.clone()))
        }
        InterfaceImplementationReference::Support(entity) => {
            offset_support_entity(*entity, support_base, support_count)
                .map(InterfaceImplementationReference::Support)
        }
    }
}

pub(super) fn remap_support_entity(
    entity: &InterfaceSupportEntity,
    remap: &InterfaceSemanticIdRemap,
    checked_template_base: u32,
    checked_template_count: usize,
) -> Result<InterfaceSupportEntity, InterfaceSemanticCommitError> {
    match entity {
        InterfaceSupportEntity::Declaration(declaration) => {
            Ok(InterfaceSupportEntity::Declaration(declaration.clone()))
        }
        InterfaceSupportEntity::Implementation(implementation) => Ok(
            InterfaceSupportEntity::Implementation(InterfaceSupportImplementation::new(
                implementation.declaration().clone(),
                remap.ty(implementation.subject())?,
                implementation
                    .trait_application()
                    .map(|application| remap.trait_application(application))
                    .transpose()?,
            )),
        ),
        InterfaceSupportEntity::CheckedTemplate(template) => {
            offset_checked_template(*template, checked_template_base, checked_template_count)
                .map(InterfaceSupportEntity::CheckedTemplate)
        }
    }
}

pub(super) fn offset_support_entity(
    id: InterfaceSupportEntityId,
    base: u32,
    count: usize,
) -> Result<InterfaceSupportEntityId, InterfaceSemanticCommitError> {
    offset_reference(
        id.raw(),
        base,
        count,
        InterfaceSemanticTableKind::SupportEntity,
    )
    .map(InterfaceSupportEntityId::new)
}

fn offset_checked_template(
    id: InterfaceCheckedTemplateId,
    base: u32,
    count: usize,
) -> Result<InterfaceCheckedTemplateId, InterfaceSemanticCommitError> {
    offset_reference(
        id.raw(),
        base,
        count,
        InterfaceSemanticTableKind::CheckedTemplate,
    )
    .map(InterfaceCheckedTemplateId::new)
}

fn offset_reference(
    raw: u32,
    base: u32,
    count: usize,
    table: InterfaceSemanticTableKind,
) -> Result<u32, InterfaceSemanticCommitError> {
    let index = usize::try_from(raw).ok();

    if index.is_none_or(|index| index >= count) {
        return Err(InterfaceSemanticCommitError::MissingReference {
            table,
            reference: raw,
        });
    }

    base.checked_add(raw)
        .ok_or(InterfaceSemanticCommitError::IdentityOverflow(table))
}

pub(super) fn offset(
    length: usize,
    table: InterfaceSemanticTableKind,
) -> Result<u32, InterfaceSemanticCommitError> {
    u32::try_from(length).map_err(|_| InterfaceSemanticCommitError::IdentityOverflow(table))
}
