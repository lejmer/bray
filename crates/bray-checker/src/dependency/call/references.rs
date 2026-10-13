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
    dependencies: &crate::dependency::defaults::CallResultDependencies,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let template = &dependencies.template;

    let deferred = matches!(
        call.resolution().result(),
        bray_bound_tree::BoundCallResult::LazyFuture(_)
    );

    let contracts = if deferred {
        contracts.with_deferred_execution(
            template.effects().iter().cloned(),
            template.normal_completion(),
        )
    } else {
        contracts
    };

    let witness = request
        .trusted_contracts()
        .and_then(|contracts| contracts.calls.get(&expression.into()))
        .is_some_and(crate::TrustedCallContract::result_guarantees_name_whole_owner);

    let retained_inputs = (witness
        || template
            .borrows()
            .iter()
            .any(|relation| relation.exit().subject_root() == DependencySubjectRoot::Result))
    .then(|| crate::dependency::result_inputs(call, template));

    Ok(
        with_reference_inputs(request, contracts, template, retained_inputs, |subject| {
            reference_input_type(request, call, subject)
        })?
        .with_effects(
            dependencies
                .evaluation_effects
                .iter()
                .chain(template.effects().iter().filter(|_| !deferred))
                .cloned(),
        )
        .with_normal_completion(
            dependencies.evaluation_completes && (deferred || template.normal_completion()),
        ),
    )
}

pub(super) fn with_scoped_references<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    scoped: &bray_bound_tree::SelectedScopedUse,
    occurrence: bray_bound_tree::BoundExecutionSite,
    contracts: BoundCallDependencyContracts,
    template: &bray_symbols::DependencyContractTemplateData,
) -> Result<BoundCallDependencyContracts, CheckerQueryError<C::UpstreamError>> {
    let (_, signature, _) = scoped.invocation(occurrence);

    let (root, ty) = if let Some(receiver) = signature.receiver() {
        let selected = bray_bound_tree::SelectedReceiver::new(
            scoped.initializer(),
            receiver.parameter(),
            receiver.mode(),
            scoped.source_type(),
            receiver.ty(),
        );

        (
            DependencySubjectRoot::Receiver,
            selected
                .input_type(request.semantic_values())
                .expect("checked scoped receiver has a passed value type"),
        )
    } else {
        (
            DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0)),
            signature.parameters()[0].ty(),
        )
    };

    let contracts = with_reference_inputs(request, contracts, template, None, |subject| {
        if subject.subject_root() != root {
            return Ok(None);
        }

        let Some(projections) = crate::storage::storage_projections(request, subject.projections())
        else {
            return Ok(None);
        };

        crate::storage::reference_path(request, ty, projections.into(), None)
    })?;

    Ok(contracts
        .with_normal_completion(template.normal_completion())
        .with_effects(template.effects().iter().cloned()))
}

fn with_reference_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    contracts: BoundCallDependencyContracts,
    template: &bray_symbols::DependencyContractTemplateData,
    retained_inputs: Option<std::collections::BTreeSet<BoundExpressionId>>,
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

    for relation in template.borrows() {
        if let Some((ty, _)) = inputs.get(relation.entry())
            && let Some(reference) = prepare_reference(request, relation, *ty)?
        {
            references.push(reference);
        }
    }

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
        kind == Some(relation.kind()) && relation.target_projections().is_empty(),
        addressed_type,
    )))
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
