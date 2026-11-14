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

    let pure = call.phase_behaviors().invocation().execution_properties()
        .contains(&bray_symbols::ExecutionProperty::Pure);

    let contracts = if deferred {
        let (captures, result) = super::super::cleanup::prepare_future_effects(request, call)?;

        contracts.with_deferred_cleanup(captures, result)
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
        .with_effects(template.effects().iter().filter(|_| !deferred && !pure).cloned())
        .with_synchronous_cleanup(template.synchronous_cleanup())
        .with_carriers(template.carriers().iter().cloned())
        .with_owned_result(template.owned_result().clone())
        .with_normal_completion(deferred || template.normal_completion())
        .with_normal_storage_preservation(
            deferred || pure || template.preserves_storage_on_normal_return(),
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
        || !template.owned_result().is_empty()
        || template
            .borrows()
            .iter()
            .any(|relation| relation.exit().subject_root() == DependencySubjectRoot::Result)
}

pub(super) fn with_hidden_references<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    root: DependencySubjectRoot,
    ty: bray_symbols::TypeId,
    result_type: Option<bray_symbols::TypeId>,
    source: bray_bound_tree::BoundExecutionSite,
    invocation: bray_bound_tree::BoundExecutionSite,
    contracts: BoundCallDependencyContracts,
    template: &bray_symbols::DependencyContractTemplateData,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let retained = result_retains_inputs(request, invocation, template)
        .then(|| std::collections::BTreeSet::from([source]));

    let contracts = with_reference_inputs(request, contracts, template, retained, |subject| {
        let ty = if subject.subject_root() == root {
            ty
        } else if subject.subject_root() == DependencySubjectRoot::Result {
            let Some(ty) = result_type else { return Ok(None); };

            ty
        } else { return Ok(None); };

        reference_subject_path(request, subject, ty)
    })?;

    Ok(contracts
        .with_normal_completion(template.normal_completion())
        .with_normal_storage_preservation(template.preserves_storage_on_normal_return())
        .with_effects(template.effects().iter().cloned())
        .with_synchronous_cleanup(template.synchronous_cleanup())
        .with_carriers(template.carriers().iter().cloned())
        .with_owned_result(template.owned_result().clone())
)
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
        .chain(contracts.deferred_capture_cleanup().into_iter().flatten().chain(contracts.deferred_result_cleanup().into_iter().flatten()).filter_map(bray_symbols::DependencyStorageEffect::subject))
        .collect::<std::collections::BTreeSet<_>>()
    {
        if let Some(prepared) = input_type(subject)? {
            inputs.insert(subject.clone(), prepared);
        }
    }

    // Completion forms reference values independently of their origin relations.
    for root in [DependencySubjectRoot::Result, DependencySubjectRoot::Receiver].into_iter().chain(
        (0..contracts.input_count()).map(|ordinal| {
            DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                u32::try_from(ordinal)
                    .expect("checked invocation arity fits its parameter ordinals"),
            ))
        }),
    ) {
        let subject = bray_symbols::DependencySubject::root(root);

        if !inputs.contains_key(&subject)
            && let Some(prepared) = input_type(&subject)?
            && crate::representation::reference_kind(request, prepared.0).is_some()
        {
            inputs.insert(subject, prepared);
        }
    }

    let receiver = input_type(&bray_symbols::DependencySubject::root(DependencySubjectRoot::Receiver))?.is_some();

    let result_type = input_type(&bray_symbols::DependencySubject::root(DependencySubjectRoot::Result))?.map(|(ty, _)| ty);

    let returned = returned_values(request, template.owned_result().transfers(), result_type, contracts.input_count(), |ordinal| {
        let root = if receiver && ordinal == 0 { DependencySubjectRoot::Receiver }
            else { DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(ordinal - u32::from(receiver))) };

        Ok(input_type(&bray_symbols::DependencySubject::root(root))?.map(|(ty, _)| ty))
    })?;

    let mut references = Vec::with_capacity(template.borrows().len());
    let mut incomplete = std::collections::BTreeSet::new();

    for relation in template.borrows() {
        let subject =
            if relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions {
                relation.exit()
            } else {
                relation.entry().unwrap_or_else(|| relation.exit())
            };

        if relation
            .entry()
            .is_none_or(|entry| inputs.contains_key(entry))
            && let Some((ty, _)) = inputs.get(subject)
            && let Some(reference) = prepare_reference(request, relation, *ty)?
        {
            references.push(reference);
        } else {
            incomplete.insert(relation.exit().clone());
        }
    }

    references.retain(|reference| !incomplete.contains(reference.relation().exit()));

    Ok(contracts
        .with_returned_values(returned)
        .with_reference_subjects(inputs)
        .with_references(references, retained_inputs))
}

pub(crate) fn prepare_reference<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    relation: &bray_symbols::DependencyBorrowRelation,
    mut ty: bray_symbols::TypeId,
) -> Result<Option<BoundCallReference>, CheckerQueryError<C::UpstreamError>> {
    if relation.entry().is_none()
        || relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions
    {
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
    let ty = if subject.subject_root() == DependencySubjectRoot::Result {
        Some(match call.resolution().result() {
            bray_bound_tree::BoundCallResult::Immediate(ty) => ty,
            bray_bound_tree::BoundCallResult::LazyFuture(future) => future.completion_type(),
        })
    } else {
        crate::dependency::result_input(call, subject.subject_root())
            .map(|input| input.ty(request.semantic_values()))
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

fn returned_values<'a, C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    transfers: impl IntoIterator<Item = &'a bray_symbols::DependencyOwnedTransfer>,
    result_type: Option<bray_symbols::TypeId>,
    input_count: usize,
    mut input_type: impl FnMut(
        u32,
    ) -> Result<
        Option<bray_symbols::TypeId>,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<
    std::collections::BTreeMap<
        bray_symbols::DependencyOwnedTransfer,
        bray_bound_tree::BoundReturnedValue,
    >,
    CheckerQueryError<C::UpstreamError>,
> {
    let mut result = std::collections::BTreeMap::new();

    for transfer in transfers {
        let mut output_type = result_type.expect("an owning return retains its instantiated result type");

        for projection in transfer.output() {
            output_type = crate::storage::projected_owned_type(request, output_type, *projection)?
                .value().expect("checked owning return retains its output component type");
        }

        let borrowed = matches!(request.semantic_values().type_data(output_type).as_ref(), bray_symbols::TypeData::Borrow { .. });
        let mut conditions = Vec::new();

        let (transferred, ty) = returned_input_projection_conditions(
            request,
            transfer.input(),
            input_count,
            &mut input_type,
        )?;

        for condition in transferred {
            let condition = if let Some(guard) = transfer.condition() {
                let values = request.semantic_values();

                let inactive = values
                    .intern_constant_term(bray_symbols::ConstantTermData::Unary {
                        operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                        operand: guard,
                    })
                    .expect("checked result guard belongs to the same semantic store");

                values
                    .intern_constant_term(bray_symbols::ConstantTermData::Binary {
                        operation: bray_symbols::ConstantBinaryOperation::LogicalOr,
                        left: inactive,
                        right: condition,
                    })
                    .expect("checked transport condition belongs to the same semantic store")
            } else {
                condition
            };

            conditions.push(condition);
        }

        conditions.sort_unstable();
        conditions.dedup();

        result.insert(transfer.clone(), bray_bound_tree::BoundReturnedValue::new(
            borrowed, conditions, crate::storage::observation::returned_observations(request, ty)?,
        ));
    }

    Ok(result)
}

fn returned_input_projection_conditions<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    mut term: bray_symbols::ConstantTermId,
    input_count: usize,
    input_type: &mut impl FnMut(
        u32,
    ) -> Result<
        Option<bray_symbols::TypeId>,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<(Vec<bray_symbols::ConstantTermId>, bray_symbols::TypeId), CheckerQueryError<C::UpstreamError>> {
    use bray_symbols::{
        ConstantBinaryOperation, ConstantProjectionKind, ConstantTermData, ConstantValueData,
        ConstantValueKind,
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

    assert!(
        usize::try_from(ordinal).is_ok_and(|ordinal| ordinal < input_count),
        "owned transfer refers to an actual input"
    );

    let mut ty = input_type(ordinal)?
        .expect("checked owned transfer retains its actual input type");

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

        ty = crate::storage::projected_owned_type(request, ty, projection)?
            .value()
            .unwrap_or_else(|| panic!(
                "checked returned input has no projected type: unit={:?}, input={ordinal}, subject={:?}, type={:?}, projection={projection:?}",
                request.unit().unit(), values.constant_term_data(subject), values.type_data(ty),
            ));
    }

    Ok((conditions, ty))
}
