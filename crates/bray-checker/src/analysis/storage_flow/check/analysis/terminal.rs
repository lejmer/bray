use std::collections::BTreeSet;

use bray_bound_tree::{
    CheckedMemoryOperationKind, StorageAccessPurpose, StoragePath, StorageRelationship,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, SeverityKind,
};
use bray_symbols::{DependencyMutationKind, DependencyStorageEffect, TypeId};

use super::core::StorageFlowCollector;
use crate::analysis::model::AnalysisOperationKind;
use crate::analysis::storage_flow::memory::{Effect, State, Target};
use crate::analysis::storage_flow::model::{StorageFlowInput, StorageFlowState};
use crate::{CheckerRequestContext, CheckerUnitView};

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn prepare_terminal_ownership<
        C: CheckerRequestContext + ?Sized,
    >(
        &mut self,
        shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &bray_bound_tree::StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        memory: &bray_bound_tree::CheckedMemoryOperations,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        self.terminal_receiver = storage.identity_entries().find_map(|(id, _)| {
            bray_bound_tree::storage_identity_is_destructor_receiver(request.unit(), storage, id)
                .then(|| StoragePath::root(id))
        });

        let Some(receiver) = &self.terminal_receiver else {
            return Ok(());
        };

        let ty = storage
            .storage_type(receiver.root)
            .expect("destructor receiver retains its checked type");

        // A lifetime dependency does not transfer its referent's graceful ownership.
        // Dependencies matter here when they support the receiver's own obligations.
        self.terminal_dependencies = shapes.resolve(ty)?.graceful;

        let mut types = storage
            .access_plans()
            .iter()
            .filter(|plan| plan.purpose() == StorageAccessPurpose::Assignment)
            .map(|plan| {
                storage
                    .access(plan.access())
                    .expect("assignment retains its access")
                    .reached_type()
            })
            .collect::<BTreeSet<_>>();

        for effect in dependencies
            .calls()
            .values()
            .flat_map(|call| {
                call.effects()
                    .iter()
                    .chain(call.deferred_effects().into_iter().flatten())
            })
            .chain(
                dependencies
                    .lifecycle_dependencies()
                    .values()
                    .flat_map(|contract| contract.effects()),
            )
        {
            if let DependencyStorageEffect::Direct {
                kind: DependencyMutationKind::Replace(ty),
                ..
            }
            | DependencyStorageEffect::Indirect {
                kind: DependencyMutationKind::Replace(ty),
                ..
            } = effect
            {
                types.insert(*ty);
            }
        }

        for operation in memory.operations() {
            match *operation.kind() {
                CheckedMemoryOperationKind::Write { pointee }
                | CheckedMemoryOperationKind::VolatileWrite { pointee, .. }
                | CheckedMemoryOperationKind::Copy { pointee, .. } => {
                    types.insert(pointee);
                }
                CheckedMemoryOperationKind::UninitWrite { element } => {
                    types.insert(element);
                }
                _ => {}
            }
        }

        for ty in types {
            if shapes.resolve(ty)?.graceful {
                self.graceful_types.insert(ty);
            }
        }

        Ok(())
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn check_terminal_operation(
        &mut self,
        state: &StorageFlowState,
        operation: AnalysisOperationKind,
        assignment: Option<&Target>,
        mutation: Option<DependencyMutationKind>,
        effects: Option<&[Effect]>,
        roots: &[Target],
    ) {
        if !self.publish || state.recovered || self.input.terminal_receiver.is_none() {
            return;
        }

        if let Some(mutation) = mutation {
            if let Some(target) = assignment {
                // The effect owns its path while the assignment retains the same shared projections.
                self.check_terminal_effects(
                    state,
                    &state.memory,
                    operation,
                    Some(&[Effect::Mutation(target.clone(), mutation)]),
                    [],
                );
            } else if matches!(mutation, DependencyMutationKind::Replace(ty) if self.input.graceful_types.contains(&ty))
            {
                // Without a physical destination, a lifetime anchor cannot prove that
                // installation is disjoint from the terminal receiver.
                self.report_terminal_operation(operation, None);
            }
        }

        if effects.is_some() || self.call_attempt(operation).is_some() {
            self.check_terminal_effects(
                state,
                &state.memory,
                operation,
                effects,
                roots.iter().cloned(),
            );
        }
    }

    pub(super) fn check_terminal_effects(
        &mut self,
        state: &StorageFlowState,
        memory: &State,
        operation: AnalysisOperationKind,
        effects: Option<&[Effect]>,
        roots: impl IntoIterator<Item = Target>,
    ) {
        let Some(receiver) = self
            .input
            .terminal_receiver
            .as_ref()
            .filter(|_| self.publish)
        else {
            return;
        };

        // Scalar writes and release/consumption cannot introduce typed graceful ownership.
        // Unknown writes and opaque cleanup still need a reachability proof.
        let relevant = effects.is_none_or(|effects| {
            effects.iter().any(|effect| match effect {
                Effect::Mutation(_, DependencyMutationKind::Replace(ty)) => {
                    self.input.graceful_types.contains(ty)
                }
                Effect::Mutation(
                    _,
                    DependencyMutationKind::Write | DependencyMutationKind::Escape,
                )
                | Effect::Escape
                | Effect::Unknown
                | Effect::Ambient
                | Effect::Cleanup(_) => true,
                _ => false,
            })
        });

        if !relevant {
            return;
        }

        let (protected, unknown) =
            memory.transitive_regions([Target::storage(receiver.clone())], |slot| {
                self.input.terminal_dependencies
                    && !state.fully_moved.contains(&receiver.root)
                    && slot.observation_path().is_none_or(|path| {
                        !self.input.opaque_values.contains(&path)
                            && !state
                                .definitely_absent
                                .keys()
                                .any(|absent| absent.contains(&path))
                    })
            });

        let overlaps = |target: &Target| {
            unknown && memory.unknown_reference_may_reach(target)
                || protected.iter().any(|part| {
                    memory.target_relationship(target, part) != StorageRelationship::Disjoint
                })
        };

        let introduced = effects
            .into_iter()
            .flatten()
            .find_map(|effect| match effect {
                Effect::Mutation(target, DependencyMutationKind::Replace(ty))
                    if self.input.graceful_types.contains(ty) && overlaps(target) =>
                {
                    Some(*ty)
                }
                _ => None,
            });

        let opaque = effects.is_none_or(|effects| {
            effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::Unknown | Effect::Ambient | Effect::Escape | Effect::Cleanup(_)
                )
            })
        });

        let inputs = effects.is_none_or(|effects| {
            effects.iter().any(|effect| {
                matches!(
                    effect,
                    Effect::Unknown | Effect::Escape | Effect::Cleanup(_)
                )
            })
        });

        let reached = memory.reachable_targets(roots.into_iter().filter(|_| inputs));

        let unproved = effects.into_iter().flatten().any(|effect| {
            matches!(effect, Effect::Mutation(target, DependencyMutationKind::Write | DependencyMutationKind::Escape) if overlaps(target))
        }) || opaque && (unknown || reached.iter().any(overlaps));

        if introduced.is_some() || unproved {
            self.report_terminal_operation(operation, introduced);
        }
    }

    fn report_terminal_operation(
        &mut self,
        operation: AnalysisOperationKind,
        introduced: Option<TypeId>,
    ) {
        if !self.terminal_diagnostics.insert(operation.occurrence()) {
            return;
        }

        let origin = crate::diagnostic::bound_node_origin(self.request, operation.node())
            .expect("checked destructor operation retains its source");

        let kind = if introduced.is_some() {
            DiagnosticKind::CheckingDestructorOwnershipIntroduction
        } else {
            DiagnosticKind::CheckingUnprovedDestructorEffects
        };

        let span = self.request.source(origin.source_anchor()).span();

        let mut diagnostic = Diagnostic::new(
            crate::diagnostic::diagnostic_id(self.diagnostics.len()),
            kind,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::DestructorOwnershipOperation,
            span,
        ));

        if let Some(ty) = introduced {
            match crate::diagnostic::diagnostic_type(self.request.context(), ty) {
                Ok(ty) => diagnostic = diagnostic.with_arg(DiagnosticArg::actual_type(ty)),
                Err(error) => {
                    self.query_failure = Some(error);
                    return;
                }
            }
        }

        self.diagnostics.add(diagnostic);
    }
}
