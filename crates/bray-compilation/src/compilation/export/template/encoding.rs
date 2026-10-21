use std::cell::RefCell;

use bray_bound_tree::{CheckedTemplate, CheckedTemplateInputKind};
use bray_package_interface::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateExecution,
    InterfaceCheckedTemplateInput, InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateTemporary, InterfaceImplementationReference,
    InterfaceTemplateReference,
};

use super::super::PackageInterfaceExportError;
use super::super::semantic::SemanticExporter;

pub(in crate::compilation::export) fn encode_template(
    export: &mut SemanticExporter<'_>,
    template: &CheckedTemplate,
) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
    let inputs = template
        .inputs()
        .iter()
        .map(|input| {
            let kind = match input.kind() {
                CheckedTemplateInputKind::GenericType(key) => {
                    InterfaceCheckedTemplateInputKind::GenericType(export.key_reference(key)?)
                }
                CheckedTemplateInputKind::GenericConstant(key) => {
                    InterfaceCheckedTemplateInputKind::GenericConstant(export.key_reference(key)?)
                }
                CheckedTemplateInputKind::Receiver => InterfaceCheckedTemplateInputKind::Receiver,
                CheckedTemplateInputKind::Parameter(ordinal) => {
                    InterfaceCheckedTemplateInputKind::Parameter(*ordinal)
                }
                CheckedTemplateInputKind::PostconditionResult => {
                    InterfaceCheckedTemplateInputKind::PostconditionResult
                }
            };

            Ok(InterfaceCheckedTemplateInput::new(
                kind,
                export.type_id(input.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

    // The existing mapper calls these disjoint reference projections sequentially.
    // A local cell shares the exporter without cloning its growing semantic tables.
    let references = RefCell::new(&mut *export);

    let nodes = template
        .nodes()
        .iter()
        .map(|node| {
            let operation = node.operation().try_map_references(
                |term| references.borrow_mut().constant_term_id(*term),
                |ty| references.borrow_mut().type_id(*ty),
                |key| {
                    references
                        .borrow()
                        .key_reference(key)
                        .map(InterfaceTemplateReference::Symbol)
                },
                |substitution| references.borrow_mut().substitution_id(*substitution),
                |key| {
                    references
                        .borrow()
                        .key_reference(key)
                        .map(InterfaceImplementationReference::Symbol)
                },
            )?;

            Ok(InterfaceCheckedTemplateNode::new(
                operation,
                references.borrow_mut().type_id(node.ty())?,
            ))
        })
        .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

    let temporaries = template
        .temporaries()
        .iter()
        .map(|temporary| {
            Ok(InterfaceCheckedTemplateTemporary::new(
                temporary.initializer(),
                export.type_id(temporary.ty())?,
                export.dependency_contract_id(temporary.dependency_contract())?,
            ))
        })
        .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

    let behavior = template.behavior();

    let encoded_behavior = InterfaceCheckedTemplateBehavior::new(
        behavior
            .effects()
            .iter()
            .map(|effect| export.key_reference(effect.declaration()))
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .capabilities()
            .iter()
            .map(|capability| export.key_reference(capability.declaration()))
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .trusted_obligations()
            .iter()
            .map(|obligation| export.key_reference(obligation.declaration()))
            .collect::<Result<Vec<_>, _>>()?,
        InterfaceCheckedTemplateExecution::new(
            behavior
                .execution_requirements()
                .iter()
                .map(|requirement| export.key_reference(requirement.declaration()))
                .collect::<Result<Vec<_>, _>>()?,
            behavior.current_run_cancellation(),
        )
        .with_certification(
            behavior.execution().properties().iter().copied(),
            behavior.execution().constant_eligible(),
            behavior.execution().may_unwind(),
        ),
        behavior.lifecycle_obligations().iter().copied(),
        export.dependency_contract_id(behavior.dependency_contract())?,
        export.dependency_contract_id(behavior.result_dependencies())?,
        behavior
            .witnesses()
            .iter()
            .map(|witness| {
                export
                    .key_reference(witness.declaration())
                    .map(InterfaceImplementationReference::Symbol)
            })
            .collect::<Result<Vec<_>, _>>()?,
    );

    Ok(InterfaceCheckedTemplate::new(
        template.kind(),
        inputs,
        nodes,
        temporaries,
        template.result(),
        encoded_behavior,
    ))
}
