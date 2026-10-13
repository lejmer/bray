use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundCallDependencyContracts, BoundCallReference, BoundExpressionId, SelectedCall,
};
use bray_symbols::{DependencyContractTemplateData, DependencySubjectRoot};

pub(super) fn with_references<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    call: &SelectedCall,
    contracts: BoundCallDependencyContracts,
    template: &DependencyContractTemplateData,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let subjects = template
        .borrows()
        .iter()
        .flat_map(|relation| [relation.entry(), relation.exit()])
        .chain(template.effects().iter().filter_map(|effect| match effect {
            bray_symbols::DependencyStorageEffect::Unknown => None,
            bray_symbols::DependencyStorageEffect::Direct { subject, .. } => Some(subject),
            bray_symbols::DependencyStorageEffect::Indirect { reference, .. } => Some(reference),
        }))
        .collect::<std::collections::BTreeSet<_>>();

    let mut inputs = std::collections::BTreeMap::new();

    for subject in subjects {
        if let Some(prepared) = reference_input_type(request, call, subject)? {
            // Each selected input owns one path. Relations share its immutable formal subject.
            inputs.insert(subject.clone(), prepared);
        }
    }

    let mut references = Vec::with_capacity(template.borrows().len());

    for relation in template.borrows() {
        let Some((ty, _)) = inputs.get(relation.entry()) else {
            continue;
        };

        let ty = *ty;
        let kind = crate::representation::reference_kind(request, ty);

        let addressed_type = if relation.target_projections().is_empty() {
            kind.is_none().then_some(ty)
        } else {
            let Some(mut target) = crate::representation::reference_target_type(request, ty) else {
                continue;
            };

            let mut valid = true;

            for projection in relation.target_projections() {
                let projected = crate::storage::projected_value_type(request, target, *projection)?;

                let Some(projected) = *projected.value() else {
                    valid = false;
                    break;
                };

                target = projected;
            }

            if !valid {
                continue;
            }

            Some(target)
        };

        let output = output_argument(request, call, relation.exit());

        references.push(BoundCallReference::new(
            relation.clone(),
            kind == Some(relation.kind()) && relation.target_projections().is_empty(),
            addressed_type,
            output,
        ));
    }

    let witness = request
        .trusted_contracts()
        .and_then(|contracts| contracts.calls.get(&expression.into()))
        .is_some_and(crate::TrustedCallContract::result_guarantees_name_whole_owner);

    let retained_inputs = (witness
        || references.iter().any(|reference| {
            reference.relation().exit().subject_root() == DependencySubjectRoot::Result
        }))
    .then(|| crate::dependency::result_inputs(call, template));

    Ok(contracts
        .with_reference_paths(
            inputs
                .into_iter()
                .map(|(subject, (_, path))| (subject, path))
                .collect(),
        )
        .with_references(references, retained_inputs)
        .with_effects(template.effects().iter().cloned()))
}

fn reference_input_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &bray_bound_tree::SelectedCall,
    subject: &bray_symbols::DependencySubject,
) -> Result<
    Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>,
    CheckerQueryError<C::UpstreamError>,
> {
    let ty = match subject.subject_root() {
        bray_symbols::DependencySubjectRoot::Receiver => call.receiver().map(|receiver| {
            receiver
                .input_type(request.semantic_values())
                .expect("selected receiver has a valid passed value type")
        }),
        bray_symbols::DependencySubjectRoot::Parameter(parameter) => {
            call.arguments().iter().find_map(|argument| match argument {
                bray_bound_tree::SelectedArgument::Explicit {
                    ordinal,
                    conversion,
                    ..
                } if *ordinal == parameter.raw() => Some(conversion.target_type()),
                _ => None,
            })
        }
        _ => None,
    };

    let Some(ty) = ty else {
        return Ok(None);
    };

    let Some(projections) = crate::storage::storage_projections(request, subject.projections())
    else {
        return Ok(None);
    };

    crate::storage::reference_path(request, ty, projections.into(), None)
}

fn output_argument<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    output: &bray_symbols::DependencySubject,
) -> Option<BoundExpressionId> {
    // Rebinding the formal reference itself changes only the callee's local slot.
    if output.projections().is_empty() {
        return None;
    }

    match output.subject_root() {
        DependencySubjectRoot::Receiver => call.receiver().and_then(|receiver| {
            matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable
            )
            .then_some(receiver.expression())
        }),
        DependencySubjectRoot::Parameter(parameter) => {
            call.arguments().iter().find_map(|argument| {
                let bray_bound_tree::SelectedArgument::Explicit {
                    ordinal,
                    expression,
                    conversion,
                    ..
                } = argument
                else {
                    return None;
                };

                (*ordinal == parameter.raw()
                    && matches!(
                        request
                            .semantic_values()
                            .type_data(conversion.target_type())
                            .as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    ))
                .then_some(*expression)
            })
        }
        _ => None,
    }
}
