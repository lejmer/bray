use std::collections::BTreeSet;

use bray_symbols::{DependencyMutationKind as Kind, DependencyProjection as Projection, DependencyStorageEffect as Effect, DependencySubject, DependencySubjectRoot, TypeData, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(in crate::dependency) fn prepare_future_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    inputs: impl IntoIterator<Item = (DependencySubjectRoot, TypeId)>,
    result: TypeId,
) -> Result<(Vec<Effect>, Vec<Effect>), CheckerQueryError<C::UpstreamError>> {
    let mut captures = Vec::new();

    for (root, ty) in inputs {
        captures.extend(resolve_effects(request, &[Effect::Direct { addressed: true,
            subject: DependencySubject::root(root), kind: Kind::Dispose(ty), access_type: Some(ty),
        }])?);
    }

    let result = resolve_effects(request, &[Effect::Direct { addressed: true,
        subject: DependencySubject::root(DependencySubjectRoot::Result), kind: Kind::Dispose(result), access_type: Some(result),
    }])?;

    Ok((captures, result))
}

pub(crate) fn resolve_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &[Effect],
) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>> {
    resolve_effects_using(request, effects, bray_symbols::DependencyContractTemplateData::effects)
}

pub(crate) fn resolve_contract_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    template: bray_symbols::DependencyContractTemplateData,
) -> Result<bray_symbols::DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let possible = resolve_effects(request, template.effects())?;
    let mut normal = std::collections::BTreeMap::<&Effect, Vec<Effect>>::new();
    let mut outcomes = Vec::new();

    for row in template.captured_values() {
        let mut effects = Vec::new();

        for effect in row.effects.iter() {
            let resolved = match normal.entry(effect) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => entry.insert(resolve_effects_using(
                    request, std::slice::from_ref(effect), bray_symbols::DependencyContractTemplateData::normal_effects)?),
            };

            effects.extend(resolved.iter().cloned());
        }

        // Layout, reports and value bindings belong to the same specialized exit.
        outcomes.push(row.clone().with_effects(effects));
    }

    if outcomes.is_empty() && template.normal_completion() {
        let effects = resolve_effects_using(request, template.normal_effects(), bray_symbols::DependencyContractTemplateData::normal_effects)?;

        if effects != possible {
            outcomes.push(bray_symbols::CapturedValueOutcome::new(None, []).with_effects(effects));
        }
    }

    let possible = bray_base::sorted_unique_shared_slice(possible);

    let failures = template.failure_outcomes().map(|rows| {
        let mut prefixes = std::collections::BTreeMap::from([(template.effects(), std::sync::Arc::clone(&possible))]);

        rows.iter().map(|row| {
            let effects = row.effects.as_deref().map(|prefix| {
                let expanded = match prefixes.entry(prefix) {
                    std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                    std::collections::btree_map::Entry::Vacant(entry) => entry.insert(bray_base::sorted_unique_shared_slice(resolve_effects(request, prefix)?)),
                };

                Ok::<_, CheckerQueryError<C::UpstreamError>>(std::sync::Arc::clone(expanded))
            }).transpose()?;

            Ok(bray_symbols::CapturedFailureOutcome { effects, ..row.clone() })
        }).collect::<Result<Vec<_>, _>>()
    }).transpose()?;

    Ok(template.with_captured_values(outcomes).with_failure_outcomes(failures).with_possible_effects(possible.iter().cloned()))
}

fn resolve_effects_using<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &[Effect],
    select: fn(&bray_symbols::DependencyContractTemplateData) -> &[Effect],
) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>> {
    expand_effects(request, effects, &mut |callable| {
        let template = request.callable_result_dependencies(callable.definition().callable_symbol())?;

        let template = request.semantic_values().substitute_dependency_contract(template, callable.substitution())
            .expect("selected lifecycle substitution retains its checked dependency contract");

        Ok(select(&request.semantic_values().dependency_contract_template_data(template)).to_vec())
    })
}

pub(crate) fn cleanup_callees<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    types: impl IntoIterator<Item = TypeId>,
) -> Result<BTreeSet<bray_symbols::CallableSymbolId>, CheckerQueryError<C::UpstreamError>> {
    let effects = types.into_iter().collect::<BTreeSet<_>>().into_iter().map(|ty| Effect::Direct { addressed: true,
        subject: DependencySubject::root(DependencySubjectRoot::Receiver),
        kind: Kind::Dispose(ty), access_type: Some(ty),
    }).collect::<Vec<_>>();

    let mut callees = BTreeSet::new();

    // Discover the structural edges before any source summary is requested.
    // Each selected callable's body owns discovery of its own outgoing edges.
    expand_effects(request, &effects, &mut |callable| {
        callees.insert(callable.definition().callable_symbol());

        Ok(Vec::new())
    })?;

    Ok(callees)
}

fn expand_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &[Effect],
    resolve: &mut impl FnMut(bray_symbols::CallableInstanceData) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>>,
) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>> {
    let mut shapes = crate::asynchronous::CleanupShapeResolver::new(request);
    let mut resolved = Vec::new();
    let mut active = BTreeSet::new();

    for effect in effects {
        expand(request, &mut shapes, effect, &mut active, &mut resolved, resolve)?;
    }

    Ok(resolved)
}

fn expand<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    effect: &Effect,
    active: &mut BTreeSet<TypeId>,
    resolved: &mut Vec<Effect>,
    resolve: &mut impl FnMut(bray_symbols::CallableInstanceData) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    let Some((kind @ (Kind::Dispose(ty) | Kind::Cleanup(ty)), _)) = mutation(effect) else {
        // The result owns its effects while sharing immutable projection paths.
        resolved.push(effect.clone());

        return Ok(());
    };

    if shapes.requirement(ty)? == bray_bound_tree::AsyncStorageCleanupRequirement::None {
        return Ok(());
    }

    if matches!(kind, Kind::Cleanup(_)) {
        return expand_region(request, shapes, effect, ty, active, resolved, resolve);
    }

    if !active.insert(ty) {
        resolved.push(effect.clone());

        return Ok(());
    }

    let parts = crate::asynchronous::full_cleanup_parts(shapes, ty, request.unit().key().source(), false)?;

    if parts.value().is_empty() {
        resolved.push(effect.clone());
        active.remove(&ty);

        return Ok(());
    }

    for (ty, part) in parts.value() {
        let scope = part.as_ref().and_then(|part| {
            part.projections().iter().map(|projection| match projection.projection() {
                bray_bound_tree::StorageCleanupProjectionKind::Component(component) => crate::storage::dependency_projection(component),
                bray_bound_tree::StorageCleanupProjectionKind::OwnedTarget(_) => Some(Projection::OwnedTarget),
                bray_bound_tree::StorageCleanupProjectionKind::ArrayElements(_) => Some(Projection::AnyElement),
                _ => None,
            }).collect::<Option<Vec<_>>>()
        });

        let scope = match scope {
            Some(path) => project(effect, &path, Kind::Dispose(*ty), None),
            None if part.is_some() => unbounded_scope(effect, Kind::Dispose(*ty), None),
            None => replace_kind(effect, Kind::Dispose(*ty), None),
        };

        if part.is_none() || matches!(request.semantic_values().type_data(*ty).as_ref(), TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) | TypeData::TypeValuedMemberProjection { .. })
            && part.as_ref().is_some_and(|part| part.release().is_none())
        {
            resolved.push(scope);
            continue;
        }

        let mut children = Vec::new();

        append_part(request, shapes, *ty, part.as_ref().expect("concrete represented cleanup part"), &scope, &mut children, resolve)?;

        for child in children {
            expand(request, shapes, &child, active, resolved, resolve)?;
        }
    }

    active.remove(&ty);

    Ok(())
}

fn expand_region<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    region: &Effect,
    ty: TypeId,
    active: &mut BTreeSet<TypeId>,
    resolved: &mut Vec<Effect>,
    resolve: &mut impl FnMut(bray_symbols::CallableInstanceData) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    // Expand the actual value layout once. Its receiver-relative projections
    // cannot be applied to a region that merely bounds possible cleanup effects.
    let value = Effect::Direct { addressed: true,
        subject: DependencySubject::root(DependencySubjectRoot::Result),
        kind: Kind::Dispose(ty), access_type: Some(ty),
    };

    let mut effects = Vec::new();

    expand(request, shapes, &value, active, &mut effects, resolve)?;

    for effect in effects {
        let kind = match effect.retirement_parts().0 {
            Effect::Direct { subject, kind, .. } if subject.subject_root() == DependencySubjectRoot::Result => Some(*kind),
            Effect::Indirect { reference, kind, .. } if reference.subject_root() == DependencySubjectRoot::Result => Some(*kind),
            _ => None,
        };

        resolved.push(match kind {
            Some(Kind::Dispose(ty)) => replace_kind(region, Kind::Cleanup(ty), None).into_region(),
            Some(kind) => replace_kind(region, kind, None).into_region(),
            None => effect,
        });
    }

    Ok(())
}

fn append_part<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    shapes: &crate::asynchronous::CleanupShapeResolver<'_, C>,
    ty: TypeId,
    part: &bray_bound_tree::StorageCleanupPart,
    scope: &Effect,
    effects: &mut Vec<Effect>,
    resolve: &mut impl FnMut(bray_symbols::CallableInstanceData) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    if let Some(fields) = crate::representation::raw_buffer_fields(request, ty)? {
        let backing = if matches!(scope, Effect::Direct { .. }) {
            project(scope, &[Projection::OwnedTarget], Kind::Retire, None)
        } else {
            follow(scope, &[Projection::ProductField(fields.pointer)], Kind::Retire, None)
        };

        let elements = project(&backing, &[Projection::AnyElement], Kind::Dispose(fields.element), Some(fields.element));

        effects.push(elements);
        effects.push(backing);

        return Ok(());
    }

    if let Some(release) = part.release() {
        return append_call(release.callable(), scope, effects, resolve);
    }

    if !shapes.has_source_lifecycle(ty, Some(part))? {
        effects.push(scope.clone());

        return Ok(());
    }

    for slot in [bray_symbols::TypeAssociatedLifecycleSlot::Finalizer, bray_symbols::TypeAssociatedLifecycleSlot::Destructor] {
        let selected = request.context().lifecycle_callable(ty, slot)?;

        assert!(!selected.diagnostics().has_errors(), "cleanup of {:?} has no checked {slot:?}: {:?}",
            request.semantic_values().type_data(ty), selected.diagnostics());

        if let Some((callable, _)) = selected.value() {
            append_call(*callable, scope, effects, resolve)?;
        }
    }

    Ok(())
}

fn append_call<E>(
    callable: bray_symbols::CallableInstanceData,
    scope: &Effect,
    effects: &mut Vec<Effect>,
    resolve: &mut impl FnMut(bray_symbols::CallableInstanceData) -> Result<Vec<Effect>, CheckerQueryError<E>>,
) -> Result<(), CheckerQueryError<E>> {
    let selected = resolve(callable)?;

    effects.extend(selected.iter().map(|effect| {
        let (effect, discharged) = effect.retirement_parts();

        let projected = match effect {
        Effect::Direct { addressed, subject, kind, access_type } if cleanup_receiver(subject.subject_root()) => {
            let projected = project(scope, subject.projections(), *kind, *access_type);

            if *addressed { projected } else { projected.into_region() }
        }
        Effect::Indirect { addressed, reference, reads, projections, kind, access_type } if cleanup_receiver(reference.subject_root()) => {
            // A selected lifecycle receives the addressed object as its receiver.
            // Reading a field of that object is an additional descriptor load.
            let mut target = if reference.projections().is_empty() {
                scope.clone()
            } else {
                follow(scope, reference.projections(), *kind, *access_type)
            };

            let mut start = 0;

            for end in reads.iter().map(|read| *read as usize) {
                target = follow(&target, &projections[start..end], *kind, *access_type);
                start = end;
            }

            let target = project(&target, &projections[start..], *kind, *access_type);

            if *addressed { target } else { target.into_region() }
        }
        // A selected destructor cannot retain its own terminal receiver address.
        // Unknown effects keep the supplied receiver scope instead of exposing it.
        Effect::Unknown | Effect::Escape if !matches!(scope, Effect::UnboundedMutation { .. }) => scope.clone(),
        effect => effect.clone(),
        };

        if projected.is_addressed_retirement() && !discharged.is_empty() {
            projected.with_retirement_discharges(discharged.into())
        } else {
            projected
        }
    }));

    Ok(())
}

fn mutation(effect: &Effect) -> Option<(Kind, Option<TypeId>)> {
    match effect.retirement_parts().0 {
        Effect::Direct { kind, access_type, .. } | Effect::Indirect { kind, access_type, .. } | Effect::UnboundedMutation { kind, access_type, .. } => Some((*kind, *access_type)),
        _ => None,
    }
}

fn replace_kind(scope: &Effect, kind: Kind, access_type: Option<TypeId>) -> Effect {
    project(scope, &[], kind, access_type)
}

fn unbounded_scope(scope: &Effect, kind: Kind, access_type: Option<TypeId>) -> Effect {
    // Losing a component route does not give ambient cleanup access to private caller storage.
    Effect::UnboundedMutation {
        kind, access_type, ambient: matches!(scope, Effect::UnboundedMutation { ambient: true, .. }),
    }
}

fn follow(scope: &Effect, fields: &[Projection], kind: Kind, access_type: Option<TypeId>) -> Effect {
    match project(scope, fields, kind, access_type) {
        Effect::Direct { addressed: true, subject, .. } => Effect::Indirect {
            reference: subject, reads: [].into(), projections: [].into(), addressed: true, kind, access_type,
        },
        Effect::Indirect { addressed: true, reference, reads, projections, .. } => {
            let index = u32::try_from(projections.len()).expect("checked effect path fits the interface projection count");

            Effect::Indirect { addressed: true, reference, reads: reads.iter().copied().chain([index]).collect(),
                projections, kind, access_type }
        }
        _ => unbounded_scope(scope, kind.region_kind(), access_type),
    }
}

fn project(scope: &Effect, suffix: &[Projection], kind: Kind, access_type: Option<TypeId>) -> Effect {
    match scope {
        Effect::Direct { subject, addressed, .. } => Effect::Direct { addressed: *addressed,
            subject: DependencySubject::new(subject.subject_root(), subject.projections().iter().copied().chain(suffix.iter().copied())),
            kind, access_type,
        },
        Effect::Indirect { reference, reads, projections, addressed, .. } => Effect::Indirect { addressed: *addressed, reads: std::sync::Arc::clone(reads),
            reference: reference.clone(),
            projections: projections.iter().copied().chain(suffix.iter().copied()).collect(), kind, access_type,
        },
        Effect::UnboundedMutation { ambient, .. } => Effect::UnboundedMutation { kind, access_type, ambient: *ambient },
        _ => panic!("cleanup retains its mutation location: {scope:?}"),
    }
}

fn cleanup_receiver(root: DependencySubjectRoot) -> bool {
    match root { DependencySubjectRoot::Receiver => true, DependencySubjectRoot::Parameter(ordinal) => ordinal.raw() == 0, _ => false }
}
