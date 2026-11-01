use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundCallDependencyContracts, BoundCallReference, BoundExpressionId, SelectedCall,
};
use bray_symbols::DependencySubjectRoot;

pub(super) fn with_references<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    call: &SelectedCall,
    contracts: BoundCallDependencyContracts,
    template: &bray_symbols::DependencyContractTemplateData,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let deferred = matches!(
        call.resolution().result(),
        bray_bound_tree::BoundCallResult::LazyFuture(_)
    );

    let contracts = if deferred {
        contracts
            .with_deferred_execution(
                template.effects().iter().cloned(),
                template.normal_completion(),
            )
            .with_deferred_storage_preservation(template.preserves_storage_on_normal_return())
    } else {
        contracts
    };

    let retained_inputs = result_retains_inputs(request, expression.into(), template)
        .then(|| crate::dependency::result_inputs(expression, call, template));

    Ok(
        with_reference_inputs(request, contracts, template, retained_inputs, |subject| {
            reference_input_type(request, call, subject)
        })?
        .with_effects(template.effects().iter().filter(|_| !deferred).cloned())
        .with_synchronous_cleanup(template.synchronous_cleanup())
        .with_carriers(template.carriers().iter().cloned())
        .with_returned_input(template.returned_input())
        .with_returned_input_conditions(returned_input_conditions(
            request,
            template.returned_input(),
            |ordinal| {
                let receiver = call.receiver().is_some();

                let root = if receiver && ordinal == 0 {
                    DependencySubjectRoot::Receiver
                } else {
                    DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                        ordinal - u32::from(receiver),
                    ))
                };

                Ok(reference_input_type(
                    request,
                    call,
                    &bray_symbols::DependencySubject::root(root),
                )?
                .map(|(ty, _)| ty))
            },
        )?)
        .with_normal_completion(deferred || template.normal_completion())
        .with_normal_storage_preservation(
            deferred || template.preserves_storage_on_normal_return(),
        ),
    )
}

fn result_retains_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    invocation: bray_bound_tree::BoundExecutionSite,
    template: &bray_symbols::DependencyContractTemplateData,
) -> bool {
    request
        .trusted_contracts()
        .and_then(|contracts| contracts.calls.get(&invocation))
        .is_some_and(|contract| {
            contract
                .witness_subjects
                .iter()
                .any(|(_, subject)| subject.observes_completion_result())
                || contract
                    .address_subjects
                    .iter()
                    .any(|(_, subject, _)| subject.observes_completion_result())
        })
        || !template.carriers().is_empty()
        || template
            .borrows()
            .iter()
            .any(|relation| relation.exit().subject_root() == DependencySubjectRoot::Result)
}

pub(super) fn with_hidden_references<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    root: DependencySubjectRoot,
    ty: bray_symbols::TypeId,
    source: bray_bound_tree::BoundExecutionSite,
    invocation: bray_bound_tree::BoundExecutionSite,
    contracts: BoundCallDependencyContracts,
    template: &bray_symbols::DependencyContractTemplateData,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let retained = result_retains_inputs(request, invocation, template)
        .then(|| std::collections::BTreeSet::from([source]));

    let contracts = with_reference_inputs(request, contracts, template, retained, |subject| {
        if subject.subject_root() != root {
            return Ok(None);
        }

        let Some(projections) =
            crate::storage::resolve_storage_projections(request, subject.projections())?
        else {
            return Ok(None);
        };

        crate::storage::reference_path(request, ty, projections.into(), None)
    })?;

    Ok(contracts
        .with_normal_completion(template.normal_completion())
        .with_normal_storage_preservation(template.preserves_storage_on_normal_return())
        .with_effects(template.effects().iter().cloned())
        .with_synchronous_cleanup(template.synchronous_cleanup())
        .with_carriers(template.carriers().iter().cloned())
        .with_returned_input(template.returned_input())
        .with_returned_input_conditions(returned_input_conditions(
            request,
            template.returned_input(),
            |ordinal| {
                assert_eq!(
                    ordinal, 0,
                    "implicit invocation has one receiver-first input"
                );

                Ok(Some(ty))
            },
        )?))
}

pub(super) fn with_reference_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    contracts: BoundCallDependencyContracts,
    template: &bray_symbols::DependencyContractTemplateData,
    retained_inputs: Option<std::collections::BTreeSet<bray_bound_tree::BoundExecutionSite>>,
    mut input_type: impl FnMut(
        &bray_symbols::DependencySubject,
    ) -> Result<
        Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let mut inputs = std::collections::BTreeMap::new();

    for subject in template
        .reference_subjects()
        .collect::<std::collections::BTreeSet<_>>()
    {
        if let Some(prepared) = input_type(subject)? {
            inputs.insert(subject.clone(), prepared);
        }
    }

    let mut references = Vec::with_capacity(template.borrows().len());
    let mut incomplete = std::collections::BTreeSet::new();

    for relation in template.borrows() {
        if let Some((ty, _)) = inputs.get(relation.entry().unwrap_or_else(|| relation.exit()))
            && let Some(reference) = prepare_reference(request, relation, *ty)?
        {
            references.push(reference);
        } else {
            incomplete.insert(relation.exit().clone());
        }
    }

    references.retain(|reference| !incomplete.contains(reference.relation().exit()));

    Ok(contracts
        .with_reference_paths(
            inputs
                .into_iter()
                .map(|(subject, (_, path))| (subject, path))
                .collect(),
        )
        .with_references(references, retained_inputs))
}

pub(crate) fn prepare_reference<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    relation: &bray_symbols::DependencyBorrowRelation,
    mut ty: bray_symbols::TypeId,
) -> Result<Option<BoundCallReference>, CheckerQueryError<C::UpstreamError>> {
    if relation.entry().is_none() {
        return Ok(crate::representation::reference_target_type(request, ty)
            .map(|target| BoundCallReference::new(relation.clone(), false, Some(target))));
    }

    for _ in 0..relation.dereferences() {
        let bray_symbols::TypeData::Borrow { target, .. } =
            *request.semantic_values().type_data(ty)
        else {
            return Ok(None);
        };

        ty = target;
    }

    let kind = crate::representation::reference_kind(request, ty);

    let addressed_type = if relation.target_projections().is_empty() {
        kind.is_none().then_some(ty)
    } else {
        let Some(mut target) = crate::representation::reference_target_type(request, ty) else {
            return Ok(None);
        };

        for projection in relation.target_projections() {
            let projected = crate::storage::projected_value_type(request, target, *projection)?;

            let Some(projected) = *projected.value() else {
                return Ok(None);
            };

            target = projected;
        }

        Some(target)
    };

    // The bound call retains the immutable relation independently of its source template.
    Ok(Some(BoundCallReference::new(
        relation.clone(),
        relation.is_exact()
            && kind == Some(relation.kind())
            && relation.target_projections().is_empty(),
        addressed_type,
    )))
}

fn reference_input_type<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    subject: &bray_symbols::DependencySubject,
) -> Result<
    Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>,
    CheckerQueryError<C::UpstreamError>,
> {
    let ty = match subject.subject_root() {
        DependencySubjectRoot::Receiver => call.receiver().map(|receiver| {
            receiver
                .input_type(request.semantic_values())
                .expect("selected receiver has a valid passed value type")
        }),
        DependencySubjectRoot::Parameter(parameter) => {
            call.arguments().iter().find_map(|argument| match argument {
                bray_bound_tree::SelectedArgument::Explicit {
                    ordinal,
                    conversion,
                    ..
                } if *ordinal == parameter.raw() => Some(conversion.target_type()),
                bray_bound_tree::SelectedArgument::Default { ordinal, ty, .. }
                    if *ordinal == parameter.raw() =>
                {
                    Some(*ty)
                }
                _ => None,
            })
        }
        bray_symbols::DependencySubjectRoot::Result => match call.resolution().result() {
            bray_bound_tree::BoundCallResult::Immediate(ty) => Some(ty),
            bray_bound_tree::BoundCallResult::LazyFuture(future) => Some(future.completion_type()),
        },
        _ => None,
    };

    let Some(ty) = ty else {
        return Ok(None);
    };

    reference_subject_path(request, subject, ty)
}

pub(super) fn reference_subject_path<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    subject: &bray_symbols::DependencySubject,
    ty: bray_symbols::TypeId,
) -> Result<
    Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>,
    CheckerQueryError<C::UpstreamError>,
> {
    let Some(projections) =
        crate::storage::resolve_storage_projections(request, subject.projections())?
    else {
        return Ok(None);
    };

    crate::storage::reference_path(request, ty, projections.into(), None)
}

fn returned_input_conditions<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    input: Option<bray_symbols::ConstantTermId>,
    input_type: impl FnOnce(
        u32,
    )
        -> Result<Option<bray_symbols::TypeId>, CheckerQueryError<C::UpstreamError>>,
) -> Result<Vec<bray_symbols::ConstantTermId>, CheckerQueryError<C::UpstreamError>> {
    use bray_symbols::{
        ConstantBinaryOperation, ConstantProjectionKind, ConstantTermData, ConstantValueData,
        ConstantValueKind, DependencyProjection,
    };

    let Some(mut term) = input else {
        return Ok(Vec::new());
    };

    let values = request.semantic_values();
    let mut projections = Vec::new();

    let ordinal = loop {
        match values.constant_term_data(term).as_ref() {
            ConstantTermData::CallableArgument(ordinal) => break ordinal.raw(),
            ConstantTermData::Projection(projection) => {
                projections.push((projection.subject(), projection.kind()));
                term = projection.subject();
            }
            _ => unreachable!("checked whole-value transport contains only input projections"),
        }
    };

    let Some(mut ty) = input_type(ordinal)? else {
        return Ok(Vec::new());
    };

    let mut conditions = Vec::new();

    for (subject, projection) in projections.into_iter().rev() {
        let condition = match projection {
            ConstantProjectionKind::UnionPayloadField(field) => {
                let variant = request
                    .union_payload_field(field)?
                    .expect("checked transport field retains its variant")
                    .variant();

                Some(ConstantTermData::VariantTest { subject, variant })
            }
            ConstantProjectionKind::NullableValue => {
                let absent = values
                    .intern_constant_value(ConstantValueData::new(
                        ty,
                        ConstantValueKind::NullableAbsent,
                    ))
                    .expect("selected nullable type belongs to the same semantic store");

                let absent = values
                    .intern_constant_term(ConstantTermData::Value(absent))
                    .expect("selected nullable literal belongs to the same semantic store");

                Some(ConstantTermData::Binary {
                    operation: ConstantBinaryOperation::NotEqual,
                    left: subject,
                    right: absent,
                })
            }
            _ => None,
        };

        if let Some(condition) = condition {
            conditions.push(
                values
                    .intern_constant_term(condition)
                    .expect("selected transport condition belongs to the same semantic store"),
            );
        }

        let projection = match projection {
            ConstantProjectionKind::ProductField(field) => {
                DependencyProjection::ProductField(field)
            }
            ConstantProjectionKind::UnionPayloadField(field) => {
                DependencyProjection::UnionPayloadField(field)
            }
            ConstantProjectionKind::TupleElement(index) => {
                DependencyProjection::TupleElement(index)
            }
            ConstantProjectionKind::NullableValue => DependencyProjection::NullableValue,
            ConstantProjectionKind::ArrayElementOrdinal(_) => {
                let data = values.type_data(ty);

                let bray_symbols::TypeData::Array { element, .. } = data.as_ref() else {
                    unreachable!("checked owned element transport retains an array type");
                };

                ty = *element;
                continue;
            }
            _ => unreachable!("checked transport projection is owned"),
        };

        ty = crate::storage::projected_value_type(request, ty, projection)?
            .value()
            .expect("checked returned input retains its projected type");
    }

    Ok(conditions)
}
