use std::collections::BTreeSet;

use bray_symbols::{DependencyMutationKind as Kind, DependencyProjection as Projection, DependencyStorageEffect as Effect, DependencySubject, DependencySubjectRoot, TypeData, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(in crate::dependency) fn prepare_future_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &bray_bound_tree::SelectedCall,
) -> Result<(Vec<Effect>, Vec<Effect>), CheckerQueryError<C::UpstreamError>> {
    let mut captures = Vec::new();

    let inputs = call.arguments().iter().map(bray_bound_tree::SelectedInput::Argument)
        .chain(call.receiver().map(bray_bound_tree::SelectedInput::Receiver));

    for input in inputs {
        captures.extend(resolve_effects(request, &[Effect::Direct {
            subject: DependencySubject::root(input.root()), kind: Kind::Dispose(input.ty(request.semantic_values())), access_type: Some(input.ty(request.semantic_values())),
        }])?);
    }

    let ty = call.resolution().result().completion_type();

    let result = resolve_effects(request, &[Effect::Direct {
        subject: DependencySubject::root(DependencySubjectRoot::Result), kind: Kind::Dispose(ty), access_type: Some(ty),
    }])?;

    Ok((captures, result))
}

pub(crate) fn resolve_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &[Effect],
) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>> {
    resolve_effects_using(request, effects, bray_symbols::DependencyContractTemplateData::effects)
}

pub(crate) fn resolve_effect_outcomes<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &bray_symbols::DependencyEffectOutcomes<Effect>,
) -> Result<bray_symbols::DependencyEffectOutcomes<Effect>, CheckerQueryError<C::UpstreamError>> {
    Ok(bray_symbols::DependencyEffectOutcomes::new(resolve_effects(request, effects.possible())?)
        .with_normal(resolve_effects_using(request, effects.normal(), bray_symbols::DependencyContractTemplateData::normal_effects)?))
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
    let effects = types.into_iter().collect::<BTreeSet<_>>().into_iter().map(|ty| Effect::Direct {
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

        append_part(request, *ty, part.as_ref().expect("concrete represented cleanup part"), &scope, &mut children, resolve)?;

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
    let value = Effect::Direct {
        subject: DependencySubject::root(DependencySubjectRoot::Result),
        kind: Kind::Dispose(ty), access_type: Some(ty),
    };

    let mut effects = Vec::new();

    expand(request, shapes, &value, active, &mut effects, resolve)?;

    for effect in effects {
        let kind = match &effect {
            Effect::Direct { subject, kind, .. } if subject.subject_root() == DependencySubjectRoot::Result => Some(*kind),
            Effect::Indirect { reference, kind, .. } if reference.subject_root() == DependencySubjectRoot::Result => Some(*kind),
            _ => None,
        };

        resolved.push(match kind {
            Some(Kind::Dispose(ty)) => replace_kind(region, Kind::Cleanup(ty), None),
            Some(kind) => replace_kind(region, kind, None),
            None => effect,
        });
    }

    Ok(())
}

fn append_part<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    part: &bray_bound_tree::StorageCleanupPart,
    scope: &Effect,
    effects: &mut Vec<Effect>,
    resolve: &mut impl FnMut(bray_symbols::CallableInstanceData) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    if let Some(fields) = crate::representation::raw_buffer_fields(request, ty)? {
        let pointer = [Projection::ProductField(fields.pointer)];

        let backing = match scope {
            Effect::Direct { subject, .. } => Effect::Indirect {
                reference: DependencySubject::new(subject.subject_root(), subject.projections().iter().copied().chain(pointer)),
                projections: [].into(), access_type: None, kind: Kind::Retire,
            },
            _ => unbounded_scope(scope, Kind::Retire, None),
        };

        let elements = if matches!(scope, Effect::Direct { .. }) {
            project(scope, &[Projection::OwnedTarget, Projection::AnyElement], Kind::Dispose(fields.element), Some(fields.element))
        } else {
            // A region bound does not locate the element descriptors within it.
            replace_kind(&backing, Kind::Cleanup(fields.element), None)
        };

        effects.push(elements);
        effects.push(backing);

        return Ok(());
    }

    if let Some(release) = part.release() {
        return append_call(release.callable(), scope, effects, resolve);
    }

    if !part.phases().includes_lifecycle() {
        effects.push(scope.clone());

        return Ok(());
    }

    for slot in [bray_symbols::TypeAssociatedLifecycleSlot::Finalizer, bray_symbols::TypeAssociatedLifecycleSlot::Destructor] {
        let selected = request.context().lifecycle_callable(ty, slot)?;

        assert!(!selected.diagnostics().has_errors(), "selected cleanup lifecycle retains its checked declaration: {ty:?}");

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

    effects.extend(selected.iter().map(|effect| match effect {
        Effect::Direct { subject, kind, access_type } if cleanup_receiver(subject.subject_root()) => {
            project(scope, subject.projections(), *kind, *access_type)
        }
        Effect::Indirect { reference, projections, kind, access_type } if cleanup_receiver(reference.subject_root()) => {
            if reference.projections().is_empty() {
                project(scope, projections, *kind, *access_type)
            } else if let Effect::Direct { subject, .. } = scope {
                Effect::Indirect {
                    reference: DependencySubject::new(subject.subject_root(), subject.projections().iter().copied().chain(reference.projections().iter().copied())),
                    projections: std::sync::Arc::clone(projections), kind: *kind, access_type: *access_type,
                }
            } else {
                unbounded_scope(scope, *kind, *access_type)
            }
        }
        // A selected destructor cannot retain its own terminal receiver address.
        // Unknown effects keep the supplied receiver scope instead of exposing it.
        Effect::Unknown | Effect::Escape if !matches!(scope, Effect::UnboundedMutation { .. }) => scope.clone(),
        effect => effect.clone(),
    }));

    Ok(())
}

fn mutation(effect: &Effect) -> Option<(Kind, Option<TypeId>)> {
    match effect {
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

fn project(scope: &Effect, suffix: &[Projection], kind: Kind, access_type: Option<TypeId>) -> Effect {
    match scope {
        Effect::Direct { subject, .. } => Effect::Direct {
            subject: DependencySubject::new(subject.subject_root(), subject.projections().iter().copied().chain(suffix.iter().copied())),
            kind, access_type,
        },
        Effect::Indirect { reference, projections, .. } => Effect::Indirect {
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
