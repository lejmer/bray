use bray_symbols::ConstantTermData;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundCallDependencyContracts, BoundCallReference, BoundCallStorage, BoundExpressionId, SelectedCall,
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

    let observational = is_intrinsically_observational(call);

    let contracts = if deferred {
        let inputs = call.arguments().iter().map(bray_bound_tree::SelectedInput::Argument)
            .chain(call.receiver().map(bray_bound_tree::SelectedInput::Receiver))
            .map(|input| (input.root(), input.ty(request.semantic_values())));

        let (captures, result) = super::super::cleanup::prepare_future_effects(
            request, inputs, call.resolution().result().completion_type(),
        )?;

        let callee = call.resolution().declaration_source().map(|instance| {
            let values = request.semantic_values();
            let signature = call.callable_type();
            let receiver = call.receiver().map(|receiver| bray_bound_tree::SelectedInput::Receiver(receiver).ty(values));
            let requirement = call.resolution().trait_dispatch().map(|dispatch| request.context().dispatch_requirement(dispatch)).transpose()?;

            Ok::<_, CheckerQueryError<C::UpstreamError>>(bray_symbols::CapturedCallable {
                instance: values.intern_callable_instance(instance).expect("selected deferred instance belongs to its semantic store"),
                signature, receiver, requirement, dispatch: call.dispatch_implementation(), witnesses: call.resolution().implementation_witnesses().into(),
            })
        }).transpose()?;

        contracts.with_deferred_cleanup(captures, result)
            .with_deferred_execution(
                template.effect_outcomes().clone(),
                template.normal_completion(), callee,
            )
    } else {
        contracts
    };

    let retained_inputs = result_retains_inputs(request, expression.into(), template)
        .then(|| crate::dependency::result_inputs(expression, call, template));

    let effects = if deferred || observational {
        bray_symbols::DependencyEffectOutcomes::new([])
    } else {
        template.effect_outcomes().clone()
    };

    Ok(
        with_reference_inputs(request, contracts, template, retained_inputs, |subject| {
            reference_input_type(request, call, subject)
        })?
        .with_effects(effects)
        .with_synchronous_cleanup(template.synchronous_cleanup())
        .with_normal_completion(deferred || template.normal_completion()),
    )
}

fn is_intrinsically_observational(call: &SelectedCall) -> bool {
    matches!(call.target(), bray_bound_tree::BoundCallableTarget::Predicate(_))
        || call.constness() == bray_symbols::CallableConstness::Constant
        || matches!(
            call.implementation_hook(),
            Some(
                bray_compiler_known::ImplementationHook::SequenceLength
                    | bray_compiler_known::ImplementationHook::SequenceIsEmpty
                    | bray_compiler_known::ImplementationHook::NumericTruncate
                    | bray_compiler_known::ImplementationHook::NumericTruncationSupported
                    | bray_compiler_known::ImplementationHook::NumericTruncationDomain
            )
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
                    .any(|subject| subject.subject.observes_completion_result())
        })
        || template.captured_values().iter().any(|row| !row.carriers.is_empty() || !row.ownership.is_empty())
        || template.captured_values().iter().flat_map(|outcome| outcome.bindings.iter()).any(|(_, value)| value.inputs().next().is_some())
        || template.captured_values().iter().flat_map(|outcome| outcome.references.iter())
            .any(|relation| relation.exit().subject_root() == DependencySubjectRoot::Result)
}

pub(super) fn with_hidden_references<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    root: DependencySubjectRoot,
    ty: bray_symbols::TypeId,
    result_type: bray_symbols::TypeId,
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
            result_type
        } else {
            let Some(ty) = crate::storage::static_subject_type(request, subject.subject_root())? else {
                return Ok(None);
            };

            ty
        };

        reference_subject_path(request, subject, ty)
    })?;

    Ok(contracts
        .with_normal_completion(template.normal_completion())
        .with_effects(template.effect_outcomes().clone())
        .with_synchronous_cleanup(template.synchronous_cleanup())
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
    let cleanup = contracts.deferred_capture_cleanup().into_iter().flatten()
        .chain(contracts.deferred_result_cleanup().into_iter().flatten());

    let roots = [DependencySubjectRoot::Result, DependencySubjectRoot::Receiver].into_iter().chain(
        (0..contracts.input_count()).map(|ordinal| {
            DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(
                u32::try_from(ordinal)
                    .expect("checked invocation arity fits its parameter ordinals"),
            ))
        }),
    );

    let execution = contracts.deferred_template().into_iter().flat_map(|contract| contract.requirement_subjects());
    let storage = prepare_call_storage(request, template, cleanup, execution, roots, None, &mut input_type)?;

    Ok(contracts
        .with_captured_outcomes(template)
        .with_storage(storage)
        .with_retained_inputs(retained_inputs))
}

pub(crate) fn prepare_call_storage<'a, C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    template: &'a bray_symbols::DependencyContractTemplateData,
    additional_effects: impl IntoIterator<Item = &'a bray_symbols::DependencyStorageEffect>,
    additional_subjects: impl IntoIterator<Item = &'a bray_symbols::DependencySubject>,
    addressable_roots: impl IntoIterator<Item = DependencySubjectRoot>,
    receiver_input_type: Option<bray_symbols::TypeId>,
    mut input: impl FnMut(
        &bray_symbols::DependencySubject,
    ) -> crate::CheckerQueryResult<Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>, C::UpstreamError>,
) -> crate::CheckerQueryResult<BoundCallStorage, C::UpstreamError> {
    let effects = template.effects().iter()
        .chain(template.cell_changes())
        .chain(template.captured_values().iter().flat_map(|row| row.effects.iter()))
        .chain(template.failure_outcomes().into_iter().flatten().flat_map(|row| row.effects.as_deref().into_iter().flatten()))
        .chain(additional_effects)
        .collect::<std::collections::BTreeSet<_>>();

    let mut subjects = std::collections::BTreeMap::new();

    for subject in template.reference_subjects().chain(additional_subjects)
        .chain(effects.iter().copied().flat_map(bray_symbols::DependencyStorageEffect::subjects))
        .collect::<std::collections::BTreeSet<_>>() {
        if let Some(prepared) = input(subject)? {
            // The prepared path is indexed by its portable subject.
            subjects.insert(subject.clone(), prepared);
        }
    }

    // These roots can produce reference values independently of origin relations.
    for root in addressable_roots {
        let subject = bray_symbols::DependencySubject::root(root);

        if !subjects.contains_key(&subject)
            && let Some(prepared) = input(&subject)?
            && crate::representation::reference_kind(request, prepared.0).is_some()
        {
            subjects.insert(subject, prepared);
        }
    }

    let mut references = Vec::new();
    let types = |subject: &bray_symbols::DependencySubject| subjects.get(subject).map(|(ty, _)| *ty);
    let rows = template.captured_values();
    let failure_rows = template.failure_outcomes().into_iter().flatten();

    for relation in rows.iter().flat_map(|outcome| outcome.references.iter())
        .chain(failure_rows.clone().flat_map(|outcome| outcome.references.iter()))
        .map(|relation| relation.clone().with_exactness(false)).collect::<std::collections::BTreeSet<_>>() {
        references.extend(prepare_reference(request, &relation, &types)?);
    }

    let stored_paths = prepare_stored_value_paths(
        request.semantic_values(),
        rows.iter().flat_map(|outcome| outcome.stored_values.iter())
            .chain(failure_rows.flat_map(|outcome| outcome.stored_values.iter())),
        |subject| subjects.get(subject).cloned(),
    );

    let effect_paths = prepare_effect_paths(request, effects, |subject| {
        if subject.subject_root() == DependencySubjectRoot::Receiver && subject.projections().is_empty() {
            // The reached receiver supplied to cleanup has already followed
            // the first borrow. The effect still declares its suffix from that input.
            receiver_input_type.or_else(|| types(subject))
        } else {
            types(subject)
        }
    })?;

    let write_paths = prepare_write_paths(request.semantic_values(), template, &subjects);
    let returned = returned_values(request, rows, &mut input)?;

    Ok(BoundCallStorage::new(subjects, references, stored_paths)
        .with_effect_paths(effect_paths).with_write_paths(write_paths).with_returned_values(returned))
}

fn prepare_write_paths(
    values: &bray_symbols::SemanticValueStore,
    template: &bray_symbols::DependencyContractTemplateData,
    subjects: &std::collections::BTreeMap<bray_symbols::DependencySubject, (bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>,
) -> std::collections::BTreeMap<bray_symbols::DependencySubject, (bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)> {
    use bray_bound_tree::StorageReferenceStep::ReadBorrow;
    use bray_symbols::{BorrowKind, TypeData};

    let writes = template.captured_values().iter().flat_map(|row| row.bindings.iter())
        .chain(template.failure_outcomes().into_iter().flatten().flat_map(|row| row.writes.iter()))
        .map(|(subject, _)| subject).filter(|subject| subject.subject_root() != DependencySubjectRoot::Result)
        .collect::<std::collections::BTreeSet<_>>();

    writes.into_iter().filter_map(|subject| {
        let (ty, path) = subjects.get(subject)?;

        if path.projections().is_empty() {
            // Writing to the input itself replaces its referent. A projection
            // can instead replace a stored descriptor without reading that slot.
            let TypeData::Borrow { kind: BorrowKind::Mutable, target } = *values.type_data(*ty) else { return None; };

            return Some((subject.clone(), (target, path.with_terminal_borrows(&[(BorrowKind::Mutable, target)]))));
        }

        (matches!(path.steps().first(), Some(ReadBorrow { mode: BorrowKind::Mutable, .. }))
            && !path.steps().iter().any(|step| matches!(step, ReadBorrow { mode: BorrowKind::Shared, .. })))
            .then(|| (subject.clone(), (*ty, path.clone())))
    }).collect()
}

pub(crate) fn prepare_effect_paths<'a, C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: impl IntoIterator<Item = &'a bray_symbols::DependencyStorageEffect>,
    subject_type: impl Fn(&bray_symbols::DependencySubject) -> Option<bray_symbols::TypeId>,
) -> crate::CheckerQueryResult<std::collections::BTreeMap<bray_symbols::DependencyStorageEffect,
    bray_bound_tree::StorageReferencePath>, C::UpstreamError> {
    let mut paths = std::collections::BTreeMap::new();

    for effect in effects.into_iter().flat_map(bray_symbols::DependencyStorageEffect::routes) {
        let bray_symbols::DependencyStorageEffect::Indirect { reference, projections, reads, .. } = effect else { continue; };

        if (projections.is_empty() && reads.is_empty()) || paths.contains_key(effect) { continue; }

        let Some(projections) = crate::storage::resolve_effect_projections(request, projections)? else { continue; };

        if reads.is_empty() && bray_bound_tree::StorageProjection::remains_in_allocation(&projections)
            && !projections.contains(&bray_bound_tree::StorageProjection::AddressTarget)
        {
            paths.insert(effect.clone(), bray_bound_tree::StorageReferencePath::new(projections.into(), [].into()));
            continue;
        }

        let Some(ty) = subject_type(reference).and_then(|ty| crate::representation::reference_target_type(request, ty)) else { continue; };

        if let Some((_, path)) = crate::storage::effect_reference_path(request, ty, projections.into(), reads)? {
            paths.insert(effect.clone(), path);
        }
    }

    Ok(paths)
}

fn prepare_stored_value_paths<'a>(
    values: &bray_symbols::SemanticValueStore,
    stored: impl IntoIterator<Item = &'a bray_symbols::DependencyStoredValue>,
    input: impl Fn(&bray_symbols::DependencySubject) -> Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>,
) -> std::collections::BTreeMap<bray_symbols::DependencyStoredValuePath,
    (bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)> {
    stored.into_iter().flat_map(|stored| std::iter::once(stored.destination()).chain(stored.sources()))
        .collect::<std::collections::BTreeSet<_>>().into_iter().filter_map(|endpoint| {
            let (mut ty, path) = input(endpoint.subject())?;

            let mut targets = Vec::new();

            for _ in 0..endpoint.dereferences() {
                let data = values.type_data(ty);

                let bray_symbols::TypeData::Borrow { kind, target } = data.as_ref() else { return None; };

                ty = *target;
                targets.push((*kind, ty));
            }

            Some((endpoint.clone(), (ty, path.with_terminal_borrows(&targets))))
        }).collect()
}

fn prepare_reference<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    relation: &bray_symbols::DependencyBorrowRelation,
    input_type: &impl Fn(&bray_symbols::DependencySubject) -> Option<bray_symbols::TypeId>,
) -> Result<Option<BoundCallReference>, CheckerQueryError<C::UpstreamError>> {
    let Some(entry) = relation.entry() else {
        return Ok(input_type(relation.exit()).and_then(|ty| crate::representation::reference_target_type(request, ty))
            .map(|target| BoundCallReference::new(relation.clone(), false, Some(target))));
    };

    let Some(mut ty) = input_type(entry) else { return Ok(None); };

    let mut reads = Vec::new();

    for _ in 0..relation.dereferences() {
        let bray_symbols::TypeData::Borrow { kind, target } =
            *request.semantic_values().type_data(ty)
        else {
            return Ok(None);
        };

        ty = target;
        reads.push((kind, target));
    }

    let reads = (!reads.is_empty()).then(|| bray_bound_tree::StorageReferencePath::new([].into(), [].into())
        .with_terminal_borrows(&reads));

    let kind = crate::representation::reference_kind(request, ty);
    let mut target_path = None;

    let projected = if relation.target_projections().is_empty() {
        (kind.is_none() || entry.subject_root().is_static()).then_some(ty)
    } else {
        let Some(target) = crate::representation::reference_target_type(request, ty) else { return Ok(None); };

        let Some(projections) = crate::storage::resolve_storage_projections(request, relation.target_projections())?
        else { return Ok(None); };

        let Some((projected, path)) = crate::storage::effect_reference_path(request, target, projections.into(), &[])?
        else { return Ok(None); };

        target_path = Some(path);

        Some(projected)
    };

    let bounded = relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions;

    let addressed = if bounded {
        let Some(target) = input_type(relation.exit()).and_then(|ty| crate::representation::reference_target_type(request, ty))
        else { return Ok(None); };

        Some(target)
    } else { projected };

    let exposes_borrow = relation.kind() == bray_symbols::DependencyReferenceKind::Address
        && kind == Some(bray_symbols::DependencyReferenceKind::Borrow);

    Ok(Some(BoundCallReference::new(
        relation.clone(),
        !bounded && !entry.subject_root().is_static() && kind == Some(relation.kind()) && target_path.is_none(),
        addressed,
    ).with_routes(reads, target_path, exposes_borrow)))
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
            .or(crate::storage::static_subject_type(request, subject.subject_root())?)
    };

    let Some(ty) = ty else {
        return Ok(None);
    };

    reference_subject_path(request, subject, ty)
}

pub(crate) fn reference_subject_path<C: CheckerRequestContext + ?Sized>(
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

fn returned_values<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    rows: &[bray_symbols::CapturedValueOutcome],
    input: &mut impl FnMut(&bray_symbols::DependencySubject) -> crate::CheckerQueryResult<
        Option<(bray_symbols::TypeId, bray_bound_tree::StorageReferencePath)>, C::UpstreamError,
    >,
) -> Result<
    Vec<(bray_symbols::DependencyOwnedTransfer, bray_bound_tree::BoundReturnedValue)>,
    CheckerQueryError<C::UpstreamError>,
> {
    let mut result = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut transfers = rows.iter().flat_map(|row| row.ownership.transfers()).peekable();

    if transfers.peek().is_none() { return Ok(result); }

    let Some((result_type, _)) = input(&bray_symbols::DependencySubject::root(DependencySubjectRoot::Result))? else {
        // Implicit cleanup binds its receiver and discards any normal result.
        return Ok(result);
    };

    let receiver = input(&bray_symbols::DependencySubject::root(DependencySubjectRoot::Receiver))?.is_some();

    let mut input_type = |ordinal| {
        let root = if receiver && ordinal == 0 {
            DependencySubjectRoot::Receiver
        } else {
            DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(ordinal - u32::from(receiver)))
        };

        Ok(input(&bray_symbols::DependencySubject::root(root))?.map(|(ty, _)| (root, ty)))
    };

    for transfer in transfers {
        if !seen.insert((transfer.input(), transfer.output())) { continue; }

        let mut output_type = result_type;

        for projection in transfer.output() {
            output_type = crate::storage::projected_owned_type(request, output_type, *projection)?
                .value().expect("checked owning return retains its output component type");
        }

        let borrowed = matches!(request.semantic_values().type_data(output_type).as_ref(), bray_symbols::TypeData::Borrow { .. });

        let (root, path) = returned_input_path(
            request,
            transfer.input(),
            (!borrowed).then_some(output_type),
            &mut input_type,
        )?;

        result.push((transfer.clone(), bray_bound_tree::BoundReturnedValue::new(root, borrowed, path)));
    }

    Ok(result)
}

fn returned_input_path<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    mut term: bray_symbols::ConstantTermId,
    reached_type: Option<bray_symbols::TypeId>,
    input_type: &mut impl FnMut(
        u32,
    ) -> Result<
        Option<(DependencySubjectRoot, bray_symbols::TypeId)>,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<(DependencySubjectRoot, bray_bound_tree::StorageReferencePath), CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();
    let mut projections = Vec::new();

    let ordinal = loop {
        match values.constant_term_data(term).as_ref() {
            ConstantTermData::CallableArgument(ordinal) => break ordinal.raw(),
            ConstantTermData::Projection(projection) => {
                projections.push(projection.kind());
                term = projection.subject();
            }
            _ => unreachable!("checked whole-value transport contains only input projections"),
        }
    };

    let (root, ty) = input_type(ordinal)?
        .expect("checked owned transfer retains its actual input");

    projections.reverse();

    let path = crate::storage::resolve_owned_storage_projections(request, &projections)?;

    let (_, input_path) = crate::storage::reference_path(request, ty, path.into(), reached_type)?
        .expect("checked owning return retains its physical input route");

    Ok((root, input_path))
}
