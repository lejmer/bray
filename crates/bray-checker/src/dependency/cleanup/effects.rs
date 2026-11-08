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
            subject: DependencySubject::root(input.root()), kind: Kind::Cleanup(input.ty(request.semantic_values())), access_type: Some(input.ty(request.semantic_values())),
        }])?);
    }
    let ty = call.resolution().result().completion_type();
    let result = resolve_effects(request, &[Effect::Direct {
        subject: DependencySubject::root(DependencySubjectRoot::Result), kind: Kind::Cleanup(ty), access_type: Some(ty),
    }])?;
    Ok((captures, result))
}

pub(in crate::dependency) fn resolve_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &[Effect],
) -> Result<Vec<Effect>, CheckerQueryError<C::UpstreamError>> {
    let mut shapes = crate::asynchronous::CleanupShapeResolver::new(request);
    let mut resolved = Vec::new();
    let mut active = BTreeSet::new();

    for effect in effects {
        expand(request, &mut shapes, effect, &mut active, &mut resolved)?;
    }

    Ok(resolved)
}

fn expand<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    effect: &Effect,
    active: &mut BTreeSet<TypeId>,
    resolved: &mut Vec<Effect>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    let Some((Kind::Cleanup(ty), access_type)) = mutation(effect) else {
        // The result owns its effects while sharing immutable projection paths.
        resolved.push(effect.clone());
        return Ok(());
    };

    if shapes.requirement(ty)? == bray_bound_tree::AsyncStorageCleanupRequirement::None {
        return Ok(());
    }

    if access_type.is_none() && !matches!(effect, Effect::UnboundedMutation { .. }) {
        resolved.push(effect.clone());
        return Ok(());
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
            Some(path) => project(effect, &path, Kind::Cleanup(*ty), None),
            None if part.is_some() => Effect::UnboundedMutation { kind: Kind::Cleanup(*ty), access_type: None, ambient: false },
            None => replace_kind(effect, Kind::Cleanup(*ty), None),
        };

        if part.is_none() || matches!(request.semantic_values().type_data(*ty).as_ref(), TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) | TypeData::TypeValuedMemberProjection { .. })
            && part.as_ref().is_some_and(|part| part.release().is_none())
        {
            resolved.push(scope);
            continue;
        }

        let mut children = Vec::new();
        append_part(request, *ty, part.as_ref().expect("concrete represented cleanup part"), &scope, &mut children)?;

        for child in children {
            expand(request, shapes, &child, active, resolved)?;
        }
    }

    active.remove(&ty);
    Ok(())
}

fn append_part<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    part: &bray_bound_tree::StorageCleanupPart,
    scope: &Effect,
    effects: &mut Vec<Effect>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    if let Some(fields) = crate::representation::raw_buffer_fields(request, ty)? {
        let pointer = [Projection::ProductField(fields.pointer)];
        let backing = match scope {
            Effect::Direct { subject, .. } => Effect::Indirect {
                reference: DependencySubject::new(subject.subject_root(), subject.projections().iter().copied().chain(pointer)),
                projections: [].into(), access_type: None, kind: Kind::Retire,
            },
            _ => Effect::UnboundedMutation { kind: Kind::Retire, access_type: None, ambient: false },
        };

        effects.push(replace_kind(&backing, Kind::Cleanup(fields.element), None));
        effects.push(backing);
        return Ok(());
    }

    if let Some(release) = part.release() {
        return append_call(request, release.callable(), scope, effects);
    }

    if !part.phases().includes_lifecycle() {
        effects.push(scope.clone());
        return Ok(());
    }

    for slot in [bray_symbols::TypeAssociatedLifecycleSlot::Finalizer, bray_symbols::TypeAssociatedLifecycleSlot::Destructor] {
        let selected = request.context().lifecycle_callable(ty, slot)?;
        assert!(!selected.diagnostics().has_errors(), "selected cleanup lifecycle retains its checked declaration: {ty:?}");

        if let Some((callable, _)) = selected.value() {
            append_call(request, *callable, scope, effects)?;
        }
    }

    Ok(())
}

fn append_call<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    callable: bray_symbols::CallableInstanceData,
    scope: &Effect,
    effects: &mut Vec<Effect>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    let template = request.callable_result_dependencies(callable.definition().callable_symbol())?;
    let template = request.semantic_values().substitute_dependency_contract(template, callable.substitution())
        .expect("selected lifecycle substitution retains its checked dependency contract");
    let template = request.semantic_values().dependency_contract_template_data(template);

    effects.extend(template.effects().iter().map(|effect| match effect {
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
                Effect::unresolved(*kind, *access_type)
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
