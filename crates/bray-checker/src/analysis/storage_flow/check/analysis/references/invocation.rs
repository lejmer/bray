use std::collections::{BTreeMap, BTreeSet};

use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, SemanticSelection};
use bray_symbols::DependencySubjectRoot;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn address_escapes(
        &mut self,
        state: &mut StorageFlowState,
        operation: crate::analysis::model::AnalysisOperationKind,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        roots: &[Target],
        normal_effects: bool,
    ) -> BTreeSet<Target> {
        let memory = matches!(operation.occurrence(), BoundExecutionSite::Node(_))
            .then(|| {
                operation
                    .occurrence()
                    .expression()
                    .and_then(|expression| self.memory.operation(expression))
            })
            .flatten();

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
            return BTreeSet::new();
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
                .occurrence_plans(operation.occurrence())
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
                value.into(),
                self.argument_storage(value),
                self.expression_reference(state, value),
                &mut stored,
            );

            self.escape_effects(&state.memory, Some(&[]), &stored, may_store, normal_effects)
        } else {
            self.escape_effects(
                &state.memory,
                effects,
                roots,
                exposed_integer || may_store,
                normal_effects,
            )
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn escape_effects(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        roots: &[Target],
        retain_inputs: bool,
        normal_effects: bool,
    ) -> BTreeSet<Target> {
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

        let mut escaped = memory.escaped_regions(targets);

        for target in &escaped {
            self.record_target_effect(
                memory,
                target,
                DependencyMutationKind::Escape,
                normal_effects,
            );
        }

        // Generic cleanup retains its conditional effect in the exported summary.
        // Do not turn it into an unconditional escape before instantiation.
        if !all_inputs
            && effects.is_some_and(|effects| {
                effects
                    .iter()
                    .any(|effect| matches!(effect, Effect::Cleanup(_)))
            })
        {
            escaped.extend(memory.escaped_regions(roots.iter().cloned()));
        }

        escaped
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_roots(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
    ) -> Vec<Target> {
        let mut roots = Vec::new();

        if matches!(
            occurrence,
            BoundExecutionSite::Iterate(_)
                | BoundExecutionSite::Next(_)
                | BoundExecutionSite::ScopedEnter(_)
                | BoundExecutionSite::ScopedExit(_)
        ) {
            let selection = self
                .execution
                .semantics
                .selections()
                .expression(
                    occurrence
                        .expression()
                        .expect("implicit invocation has its owner"),
                )
                .expect("implicit invocation retains its selected protocol");

            let root = if matches!(occurrence, BoundExecutionSite::ScopedExit(_)) {
                DependencySubjectRoot::Parameter(bray_symbols::SymbolOrdinal::new(0))
            } else {
                DependencySubjectRoot::Receiver
            };

            let input = selection
                .invocation_input_site(occurrence, root)
                .expect("implicit invocation retains its sole actual input");

            self.append_reference_roots(
                state,
                input,
                self.invocation_place(occurrence, root),
                self.invocation_reference(state, occurrence, root),
                &mut roots,
            );

            return roots;
        }

        if let Some(expression) = occurrence.expression()
            && let Some(selection) = self.execution.semantics.selections().expression(expression)
            && selection.evaluates_defaults()
        {
            if let BoundExecutionSite::CallInput { ordinal, .. } = occurrence {
                let input = selection
                    .input(ordinal)
                    .expect("input roots retain their selected input");

                if let Some(argument) = input.expression() {
                    self.append_reference_roots(
                        state,
                        argument.into(),
                        self.argument_storage(argument),
                        self.invocation_reference(
                            state,
                            occurrence,
                            DependencySubjectRoot::Receiver,
                        ),
                        &mut roots,
                    );

                    return roots;
                }

                if !matches!(
                    input.default_provider(),
                    Some(bray_bound_tree::DefaultValueProvider::CallableParameter(_))
                ) {
                    return roots;
                }
            }

            for input in selection.inputs() {
                let root = input.root();

                let ordinal = selection
                    .input_ordinal(root)
                    .expect("selected input has an ordinal");

                if matches!(occurrence, BoundExecutionSite::CallInput { ordinal: current, .. } if ordinal >= current)
                {
                    continue;
                }

                self.append_reference_roots(
                    state,
                    BoundExecutionSite::CallInput {
                        expression,
                        ordinal,
                    },
                    self.invocation_place(occurrence, root),
                    self.invocation_reference(state, occurrence, root),
                    &mut roots,
                );
            }

            return roots;
        }

        if let Some(expression) = occurrence
            .expression()
            .and_then(|id| self.request.view().expression(id))
        {
            // An awaited task retains inputs even though its invocation is deferred.
            let receiver = invocation_argument(
                self.request.view(),
                self.execution.semantics.selections(),
                occurrence,
                DependencySubjectRoot::Receiver,
            );

            let callee = match expression {
                bray_bound_tree::BoundExpression::Call(call) => Some(call.callee()),
                bray_bound_tree::BoundExpression::ErrorCall(call) => Some(call.callee()),
                _ => None,
            };

            for argument in expression.child_expressions().chain(receiver) {
                // An implicit receiver borrow already names its physical target. Its
                // retained lifetime anchor can be a different object, such as a guard.
                let source = if receiver == Some(argument) {
                    self.invocation_reference(state, occurrence, DependencySubjectRoot::Receiver)
                } else {
                    self.expression_reference(state, argument)
                };

                self.append_reference_roots(
                    state,
                    argument.into(),
                    // Invocation can reach a callee's captures, not its descriptor's storage.
                    (callee != Some(argument))
                        .then(|| self.argument_storage(argument))
                        .flatten(),
                    source,
                    &mut roots,
                );
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
            (*value).into(),
            self.argument_storage(*value),
            self.expression_reference(state, *value),
            &mut roots,
        );

        roots
    }

    fn append_reference_roots(
        &self,
        state: &StorageFlowState,
        argument: BoundExecutionSite,
        place: Option<Place>,
        source: Option<Source>,
        roots: &mut Vec<Target>,
    ) {
        let regions = source.and_then(|source| source.regions(&state.memory));

        if regions.is_none()
            && let Some(regions) = place.as_ref().and_then(|place| {
                self.input
                    .inline_regions(&state.memory, self.storage, place)
            })
        {
            roots.extend(regions);
            return;
        }

        if let Some(place) = &place {
            roots.extend(
                state
                    .memory
                    .contained_regions(&Target::storage(place.clone()))
                    .cloned(),
            );

            if regions.is_none() && !self.input.has_complete_references(self.storage, place) {
                // Unknown value contents remain symbolic entry values until instantiation.
                roots.push(Target::storage(place.clone()));
            }
        }

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
                            .borrow_regions(capability)
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
    ) -> BTreeMap<BoundExecutionSite, std::collections::BTreeSet<Place>> {
        let mut borrow_origins = BTreeMap::new();

        if let Some(expression) = occurrence.expression()
            && let Some(selection) = self.execution.semantics.selections().expression(expression)
        {
            let current = match occurrence {
                BoundExecutionSite::CallInput { ordinal, .. } => Some((
                    ordinal,
                    selection
                        .input(ordinal)
                        .expect("borrow origins retain the exact input"),
                )),
                _ => None,
            };

            let source = current.and_then(|(_, input)| {
                input
                    .expression()
                    .map(|source| (source.into(), input.ty(self.request.semantic_values())))
            });

            let prefix = current.is_none_or(|(_, input)| {
                matches!(
                    input.default_provider(),
                    Some(bray_bound_tree::DefaultValueProvider::CallableParameter(_))
                )
            });

            let inputs =
                source
                    .into_iter()
                    .chain(selection.inputs().filter(|_| prefix).filter_map(|input| {
                        let ordinal = selection
                            .input_ordinal(input.root())
                            .expect("selected input has its ordinal");

                        if current.is_some_and(|(current, _)| ordinal >= current) {
                            return None;
                        }

                        Some((
                            selection.input_site(expression, input.root())?,
                            input.ty(self.request.semantic_values()),
                        ))
                    }));

            for (argument, ty) in inputs {
                if !matches!(
                    self.request.semantic_values().type_data(ty).as_ref(),
                    bray_symbols::TypeData::Borrow { .. }
                ) {
                    continue;
                }

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

        if matches!(
            occurrence,
            BoundExecutionSite::Iterate(_)
                | BoundExecutionSite::Next(_)
                | BoundExecutionSite::ScopedEnter(_)
                | BoundExecutionSite::ScopedExit(_)
        ) {
            let selection = self
                .execution
                .semantics
                .selections()
                .expression(occurrence.expression()?)?;

            selection.invocation_input_site(occurrence, root)?;

            return self
                .storage
                .occurrence_plans(occurrence)
                .find(|plan| plan.purpose() != bray_bound_tree::StorageAccessPurpose::Initialize)
                .and_then(|plan| self.storage.logical_path(plan.access()))
                .cloned();
        }

        let expression = occurrence.expression()?;

        if let Some(selection) = self.execution.semantics.selections().expression(expression)
            && selection.evaluates_defaults()
            && let Some(BoundExecutionSite::CallInput {
                expression,
                ordinal,
            }) = selection.invocation_input_site(occurrence, root)
        {
            let access = self
                .storage
                .invocation_input(expression, ordinal)
                .expect("selected input root retains its staged storage");

            return self.storage.logical_path(access).cloned();
        }

        if let Some(argument) = invocation_argument(
            self.request.view(),
            self.execution.semantics.selections(),
            occurrence,
            root,
        ) {
            return self.argument_storage(argument);
        }

        None
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn record_input_targets(
        &mut self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        let Some(selection) = occurrence
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        else {
            return;
        };

        let mut mutable_targets = Vec::new();
        let mut complete = true;

        for (ordinal, root, ty) in
            selection.invocation_reference_inputs(occurrence, self.request.semantic_values())
        {
            let target = self
                .invocation_reference(state, occurrence, root)
                .and_then(|source| match source {
                    Source::Borrow(capability) => state.memory.borrow_target(capability, ty),
                    Source::Value(slot) => state.memory.value_target(&slot, ty),
                });

            let mutable = matches!(occurrence, BoundExecutionSite::Next(_))
                || matches!(selection, SemanticSelection::Iteration(iteration)
                    if matches!(occurrence, BoundExecutionSite::Iterate(_)) && iteration.mode() == bray_bound_tree::IterationSourceMode::Mutable)
                || selection.input(ordinal).is_some_and(|input| {
                    matches!(
                        self.request
                            .semantic_values()
                            .type_data(input.ty(self.request.semantic_values()))
                            .as_ref(),
                        bray_symbols::TypeData::Borrow {
                            kind: bray_symbols::BorrowKind::Mutable,
                            ..
                        }
                    )
                });

            if mutable {
                match target.as_ref() {
                    Some(target) => mutable_targets.push(target.clone()),
                    None => complete = false,
                }
            }

            self.record_state_target(
                bray_bound_tree::StorageStateTarget::Input {
                    invocation: occurrence,
                    ordinal,
                },
                target,
            );
        }

        if self.publish
            && self
                .execution
                .semantics
                .selections()
                .invoked_callable(occurrence)
                .is_some()
        {
            let invalidation = if complete {
                self.target_observations(&state.memory, mutable_targets.iter())
            } else {
                bray_bound_tree::StorageInvalidation::unknown()
            };

            self.observation_invalidations
                .entry(occurrence)
                .and_modify(|previous| previous.merge(&invalidation))
                .or_insert(invalidation);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn invocation_reference(
        &self,
        state: &StorageFlowState,
        occurrence: BoundExecutionSite,
        root: DependencySubjectRoot,
    ) -> Option<Source> {
        if matches!(
            occurrence,
            BoundExecutionSite::Iterate(_)
                | BoundExecutionSite::Next(_)
                | BoundExecutionSite::ScopedEnter(_)
                | BoundExecutionSite::ScopedExit(_)
        ) {
            if root == DependencySubjectRoot::Result {
                return self
                    .invocation_result(occurrence)
                    .map(|place| Source::Value(Target::storage(place)));
            }

            let selection = self
                .execution
                .semantics
                .selections()
                .expression(occurrence.expression()?)?;

            selection.invocation_input_site(occurrence, root)?;

            let plan = self
                .storage
                .occurrence_plans(occurrence)
                .find(|plan| plan.purpose() != bray_bound_tree::StorageAccessPurpose::Initialize)
                .expect("implicit invocation retains its actual input");

            if let Some(capability) = self.storage.borrow_for_plan(plan)
                && state.memory.has_borrow(capability)
            {
                return Some(Source::Borrow(capability));
            }

            return self
                .invocation_place(occurrence, root)
                .map(|place| Source::Value(Target::storage(place)));
        }

        if let Some(argument) = invocation_argument(
            self.request.view(),
            self.execution.semantics.selections(),
            occurrence,
            root,
        ) {
            if let Some(SemanticSelection::Operation(operation)) = occurrence
                .expression()
                .and_then(|expression| self.execution.semantics.selections().expression(expression))
                && let Some((_, input @ crate::dependency::OperationInput::Borrow(_, _))) =
                    crate::dependency::operation_inputs(
                        self.request.view(),
                        occurrence.expression()?,
                        operation,
                    )
                    .into_iter()
                    .find(|(actual, _)| *actual == root)
            {
                let capability = self
                    .storage
                    .borrow_for_plan(
                        input
                            .borrow_plan(self.storage)
                            .expect("borrow input has a plan"),
                    )
                    .expect("checked borrow plan retains its capability");

                return state
                    .memory
                    .has_borrow(capability)
                    .then_some(Source::Borrow(capability));
            }

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
            self.request.view(),
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
            BoundExecutionSite::CallInput { expression, ordinal } => {
                let access = self.storage.invocation_input(expression, ordinal)
                    .expect("input completion retains its exact slot");

                self.storage.logical_path(access).cloned()
            }
            site @ (BoundExecutionSite::Iterate(_) | BoundExecutionSite::Next(_)) =>
                self.storage.invocation_result(site).and_then(|access| self.storage.logical_path(access)).cloned(),
            BoundExecutionSite::ScopedEnter(expression) => self.storage.identity_entries()
                .find_map(|(id, identity)| matches!(identity, bray_bound_tree::StorageIdentity::ScopedCapability { expression: owner, .. } if owner == expression).then_some(Place::root(id))),
            BoundExecutionSite::ScopedExit(_) => None,
            BoundExecutionSite::Cleanup(_) => panic!("cleanup suspension has no invocation result"),
            BoundExecutionSite::Node(_) => self.expression_result_storage(occurrence.expression()?),
        }
    }
}

pub(super) fn invocation_argument(
    view: bray_bound_tree::BoundUnitView<'_>,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    occurrence: BoundExecutionSite,
    root: DependencySubjectRoot,
) -> Option<BoundExpressionId> {
    let selection = selections.expression(occurrence.expression()?)?;

    if selection.evaluates_defaults() {
        return match selection.invocation_input_site(occurrence, root)? {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                Some(expression)
            }
            _ => None,
        };
    }

    match selection {
        SemanticSelection::Call(call) => {
            crate::dependency::result_argument(call.arguments(), call.receiver(), root)
        }
        SemanticSelection::Operation(operation) => {
            crate::dependency::operation_inputs(view, occurrence.expression()?, operation)
                .into_iter()
                .find(|(actual, _)| *actual == root)
                .and_then(|(_, input)| input.expression())
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
