use bray_bound_tree::{StoragePath, StorageTarget};
use bray_symbols::{TypeData, TypeId};

use super::super::state::ExecutionState;
use super::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn storage_path_type(&self, path: &StoragePath) -> Option<TypeId> {
        path.access_id(self.storage)
            .and_then(|access| self.storage.access(access))
            .map(|access| access.reached_type())
            .or_else(|| path.projections.is_empty().then(|| self.storage.storage_type(path.root)).flatten())
    }

    /// The established typed slot and whether a language loan is used to reach it.
    pub(in crate::analysis::guarantee) fn current_target_place(&self, target: &StorageTarget, ty: TypeId) -> (ExecutionPlace, bool) {
        let path = target.value_observation_path();

        let view = path.as_ref().and_then(|path| {
            if path.projections.is_empty() { self.storage.storage_type(path.root) }
            else { self.storage_path_type(path) }
        });

        let borrowed = view.is_some_and(|view| matches!(self.request.semantic_values().type_data(view).as_ref(),
            TypeData::Borrow { target, .. } if *target == ty));

        let place = path.and_then(|path| ExecutionPlace::storage_path(self.storage, &path))
            .unwrap_or_else(|| ExecutionPlace::pointee(self.storage, target, ty));

        (place, borrowed)
    }

    /// Physical contents exist only after an actual checked typed view published them.
    /// A pointer descriptor and a lifetime/alias bound are never a contents snapshot.
    pub(in crate::analysis::guarantee) fn current_target_value(
        &self,
        state: &ExecutionState,
        target: &StorageTarget,
        pointee: Option<TypeId>,
    ) -> Option<ExecutionCondition> {
        if !target.is_exact() || target.previous {
            return None;
        }

        let Some(ty) = pointee else { return self.current_storage_value(state, &target.value_observation_path()?); };

        let (place, borrowed) = self.current_target_place(target, ty);

        let value = state.current.live_value(&place)?;

        Some(if borrowed && value.is_reference() {
            ExecutionCondition::storage_value(bray_symbols::CapturedValueOperation::Read, vec![value].into()).copied_value().clone()
        } else { value })
    }

    /// Binds a produced result's reference fields to the reached post-state.
    pub(in crate::analysis::guarantee) fn complete_returned_references(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        output: Option<&ExecutionPlace>,
        (producer, result): (&ExecutionCondition, ExecutionCondition),
        completion: &bray_bound_tree::StorageCompletion,
    ) -> ExecutionCondition {
        let mut result = self.complete_captured_references(state, mutation, result);
        let Some(output) = output else { return result; };
        let origin = producer.referent_owner_identity();
        let produced = matches!(origin.kind(), crate::ExecutionConditionKind::Expression(expression)
            if mutation.occurrence().expression() == Some(*expression));

        for (path, observation) in completion.references() {
            let target = &observation.target;

            if target.previous || !target.is_exact() {
                continue;
            }

            let Some(place) = ExecutionPlace::storage_path(self.storage, path).filter(|place| output.contains(place)) else {
                continue;
            };

            let ty = observation.pointee.expect("a completed reference retains its checked pointee type");
            let projections = &place.projections[output.projections.len()..];
            let reference = projections.iter().fold(result.clone(), |value, projection| value.component(*projection));
            let reached = self.current_target_value(state, target, Some(ty))
                .or_else(|| self.publish_returned_pointee(state, target, ty, &reference));

            if let Some(reached) = reached {
                let provisional = produced && target.value_observation_path().is_some()
                    && reached.ownership_identity().is_some()
                    && projections.iter().fold(origin.clone(), |value, field| value.component(*field))
                        .borrowed().owner_identity() == reference.owner_identity();

                let reference = if provisional {
                    let bound = reached.borrowed();

                    if let Some(storage) = reference.inline_captured_storage() {
                        bound.captured_value(storage.clone())
                    } else {
                        bound
                    }
                } else if reference.is_reference() {
                    reference.with_referent(reached)
                } else {
                    reached.borrowed()
                };

                let path = &path.projections[path.projections.len() - projections.len()..];

                result = result.with_storage_component(path, reference);
            }
        }

        result
    }

    fn publish_returned_pointee(
        &self,
        state: &mut ExecutionState,
        target: &StorageTarget,
        ty: TypeId,
        reference: &ExecutionCondition,
    ) -> Option<ExecutionCondition> {
        if target.value_observation_path().is_some() { return None; }

        let static_root = match &target.origin {
            bray_bound_tree::StorageOrigin::Static { .. } => true,
            bray_bound_tree::StorageOrigin::Storage(path)
            | bray_bound_tree::StorageOrigin::Entry { reference: path, .. } => {
                matches!(self.storage.identity(path.root), Some(bray_bound_tree::StorageIdentity::Static(_)))
            }
            _ => false,
        };

        if static_root {
            // Declaration-only paths cannot identify a generic or exact-thread instance.
            return None;
        }

        if let bray_bound_tree::StorageOrigin::Returned { reference, .. } = &target.origin
            && (!reference.projections.is_empty() || !matches!(self.storage.identity(reference.root),
                Some(bray_bound_tree::StorageIdentity::Temporary(_)
                    | bray_bound_tree::StorageIdentity::Evaluated(_)
                    | bray_bound_tree::StorageIdentity::InvocationResult(_)
                    | bray_bound_tree::StorageIdentity::ScopedCapability { .. }))) {
            // An opaque field declaration does not identify the producer's epoch.
            return None;
        }

        let physical = ExecutionPlace::pointee(self.storage, target, ty);

        // A new view cannot reinitialize storage when a preceding value consumed it.
        if state.current.get(&physical).is_some() || !reference.is_reference() {
            return None;
        }

        let value = ExecutionCondition::storage_value(bray_symbols::CapturedValueOperation::Read, vec![reference.clone()].into())
            .copied_value().clone();

        if value.is_unknown() { return None; }

        state.current.insert(physical, value.clone());

        Some(value)
    }

    pub(super) fn pointee_preservations(
        &self,
        state: &ExecutionState,
        invalidation: &bray_bound_tree::StorageInvalidation,
        mut observations: Option<&mut crate::execution_guarantees::ObservationSet>,
    ) -> Vec<(ExecutionPlace, std::sync::Arc<[bray_bound_tree::StorageProjection]>, bray_bound_tree::StorageObservationKind)> {
        let mut preserved = Vec::new();

        for ((target, ty), paths) in invalidation.stable_pointees().into_iter().flatten() {
            let place = ExecutionPlace::pointee(self.storage, target, *ty);

            let Some(value) = state.current.live_value(&place) else {
                continue;
            };

            for (path, kind) in paths {
                if let Some(observations) = &mut observations {
                    let value = path.iter().copied().fold(value.clone(), ExecutionCondition::project);

                    observations.insert_classified(value.clone(), *kind);
                    observations.insert_classified(value.borrowed(), *kind);
                }

                preserved.push((place.clone(), std::sync::Arc::clone(path), *kind));
            }
        }

        preserved
    }
}
