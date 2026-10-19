use std::collections::BTreeMap;

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, SemanticSelection};
use bray_symbols::DependencySubjectRoot;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn record_address_escapes(
        &mut self,
        state: &mut StorageFlowState,
        operation: crate::analysis::model::AnalysisOperationKind,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        roots: &[Target],
    ) {
        let captures = self
            .dependencies
            .calls()
            .get(&operation.occurrence())
            .is_some_and(|contract| contract.deferred_effects().is_some());

        let memory = operation
            .occurrence()
            .expression()
            .and_then(|expression| self.memory.operation(expression));

        if memory.is_some_and(|operation| {
            matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate
                    | bray_bound_tree::CheckedMemoryOperationKind::Deallocate
                    | bray_bound_tree::CheckedMemoryOperationKind::RawBufferSetInitializedCount
            )
        }) {
            // These compiler-owned operations retire backing or update its live
            // prefix. They neither store an input address nor invoke payload code.
            return;
        }

        let exposed_integer = memory.is_some_and(|operation| {
            matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::ExposeAddress { .. }
            )
        });

        let direct_write = memory.is_some()
            || self
                .storage
                .node_plans(operation.node())
                .any(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment);

        let may_store = direct_write
            && effects.is_none_or(|effects| {
                effects
                    .iter()
                    .any(|effect| effect.may_retain_address(&state.memory))
            });

        if let Some(operation) = memory
            && matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::Write { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::VolatileWrite { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::UninitWrite { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::RawBufferPush { .. }
            )
        {
            // Dereferencing the destination does not publish its address. Only
            // references contained in the stored value can escape through this write.
            let value = operation.arguments()[1];
            let mut stored = Vec::new();

            self.append_reference_roots(
                state,
                value,
                self.expression_reference(state, value),
                &mut stored,
            );

            self.record_escape_effects(&mut state.memory, Some(&[]), &stored, may_store);
        } else {
            self.record_escape_effects(
                &mut state.memory,
                effects,
                roots,
                captures || exposed_integer || may_store,
            );
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn record_escape_effects(
        &mut self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        roots: &[Target],
        retain_inputs: bool,
    ) {
        use crate::analysis::storage_flow::memory::Effect;
        use bray_symbols::DependencyMutationKind;

        let all_inputs = retain_inputs
            || effects.is_none_or(|effects| {
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::Escape))
            });

        let targets = effects
            .into_iter()
            .flatten()
            .filter_map(|effect| match effect {
                Effect::Mutation(target, DependencyMutationKind::Escape) => Some(target),
                _ => None,
            })
            .chain(roots.iter().filter(|_| all_inputs))
            .cloned()
            .collect::<Vec<_>>();

        for target in &targets {
            self.record_target_effect(target, DependencyMutationKind::Escape);
        }

        memory.escape_targets(targets);

        // Generic cleanup retains its conditional effect in the exported summary.
        // Do not turn it into an unconditional escape before instantiation.
        if effects.is_some_and(|effects| {
            effects
                .iter()
                .any(|effect| matches!(effect, Effect::Cleanup(_)))
        }) {
            memory.escape_targets(roots.iter().cloned());
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_roots(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
    ) -> Vec<Target> {
        let mut roots = Vec::new();

        if let Some(expression) = occurrence
            .expression()
            .and_then(|id| self.request.view().expression(id))
        {
            // An awaited task retains inputs even though its invocation is deferred.
            let receiver = invocation_argument(
                self.execution.semantics.selections(),
                occurrence,
                DependencySubjectRoot::Receiver,
            );

            for argument in expression.child_expressions().chain(receiver) {
                // An implicit receiver borrow already names its physical target. Its
                // retained lifetime anchor can be a different object, such as a guard.
                let source = if receiver == Some(argument) {
                    self.invocation_reference(state, occurrence, DependencySubjectRoot::Receiver)
                } else {
                    self.expression_reference(state, argument)
                };

                self.append_reference_roots(state, argument, source, &mut roots);
            }
        }

        roots
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn assignment_roots(
        &self,
        state: &StorageFlowState,
        operation: crate::analysis::model::AnalysisOperationKind,
    ) -> Vec<Target> {
        let Some(bray_bound_tree::BoundExpression::Assignment(assignment)) = operation
            .occurrence()
            .expression()
            .and_then(|expression| self.request.view().expression(expression))
        else {
            return Vec::new();
        };

        let [_, value] = assignment.operands() else {
            unreachable!("checked assignment has a destination and stored value");
        };

        let mut roots = Vec::new();

        self.append_reference_roots(
            state,
            *value,
            self.expression_reference(state, *value),
            &mut roots,
        );

        roots
    }

    fn append_reference_roots(
        &self,
        state: &StorageFlowState,
        argument: BoundExpressionId,
        source: Option<Source>,
        roots: &mut Vec<Target>,
    ) {
        if let Some(place) = self.argument_storage(argument) {
            roots.extend(
                state
                    .memory
                    .contained_regions(&Target::storage(place))
                    .cloned(),
            );
        }

        let regions = source.and_then(|source| source.regions(&state.memory));

        if let Some(regions) = regions {
            // The physical target is authoritative. Its lifetime anchor need
            // not be reachable through the reference, as with a lock guard.
            roots.extend(regions);
        } else {
            // Opaque task representations can retain a loan without
            // publishing a physical field. Its possible targets bound effects,
            // but never establish exact reference equality.
            for dependency in self.liveness.retained_by(argument) {
                if let bray_bound_tree::BoundDependencySubject::BorrowCapability(capability) =
                    dependency
                {
                    roots.extend(
                        state
                            .memory
                            .borrow_targets(capability)
                            .into_iter()
                            .flatten(),
                    );
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_borrow_origins(
        &self,
        occurrence: BoundExecutionSite,
    ) -> BTreeMap<BoundExpressionId, std::collections::BTreeSet<Place>> {
        let mut borrow_origins = BTreeMap::new();

        if let Some(SemanticSelection::Call(call)) = occurrence
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        {
            let arguments = call
                .arguments()
                .iter()
                .filter_map(|argument| match argument {
                    bray_bound_tree::SelectedArgument::Explicit {
                        ordinal,
                        conversion,
                        ..
                    } if matches!(
                        self.request
                            .semantic_values()
                            .type_data(conversion.target_type())
                            .as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    ) =>
                    {
                        Some(DependencySubjectRoot::Parameter(
                            bray_symbols::SymbolOrdinal::new(*ordinal),
                        ))
                    }
                    _ => None,
                })
                .chain(
                    call.receiver()
                        .filter(|receiver| {
                            matches!(
                                receiver.mode(),
                                bray_symbols::ReceiverMode::Shared
                                    | bray_symbols::ReceiverMode::Mutable
                            )
                        })
                        .map(|_| DependencySubjectRoot::Receiver),
                );

            for root in arguments {
                let argument =
                    invocation_argument(self.execution.semantics.selections(), occurrence, root)
                        .expect("selected borrow input retains its argument");

                let owners = self
                    .liveness
                    .retained_by(argument)
                    .filter_map(|subject| {
                        let access = match subject {
                            bray_bound_tree::BoundDependencySubject::BorrowCapability(borrow) => {
                                let capability = self
                                    .storage
                                    .borrow_capability(borrow)
                                    .expect("checked lifetime dependency retains its loan");

                                if capability.entry_binding().is_some() {
                                    return None;
                                }

                                capability.access()
                            }
                            bray_bound_tree::BoundDependencySubject::StorageAccess(access)
                                if self.input.retained_storage.contains(&access) =>
                            {
                                access
                            }
                            _ => return None,
                        };

                        if self.borrow_reaches_external_storage(access) {
                            return None;
                        }

                        self.storage.logical_path(access).cloned()
                    })
                    .collect();

                // Physical provenance can differ from the lifetime anchor of a trusted borrow.
                // Consume the ordinary dependency analysis instead of reconstructing ownership.
                borrow_origins.insert(argument, owners);
            }
        }

        borrow_origins
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_place(
        &self,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Place> {
        if root == DependencySubjectRoot::Result {
            return self.invocation_result(occurrence);
        }

        if let Some(argument) =
            invocation_argument(self.execution.semantics.selections(), occurrence, root)
        {
            return self.argument_storage(argument);
        }

        if matches!(occurrence, BoundExecutionSite::ScopedExit(_))
            && root == DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
        {
            // A transition owns its observed path while sharing the immutable projection list.
            return self
                .storage
                .occurrence_plans(occurrence)
                .next()
                .and_then(|plan| self.storage.logical_path(plan.access()))
                .cloned();
        }

        None
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_reference(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Source> {
        if let Some(argument) =
            invocation_argument(self.execution.semantics.selections(), occurrence, root)
        {
            let owner = if matches!(occurrence, BoundExecutionSite::ScopedEnter(_)) {
                occurrence.expression()?
            } else {
                argument
            };

            if root == DependencySubjectRoot::Receiver
                && let Some((capability, _)) =
                    self.storage.borrow_capability_entries().find(|(id, loan)| {
                        loan.expression() == Some(owner) && state.memory.has_borrow(*id)
                    })
            {
                return Some(Source::Borrow(capability));
            }

            return self.expression_reference(state, argument);
        }

        self.invocation_place(occurrence, root)
            .map(|place| Source::Value(Target::storage(place)))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_locations(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        subject: &bray_symbols::DependencySubject,
    ) -> Option<Vec<Target>> {
        let path = self
            .dependencies
            .calls()
            .get(&occurrence)?
            .reference_path(subject)?;

        if let Some(argument) = invocation_argument(
            self.execution.semantics.selections(),
            occurrence,
            subject.subject_root(),
        ) {
            return self.call_reference_locations(state, argument, path);
        }

        let start = Target::storage(self.invocation_place(occurrence, subject.subject_root())?);

        Some(vec![state.memory.reference_location(start, path, None)?])
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_result(
        &self,
        occurrence: BoundExecutionSite,
    ) -> Option<Place> {
        match occurrence {
            BoundExecutionSite::ScopedEnter(expression) => self.storage.identity_entries()
                .find_map(|(id, identity)| matches!(identity, bray_bound_tree::StorageIdentity::ScopedCapability { expression: owner, .. } if owner == expression).then_some(Place::root(id))),
            BoundExecutionSite::ScopedExit(_) => None,
            BoundExecutionSite::Cleanup(_) => panic!("cleanup suspension has no invocation result"),
            BoundExecutionSite::Node(_) => self.expression_result_storage(occurrence.expression()?),
        }
    }
}

pub(super) fn invocation_argument(
    selections: &bray_bound_tree::CheckedSemanticSelections,
    occurrence: BoundExecutionSite,
    root: DependencySubjectRoot,
) -> Option<BoundExpressionId> {
    match selections.expression(occurrence.expression()?)? {
        SemanticSelection::Call(call) => {
            crate::dependency::result_argument(call.arguments(), call.receiver(), root)
        }
        SemanticSelection::ScopedUse(scoped)
            if matches!(occurrence, BoundExecutionSite::ScopedEnter(_))
                && root == DependencySubjectRoot::Receiver =>
        {
            Some(scoped.initializer())
        }
        _ => None,
    }
}
