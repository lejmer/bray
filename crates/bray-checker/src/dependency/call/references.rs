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
    let references = template
        .borrows()
        .iter()
        .map(|relation| {
            reference_input_type(request, call, relation).map(|ty| {
                ty.map(|ty| {
                    let kind = crate::representation::reference_kind(request, ty);

                    BoundCallReference::new(
                        relation.clone(),
                        kind == Some(relation.kind()),
                        kind.is_none().then_some(ty),
                        output_argument(request, call, relation.exit()),
                    )
                })
            })
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

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
        .with_references(references, retained_inputs)
        .with_effects(template.effects().iter().cloned()))
}

fn reference_input_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &bray_bound_tree::SelectedCall,
    relation: &bray_symbols::DependencyBorrowRelation,
) -> Result<Option<bray_symbols::TypeId>, CheckerQueryError<C::UpstreamError>> {
    let mut ty = match relation.entry().subject_root() {
        bray_symbols::DependencySubjectRoot::Receiver => {
            call.receiver().map(|receiver| receiver.target_type())
        }
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

    for projection in relation.entry().projections() {
        let Some(owner) = ty else {
            return Ok(None);
        };

        let projected = crate::storage::projected_value_type(request, owner, *projection)?;

        ty = *projected.value();
    }

    Ok(ty)
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
