use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::SemanticValueStoreError;

use crate::CheckerInfrastructureError;
use crate::constant::shape::storage_observation_subject;
use crate::contract::MAX_CONDITION_STEPS;

use super::flow::{DomainState, GuaranteeDomain};

impl GuaranteeDomain<'_> {
    pub(super) fn call_mutations(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Result<Option<Vec<bray_bound_tree::StorageAccessId>>, CheckerInfrastructureError> {
        if matches!(
            self.selections.expression(expression),
            Some(bray_bound_tree::SemanticSelection::Call(call))
                if matches!(call.resolution().result(), bray_bound_tree::BoundCallResult::LazyFuture(_))
                    && call.arguments().iter().all(|argument| matches!(argument,
                        bray_bound_tree::SelectedArgument::Explicit { conversion, .. }
                            if matches!(conversion.target(), bray_bound_tree::ConversionTarget::Identity)))
        ) {
            // Captures are evaluated and transferred here. The body executes at suspension.
            return Ok(Some(Vec::new()));
        }

        let Some(bray_bound_tree::SemanticSelection::Call(selected)) =
            self.selections.expression(expression)
        else {
            return Ok(None);
        };

        if !self.call_captures_are_observable(selected)? {
            return Ok(None);
        }

        let Some(call) = self.input.call(expression) else {
            return Ok(None);
        };

        let mut mutations = Vec::new();

        for argument in call.arguments() {
            let crate::ExecutionCallArgument::Expression(argument) = argument else {
                continue;
            };

            if !call.borrows_argument(*argument) {
                continue;
            }

            let mut remaining = MAX_CONDITION_STEPS;

            let subject = self
                .input
                .expression(*argument)
                .map(|term| storage_observation_subject(self.values, term, &mut remaining))
                .transpose()?
                .flatten();

            let Some(accesses) =
                subject.and_then(|subject| self.observation_accesses.get(&subject))
            else {
                return Ok(None);
            };

            mutations.extend(accesses.iter().copied());
        }

        Ok(Some(mutations))
    }

    pub(super) fn suspension_mutations(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> Result<Option<Vec<bray_bound_tree::StorageAccessId>>, CheckerInfrastructureError> {
        let Some(suspension) =
            self.asynchronous.suspensions().iter().find(|suspension| {
                suspension.expression() == expression && !suspension.is_recovered()
            })
        else {
            return Ok(None);
        };

        if !self.suspension_captures_are_observable(suspension)? {
            return Ok(None);
        }

        let Some(mut mutations) =
            self.mutable_borrow_mutations(suspension.retained_subjects().iter().filter_map(
                |subject| match subject {
                    bray_bound_tree::BoundDependencySubject::BorrowCapability(capability) => {
                        Some(*capability)
                    }
                    _ => None,
                },
            ))
        else {
            return Ok(None);
        };

        for subject in suspension.retained_subjects() {
            if matches!(
                subject,
                bray_bound_tree::BoundDependencySubject::ScopedCapability(_)
            ) {
                return Ok(None);
            }

            if let bray_bound_tree::BoundDependencySubject::StorageAccess(access) = subject
                && self.mutable_borrow_accesses.iter().any(|borrowed| {
                    self.storage.relationship(*access, *borrowed)
                        != bray_bound_tree::StorageRelationship::Disjoint
                })
            {
                mutations.push(*access);
            }
        }

        Ok(Some(mutations))
    }

    fn suspension_captures_are_observable(
        &self,
        suspension: &bray_bound_tree::AsyncSuspensionPoint,
    ) -> Result<bool, CheckerInfrastructureError> {
        let bray_bound_tree::AsyncSuspensionKind::Await { operand } = suspension.kind() else {
            return Ok(true);
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            self.selections.expression(operand)
        else {
            return Ok(false);
        };

        if !matches!(
            call.resolution().result(),
            bray_bound_tree::BoundCallResult::LazyFuture(_)
        ) {
            return Ok(false);
        }

        self.call_captures_are_observable(call)
    }

    fn call_captures_are_observable(
        &self,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<bool, CheckerInfrastructureError> {
        // Opaque owners can carry mutation authority not named by the direct call's inputs.
        // Their execution needs fresh observations until that transitive authority is known.
        if let Some(receiver) = call.receiver()
            && !self.capture_mutations_are_observable(receiver.target_type())?
        {
            return Ok(false);
        }

        for argument in call.arguments() {
            let bray_bound_tree::SelectedArgument::Explicit { conversion, .. } = argument else {
                return Ok(false);
            };

            if !matches!(
                conversion.target(),
                bray_bound_tree::ConversionTarget::Identity
            ) || !self.capture_mutations_are_observable(conversion.target_type())?
            {
                return Ok(false);
            }
        }

        Ok(true)
    }

    fn capture_mutations_are_observable(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<bool, CheckerInfrastructureError> {
        let ty = match &*self.values.type_data(ty)? {
            bray_symbols::TypeData::Borrow { target, .. } => *target,
            _ => ty,
        };

        self.owned_mutations_are_observable(ty)
    }

    fn owned_mutations_are_observable(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<bool, CheckerInfrastructureError> {
        let mut pending = vec![ty];
        let mut visited = BTreeSet::new();

        while let Some(ty) = pending.pop() {
            if !visited.insert(ty) {
                continue;
            }

            if visited.len() > MAX_CONDITION_STEPS {
                return Ok(false);
            }

            match &*self.values.type_data(ty)? {
                bray_symbols::TypeData::Named { .. } => {
                    if matches!(
                        crate::representation::type_representation_for_values(
                            self.values,
                            self.available,
                            ty
                        )?,
                        Some(
                            bray_compiler_known::RepresentationRole::Future
                                | bray_compiler_known::RepresentationRole::Task
                                | bray_compiler_known::RepresentationRole::PanicReport
                                | bray_compiler_known::RepresentationRole::RawPointer
                                | bray_compiler_known::RepresentationRole::DevicePointer
                                | bray_compiler_known::RepresentationRole::Uninit
                        )
                    ) {
                        return Ok(false);
                    }

                    let Some(children) = self.input.known_cleanup_dependencies(ty) else {
                        return Ok(false);
                    };

                    pending.extend(children.iter().copied());
                }
                bray_symbols::TypeData::Tuple(elements) => pending.extend(elements.iter().copied()),
                bray_symbols::TypeData::Array { element, .. }
                | bray_symbols::TypeData::Nullable(element)
                | bray_symbols::TypeData::Slice(element)
                | bray_symbols::TypeData::FlexibleArray(element) => pending.push(*element),
                _ => return Ok(false),
            }
        }

        Ok(true)
    }

    pub(super) fn scope_exit_mutations(
        &self,
        block: bray_bound_tree::BoundBlockId,
        exit: bray_bound_tree::AnyBoundNodeId,
        phase: super::super::model::AnalysisScopeExitPhase,
    ) -> Result<Option<Vec<bray_bound_tree::StorageAccessId>>, CheckerInfrastructureError> {
        let Some(mut mutations) =
            self.active_exit_mutations(bray_bound_tree::StorageExitPoint::new(block, exit))
        else {
            return Ok(None);
        };

        for plan in self
            .asynchronous
            .scope_exits()
            .iter()
            .filter(|plan| plan.scope() == block && plan.exit() == exit)
        {
            if plan.is_recovered() {
                return Ok(None);
            }

            let targets = match phase {
                super::super::model::AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                    plan.cancellation_broadcast()
                }
                super::super::model::AnalysisScopeExitPhase::LifecycleResolution => {
                    plan.lifecycle_resolution()
                }
            };

            for access in targets {
                if !self.cleanup_mutations_are_observable(*access, &[])? {
                    return Ok(None);
                }
            }

            mutations.extend(targets);
        }

        Ok(Some(mutations))
    }

    fn active_exit_mutations(
        &self,
        exit: bray_bound_tree::StorageExitPoint,
    ) -> Option<Vec<bray_bound_tree::StorageAccessId>> {
        let flow = self
            .storage_exits
            .get(&exit)
            .filter(|flow| !flow.is_recovered())?;

        self.mutable_borrow_mutations(flow.active_borrows().iter().copied())
    }

    fn mutable_borrow_mutations(
        &self,
        borrows: impl IntoIterator<Item = bray_bound_tree::BorrowCapabilityId>,
    ) -> Option<Vec<bray_bound_tree::StorageAccessId>> {
        let mut mutations = Vec::new();

        for borrow in borrows {
            let capability = self.storage.borrow_capability(borrow)?;

            if capability.is_recovered() {
                return None;
            }

            if capability.kind() == bray_symbols::BorrowKind::Mutable {
                mutations.push(capability.access());
            }
        }

        Some(mutations)
    }

    pub(super) fn invalidate_storage_observations(
        &self,
        state: &mut DomainState,
        mutations: &[bray_bound_tree::StorageAccessId],
    ) -> Result<(), CheckerInfrastructureError> {
        for mutation in mutations {
            let Some(root) = self.storage.root_identity(*mutation) else {
                self.invalidate_observations(state);

                return Ok(());
            };

            if self.asynchronous.cleanup_free_futures().contains(&root) {
                continue;
            }

            let observable = match self.storage.storage_type(root) {
                Some(ty)
                    if self
                        .storage
                        .identity(root)
                        .is_some_and(|identity| identity.is_parameter()) =>
                {
                    self.capture_mutations_are_observable(ty)?
                }
                Some(ty) => self.owned_mutations_are_observable(ty)?,
                None => false,
            };

            if !observable {
                self.invalidate_observations(state);

                return Ok(());
            }
        }

        self.retain_disjoint_observations(state, |access| {
            super::super::storage_index::accesses_are_disjoint(self.storage, [access], mutations)
        })
        .map_err(CheckerInfrastructureError::SemanticValueStore)
    }

    pub(super) fn invalidate_cleanup_observations(
        &self,
        state: &mut DomainState,
        access: bray_bound_tree::StorageAccessId,
        path: &[bray_bound_tree::StorageCleanupProjection],
        exit: bray_bound_tree::StorageExitPoint,
    ) -> Result<(), CheckerInfrastructureError> {
        if !self.cleanup_mutations_are_observable(access, path)? {
            self.invalidate_observations(state);

            return Ok(());
        }

        let Some(mutations) = self.active_exit_mutations(exit) else {
            self.invalidate_observations(state);

            return Ok(());
        };

        let path = super::super::storage_index::cleanup_mutation_path(path);

        self.retain_disjoint_observations(state, |dependency| {
            self.storage
                .projected_relationship(access, &path, dependency)
                == bray_bound_tree::StorageRelationship::Disjoint
                && super::super::storage_index::accesses_are_disjoint(
                    self.storage,
                    [dependency],
                    &mutations,
                )
        })
        .map_err(CheckerInfrastructureError::SemanticValueStore)
    }

    fn cleanup_mutations_are_observable(
        &self,
        access: bray_bound_tree::StorageAccessId,
        path: &[bray_bound_tree::StorageCleanupProjection],
    ) -> Result<bool, CheckerInfrastructureError> {
        if self
            .storage
            .root_identity(access)
            .is_some_and(|root| self.asynchronous.cleanup_free_futures().contains(&root))
        {
            return Ok(true);
        }

        let ty = path
            .last()
            .map(|projection| projection.result_type())
            .or_else(|| {
                self.storage
                    .access(access)
                    .map(|access| access.reached_type())
            });

        match ty {
            Some(ty) => self.owned_mutations_are_observable(ty),
            None => Ok(false),
        }
    }

    fn retain_disjoint_observations(
        &self,
        state: &mut DomainState,
        disjoint: impl Fn(bray_bound_tree::StorageAccessId) -> bool,
    ) -> Result<(), SemanticValueStoreError> {
        let mut preserved = BTreeMap::new();

        if let Some(retained) = self.retained_mutations {
            for (subject, accesses) in self.observation_accesses {
                let checked = accesses.iter().all(|access| {
                    self.storage
                        .root_identity(*access)
                        .and_then(|identity| self.storage.identity(identity))
                        .is_some_and(|identity| {
                            !matches!(
                                identity,
                                bray_bound_tree::StorageIdentity::Static(_)
                                    | bray_bound_tree::StorageIdentity::Error(_)
                            )
                        })
                });

                if checked
                    && accesses.iter().copied().all(&disjoint)
                    && super::super::storage_index::accesses_are_disjoint(
                        self.storage,
                        accesses.iter().copied(),
                        retained,
                    )
                    && let Some(value) = self.current_observation(state, *subject, None, None)?
                {
                    preserved.insert(*subject, value);
                }
            }
        }

        self.invalidate_observations(state);
        state.observations = preserved;

        Ok(())
    }
}
