use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::StorageCleanupType;
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{CallableExecution, TypeAssociatedLifecycleSlot, TypeData, TypeId};

use super::cleanup::{CleanupShapeResolver, cleanup_requirement};
use crate::{CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext};

/// One execution dependency in an owned value's cleanup graph.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CleanupExecutionStep {
    /// The selected whole-value finalizer.
    Finalization,
    /// Destruction and the initialized remainder of a consuming destructor.
    Destruction,
    /// Settlement of owned runs while preserving result ownership.
    Quiescence,
}

impl CleanupExecutionStep {
    const fn children(self) -> &'static [Self] {
        match self {
            Self::Finalization => &[],
            Self::Destruction => &[Self::Finalization, Self::Destruction],
            Self::Quiescence => &[Self::Quiescence],
        }
    }

    const fn slot(self) -> Option<TypeAssociatedLifecycleSlot> {
        match self {
            Self::Finalization => Some(TypeAssociatedLifecycleSlot::Finalizer),
            Self::Destruction => Some(TypeAssociatedLifecycleSlot::Destructor),
            Self::Quiescence => None,
        }
    }
}

type LifecycleStep = (TypeId, CleanupExecutionStep);

/// Resolves finalization, destruction, and run quiescence over the checked ownership graph.
pub fn cleanup_type_execution<C: CheckerRequestContext + ?Sized>(
    context: &C,
    ty: TypeId,
) -> Result<DiagnosticResult<StorageCleanupType>, CheckerQueryError<C::UpstreamError>> {
    cleanup_type_execution_with(context, ty, |_| Ok(None), |_| Ok(false))
}

/// Resolves cleanup execution using available dependency summaries of checked destructor bodies.
/// Each summary retains the lifecycle actions remaining after checked completion omissions.
/// Completion decisions omit only the selected whole-value finalizer and its uncreated result.
/// Callers supplying proof candidates must certify their dependencies before lowering uses the modes.
pub fn cleanup_type_execution_with<C, E>(
    context: &C,
    ty: TypeId,
    mut destructor_actions: impl FnMut(TypeId) -> Result<Option<Vec<(TypeId, CleanupExecutionStep)>>, E>,
    mut finalization_complete: impl FnMut(TypeId) -> Result<bool, E>,
) -> Result<DiagnosticResult<StorageCleanupType>, E>
where
    C: CheckerRequestContext + ?Sized,
    E: From<CheckerQueryError<C::UpstreamError>>,
{
    let mut resolver = CleanupShapeResolver::new(context);
    let shape = resolver.resolve(ty).map_err(E::from)?;

    resolver.resolve_execution_with(&mut destructor_actions, &mut finalization_complete)?;

    let cleanup = resolver
        .cleanup_types
        .remove(&ty)
        .unwrap_or_else(|| StorageCleanupType::new(ty, cleanup_requirement(shape)));

    Ok(DiagnosticResult::new(cleanup, resolver.diagnostics))
}

impl<C: CheckerRequestContext + ?Sized> CleanupShapeResolver<'_, C> {
    pub(super) fn resolve_execution(&mut self) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        self.resolve_execution_with(&mut |_| Ok(None), &mut |_| Ok(false))
    }

    fn resolve_execution_with<E: From<CheckerQueryError<C::UpstreamError>>>(
        &mut self,
        destructor_actions: &mut impl FnMut(
            TypeId,
        )
            -> Result<Option<Vec<(TypeId, CleanupExecutionStep)>>, E>,
        finalization_complete: &mut impl FnMut(TypeId) -> Result<bool, E>,
    ) -> Result<(), E> {
        let mut execution = BTreeMap::new();
        let mut dependencies = BTreeMap::new();
        let mut pending = self.completed.keys().copied().collect::<BTreeSet<_>>();
        let mut visited = BTreeSet::new();

        while let Some(ty) = pending.pop_first() {
            if self.context.cancellation().is_cancelled() {
                return Err(E::from(CheckerQueryError::Cancelled));
            }

            visited.insert(ty);

            let actions = destructor_actions(ty)?;
            let complete = finalization_complete(ty)?;

            let finalizer_result = if complete {
                None
            } else {
                self.finalizer_result(ty).map_err(E::from)?
            };

            let previous_count = self.completed.len();

            for dependency in actions
                .iter()
                .flatten()
                .map(|(ty, _)| *ty)
                .chain(finalizer_result)
            {
                self.resolve(dependency).map_err(E::from)?;
            }

            if self.completed.len() != previous_count {
                pending.extend(
                    self.completed
                        .keys()
                        .copied()
                        .filter(|ty| !visited.contains(ty)),
                );
            }

            let data = self
                .context
                .semantic_values()
                .type_data(ty)
                .map_err(CheckerInfrastructureError::SemanticValueStore)
                .map_err(CheckerQueryError::Infrastructure)
                .map_err(E::from)?;

            for step in [
                CleanupExecutionStep::Finalization,
                CleanupExecutionStep::Destruction,
                CleanupExecutionStep::Quiescence,
            ] {
                if complete && step == CleanupExecutionStep::Finalization {
                    execution.insert((ty, step), Some(CallableExecution::Synchronous));
                    dependencies.insert((ty, step), BTreeSet::new());
                    continue;
                }

                let (mode, represented) = self
                    .execution_seed(ty, data.as_ref(), step)
                    .map_err(E::from)?;

                execution.insert((ty, step), mode);

                let children = if step == CleanupExecutionStep::Finalization {
                    finalizer_result
                        .into_iter()
                        .map(|result| (result, CleanupExecutionStep::Quiescence))
                        .collect()
                } else if step == CleanupExecutionStep::Destruction
                    && let Some(actions) = &actions
                {
                    actions.iter().copied().collect()
                } else if represented {
                    self.dependencies
                        .get(&ty)
                        .into_iter()
                        .flatten()
                        .flat_map(|child| step.children().iter().map(|step| (*child, *step)))
                        .collect()
                } else {
                    BTreeSet::new()
                };

                dependencies.insert((ty, step), children);
            }
        }

        propagate_execution(&mut execution, &dependencies);

        for (ty, shape) in &self.completed {
            let cleanup = self
                .cleanup_types
                .remove(ty)
                .unwrap_or_else(|| StorageCleanupType::new(*ty, cleanup_requirement(*shape)));

            self.cleanup_types.insert(
                *ty,
                cleanup.with_execution(
                    execution
                        .get(&(*ty, CleanupExecutionStep::Finalization))
                        .copied()
                        .flatten(),
                    execution
                        .get(&(*ty, CleanupExecutionStep::Destruction))
                        .copied()
                        .flatten(),
                    execution
                        .get(&(*ty, CleanupExecutionStep::Quiescence))
                        .copied()
                        .flatten(),
                ),
            );
        }

        Ok(())
    }

    fn finalizer_result(
        &mut self,
        ty: TypeId,
    ) -> Result<Option<TypeId>, CheckerQueryError<C::UpstreamError>> {
        let selected = self
            .context
            .lifecycle_callable(ty, TypeAssociatedLifecycleSlot::Finalizer)?;

        let (selected, diagnostics) = selected.into_parts();

        self.diagnostics.add_range(diagnostics);

        // A failed finalizer's returned owner must be quiescent before an incident can retain it.
        Ok(selected.map(|(_, signature)| signature.result()))
    }

    fn execution_seed(
        &mut self,
        ty: TypeId,
        data: &TypeData,
        step: CleanupExecutionStep,
    ) -> Result<(Option<CallableExecution>, bool), CheckerQueryError<C::UpstreamError>> {
        if self.completed.get(&ty).is_some_and(|shape| shape.recovered) {
            return Ok((None, false));
        }

        match data {
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. } => {
                return Ok((None, false));
            }
            TypeData::Named { definition, .. } => {
                match self.representation_role(*definition) {
                    Some(RepresentationRole::Task) => {
                        return Ok((
                            Some(if step == CleanupExecutionStep::Destruction {
                                CallableExecution::Synchronous
                            } else {
                                CallableExecution::Asynchronous
                            }),
                            false,
                        ));
                    }
                    Some(RepresentationRole::Future) => {
                        return Ok((
                            Some(if step == CleanupExecutionStep::Finalization {
                                CallableExecution::Synchronous
                            } else {
                                CallableExecution::Asynchronous
                            }),
                            false,
                        ));
                    }
                    Some(RepresentationRole::String | RepresentationRole::PanicReport) => {
                        return Ok((Some(CallableExecution::Synchronous), false));
                    }
                    _ => {}
                }

                let Some(slot) = step.slot() else {
                    return Ok((Some(CallableExecution::Synchronous), true));
                };

                let selected = self.context.lifecycle_callable(ty, slot)?;

                let (selected, diagnostics) = selected.into_parts();

                let invalid = diagnostics.has_errors();

                self.diagnostics.add_range(diagnostics);

                if invalid {
                    return Ok((None, false));
                }

                if let Some((_, signature)) = selected {
                    let callable = self
                        .context
                        .semantic_values()
                        .type_data(signature.callable_type())
                        .map_err(CheckerInfrastructureError::SemanticValueStore)
                        .map_err(CheckerQueryError::Infrastructure)?;

                    return Ok((
                        match callable.as_ref() {
                            TypeData::Callable(callable) => Some(callable.execution()),
                            _ => None,
                        },
                        step == CleanupExecutionStep::Destruction,
                    ));
                }
            }
            _ => {}
        }

        Ok((
            Some(CallableExecution::Synchronous),
            step != CleanupExecutionStep::Finalization,
        ))
    }
}

fn propagate_execution(
    execution: &mut BTreeMap<LifecycleStep, Option<CallableExecution>>,
    dependencies: &BTreeMap<LifecycleStep, BTreeSet<LifecycleStep>>,
) {
    let components = bray_base::strongly_connected_components(execution.keys().copied(), |step| {
        dependencies.get(&step).into_iter().flatten().copied()
    });

    // The graph points from a consumer to its dependencies; resolve sink components first.
    for component in components.into_iter().rev() {
        let mut mode = Some(CallableExecution::Synchronous);

        for step in &component {
            for dependency in
                std::iter::once(step).chain(dependencies.get(step).into_iter().flatten())
            {
                mode = match (mode, execution.get(dependency).copied().flatten()) {
                    (Some(current), Some(required)) => Some(current.max(required)),
                    _ => None,
                };
            }
        }

        for step in component {
            execution.insert(step, mode);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use bray_symbols::{CallableExecution, TypeData, TypeId};

    use super::{CleanupExecutionStep, propagate_execution};

    fn types() -> [TypeId; 3] {
        let values = bray_symbols::SemanticValueStore::try_new().unwrap();
        let first = values.intern_type(TypeData::tuple([])).unwrap();
        let second = values.intern_type(TypeData::Nullable(first)).unwrap();
        let third = values.intern_type(TypeData::Nullable(second)).unwrap();

        [first, second, third]
    }

    #[test]
    fn async_child_finalization_propagates_through_recursive_destruction() {
        let [first, second, child] = types();

        let a = (first, CleanupExecutionStep::Destruction);
        let b = (second, CleanupExecutionStep::Destruction);
        let c = (child, CleanupExecutionStep::Finalization);

        let finalizer = (first, CleanupExecutionStep::Finalization);

        let mut modes = BTreeMap::from([
            (a, Some(CallableExecution::Synchronous)),
            (b, Some(CallableExecution::Synchronous)),
            (c, Some(CallableExecution::Asynchronous)),
            (finalizer, Some(CallableExecution::Synchronous)),
        ]);

        let dependencies = BTreeMap::from([(a, BTreeSet::from([b, c])), (b, BTreeSet::from([a]))]);

        propagate_execution(&mut modes, &dependencies);

        assert_eq!(modes[&a], Some(CallableExecution::Asynchronous));
        assert_eq!(modes[&b], Some(CallableExecution::Asynchronous));
        assert_eq!(modes[&finalizer], Some(CallableExecution::Synchronous));
    }

    #[test]
    fn quiescence_propagates_owned_runs_independently_of_graceful_finalizers() {
        let [first, second, child] = types();

        for quiescence in [
            Some(CallableExecution::Synchronous),
            Some(CallableExecution::Asynchronous),
            None,
        ] {
            let mut modes = BTreeMap::new();
            let mut dependencies = BTreeMap::new();

            for step in [
                CleanupExecutionStep::Finalization,
                CleanupExecutionStep::Destruction,
                CleanupExecutionStep::Quiescence,
            ] {
                modes.insert((first, step), Some(CallableExecution::Synchronous));
                modes.insert((second, step), Some(CallableExecution::Synchronous));

                modes.insert(
                    (child, step),
                    match step {
                        CleanupExecutionStep::Finalization => Some(CallableExecution::Asynchronous),
                        CleanupExecutionStep::Destruction => Some(CallableExecution::Synchronous),
                        CleanupExecutionStep::Quiescence => quiescence,
                    },
                );

                dependencies.insert(
                    (first, step),
                    [second, child]
                        .into_iter()
                        .flat_map(|ty| step.children().iter().map(move |step| (ty, *step)))
                        .collect(),
                );

                dependencies.insert(
                    (second, step),
                    step.children().iter().map(|step| (first, *step)).collect(),
                );
            }

            propagate_execution(&mut modes, &dependencies);

            for ty in [first, second] {
                assert_eq!(
                    modes[&(ty, CleanupExecutionStep::Finalization)],
                    Some(CallableExecution::Synchronous)
                );

                assert_eq!(
                    modes[&(ty, CleanupExecutionStep::Destruction)],
                    Some(CallableExecution::Asynchronous)
                );

                assert_eq!(modes[&(ty, CleanupExecutionStep::Quiescence)], quiescence);
            }
        }
    }

    #[test]
    fn missing_dependency_keeps_recursive_consumers_unresolved() {
        let [first, second, missing] = types();

        let a = (first, CleanupExecutionStep::Destruction);
        let b = (second, CleanupExecutionStep::Destruction);
        let c = (missing, CleanupExecutionStep::Finalization);

        let mut modes = BTreeMap::from([
            (a, Some(CallableExecution::Synchronous)),
            (b, Some(CallableExecution::Synchronous)),
        ]);

        let dependencies = BTreeMap::from([(a, BTreeSet::from([b, c])), (b, BTreeSet::from([a]))]);

        propagate_execution(&mut modes, &dependencies);

        assert_eq!(modes[&a], None);
        assert_eq!(modes[&b], None);
    }
}
