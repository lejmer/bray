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

    pub(in crate::analysis::guarantee) fn borrowed_path_type(&self, path: &StoragePath) -> Option<TypeId> {
        let ty = self.storage_path_type(path)?;

        match self.request.semantic_values().type_data(ty).as_ref() {
            TypeData::Borrow { target, .. } => Some(*target),
            _ => None,
        }
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

    pub(in crate::analysis::guarantee) fn publish_returned_pointees(
        &self,
        state: &mut ExecutionState,
        output: Option<&ExecutionPlace>,
        result: &ExecutionCondition,
        completion: &bray_bound_tree::StorageCompletion,
    ) {
        let Some(output) = output else {
            return;
        };

        for (path, target) in completion.references() {
            if target.previous || !target.is_exact() || target.value_observation_path().is_some() {
                continue;
            }

            let static_root = match &target.origin {
                bray_bound_tree::StorageOrigin::Static { .. } => true,
                bray_bound_tree::StorageOrigin::Storage(path)
                | bray_bound_tree::StorageOrigin::Entry { reference: path, .. } => {
                    matches!(self.storage.identity(path.root), Some(bray_bound_tree::StorageIdentity::Static(_)))
                }
                _ => false,
            };

            if static_root {
                // Declaration-only physical paths do not identify a selected
                // generic or exact-thread static instance.
                continue;
            }

            if let bray_bound_tree::StorageOrigin::Returned { reference, .. } = &target.origin
                && (!reference.projections.is_empty() || !matches!(self.storage.identity(reference.root),
                    Some(bray_bound_tree::StorageIdentity::Temporary(_)
                        | bray_bound_tree::StorageIdentity::Evaluated(_)
                        | bray_bound_tree::StorageIdentity::InvocationResult(_)
                        | bray_bound_tree::StorageIdentity::ScopedCapability { .. }))) {
                // Opaque field rereads have a separate producer-retirement
                // transition. Their declaration is not that producer's epoch.
                continue;
            }

            let Some(place) = ExecutionPlace::storage_path(self.storage, path).filter(|place| output.contains(place)) else {
                continue;
            };

            let Some(ty) = self.borrowed_path_type(path) else {
                continue;
            };

            let physical = ExecutionPlace::pointee(self.storage, target, ty);

            // Another view does not reinitialize a consumed object. Producer
            // re-evaluation removes the old epoch before publishing its new one.
            if state.current.get(&physical).is_some() {
                continue;
            }

            let reference = place.projections[output.projections.len()..].iter()
                .fold(result.clone(), |value, projection| value.component(*projection));

            let value = ExecutionCondition::storage_value(bray_symbols::CapturedValueOperation::Read, vec![reference].into())
                .copied_value().clone();

            if !value.is_unknown() {
                state.current.insert(physical, value);
            }
        }
    }

    pub(super) fn pointee_preservations(
        &self,
        state: &ExecutionState,
        invalidation: &bray_bound_tree::StorageInvalidation,
        observations: &mut crate::execution_guarantees::ObservationSet,
    ) -> Vec<(ExecutionPlace, std::sync::Arc<[bray_bound_tree::StorageProjection]>, bray_bound_tree::StorageObservationKind)> {
        let mut preserved = Vec::new();

        for ((target, ty), paths) in invalidation.stable_pointees().into_iter().flatten() {
            let place = ExecutionPlace::pointee(self.storage, target, *ty);

            let Some(value) = state.current.live_value(&place) else {
                continue;
            };

            for (path, kind) in paths {
                let value = path.iter().copied().fold(value.clone(), ExecutionCondition::project);

                observations.insert_classified(value.clone(), *kind);
                observations.insert_classified(value.borrowed(), *kind);
                preserved.push((place.clone(), std::sync::Arc::clone(path), *kind));
            }
        }

        preserved
    }
}
