use super::model::ExecutionFlowDomain;

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId};

use crate::CheckerRequestContext;
use crate::execution_guarantees::{ExecutionCondition, expression_condition};

use super::super::state::ExecutionState;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        state.expressions.get(&expression)
            .or_else(|| state.pending_results.get(&expression))
            .cloned()
            .unwrap_or(ExecutionCondition::Unknown)
    }

    pub(in crate::analysis::guarantee) fn returned_value(&self, state: &ExecutionState) -> ExecutionCondition {
        self.storage.binding(bray_bound_tree::StorageBindingTarget::Result)
            .and_then(|binding| match binding {
                bray_bound_tree::StorageBinding::Identity(identity) =>
                    crate::ExecutionPlace::storage_identity(self.storage, identity),
                _ => None,
            })
            .and_then(|place| place.value_in(&state.current))
            .unwrap_or(ExecutionCondition::Unknown)
    }

    pub(in crate::analysis::guarantee) fn publish_return(&self, state: &mut ExecutionState, value: ExecutionCondition) {
        let Some(bray_bound_tree::StorageBinding::Identity(identity)) =
            self.storage.binding(bray_bound_tree::StorageBindingTarget::Result) else {
                panic!("a returned value retains its checked callable result storage");
            };
        let place = crate::ExecutionPlace::storage_identity(self.storage, identity)
            .expect("callable result storage has a current value place");

        state.assign(place, value, None);
    }

    pub(in crate::analysis::guarantee) fn assume_produced_variant(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        value: &ExecutionCondition,
    ) {
        let Some(active) = value.known_variant() else { return; };
        let ty = self.semantics.types().expression(expression)
            .expect("a produced union retains its checked type").ty();
        let Some(variants) = self.union_variants.get(&ty) else { return; };
        let owner = std::sync::Arc::new(value.owner_identity().clone());

        // Record every inactive case while the constructor owns this knowledge.
        // Pairwise joins can then preserve exclusions across arbitrarily nested choices.
        for variant in variants.iter().copied() {
            state.assumptions.insert((ExecutionCondition::VariantTest(variant, owner.clone()), variant == active));
        }
    }

    /// Form one expression result at its defining execution transition. Every
    /// operand is an actual evaluated result, never another syntax evaluation.
    pub(super) fn produce_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        expression_condition(
            self.request.unit(),
            self.semantics,
            self.request.semantic_values(),
            self.request.available_compiler_known_symbols(),
            Some(self.storage),
            self.literals,
            self.pattern_conditions,
            &state.current,
            &|source| (source != expression).then(|| self.value(state, source)),
            expression,
            &mut { ExecutionCondition::WORK_LIMIT },
            self.request.trusted_contracts().map(|input| &input.predicates),
        )
    }

    /// A checked copy forms immutable data with its own evaluation identity.
    /// Moving a newly produced result still transfers the actual complete owner.
    pub(in crate::analysis::guarantee) fn copied_access_value(
        &self,
        identity: ExecutionCondition,
        value: ExecutionCondition,
        plan: bray_bound_tree::StorageAccessPlan,
    ) -> ExecutionCondition {
        if self.storage.access_plan_role(plan) != bray_bound_tree::StorageAccessPlanRole::SourceMaterialization {
            return value;
        }

        if self.storage.effective_purpose(plan) != bray_bound_tree::StorageAccessPurpose::Copy {
            return value;
        }

        let ty = self.storage.access(plan.access())
            .expect("checked copy retains its reached type").reached_type();

        if matches!(self.request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. }) {
            return value.borrowed();
        }

        if crate::representation::type_representation(self.request, ty)
            .is_some_and(crate::representation::is_scalar_value)
        {
            // A scalar copy reads current data independently of its source's
            // owner or loan wrapper. The immutable data is shared by this read.
            let contents = value.computed_value().clone();
            return if contents == ExecutionCondition::Unknown {
                ExecutionCondition::opaque_scalar(identity)
            } else { ExecutionCondition::scalar_observation(identity, contents) };
        }

        if !matches!(value.ownership_identity(),
                Some((crate::execution_guarantees::ExecutionObservationKind::Owned, _)))
        {
            return value;
        }

        ExecutionCondition::scalar_observation(identity, value)
    }

    /// Selects the actual output slot, independently of an invocation's source access.
    pub(in crate::analysis::guarantee) fn invocation_result_place(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionPlace> {
        use bray_bound_tree::BoundExecutionSite as Site;

        match invocation {
            Site::Node(AnyBoundNodeId::Expression(expression)) => self.storage.value_slot(expression)
                .and_then(|identity| crate::ExecutionPlace::storage_identity(self.storage, identity)),
            Site::CallInput { .. } | Site::Iterate(_) | Site::Next(_) | Site::ScopedEnter(_) => self.input_place(invocation),
            _ => None,
        }
    }

    pub(in crate::analysis::guarantee) fn borrowed_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        let value = self.value(state, expression);

        let Some(flow) = self.checked_body.map(bray_bound_tree::CheckedBodySemantics::storage_flow) else {
            return value;
        };

        let Some(completion) = flow.completion(bray_bound_tree::StorageMutation::Operation(expression.into())) else {
            return value;
        };

        let observe = |path: &bray_bound_tree::StoragePath| {
            let target = completion.references().get(path)?;

            let target = (target).value_observation_path()?;

            self.current_storage_value(state, &target)
        };

        let contents = self.storage.node_plans(expression.into())
            .filter_map(|plan| self.storage.logical_path(plan.access()))
            .find_map(observe)
            .or_else(|| self.storage.temporary(expression)
                .and_then(|root| observe(&bray_bound_tree::StoragePath::root(root))));

        // The evaluated reference retains its identity even if its source binding
        // was replaced. Physical completion names the storage it still reaches.
        match contents {
            Some(contents) => value.with_referent(contents),
            None => value,
        }
    }

    pub(in crate::analysis) fn invocation_value(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> ExecutionCondition {
        match invocation {
            bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ) => self.value(state, expression),
            bray_bound_tree::BoundExecutionSite::CallInput {
                expression,
                ordinal,
            } => {
                let access = self
                    .storage
                    .invocation_input(expression, ordinal)
                    .expect("input value has its planned slot");

                let place = self
                    .storage
                    .logical_path(access)
                    .and_then(|path| crate::ExecutionPlace::storage_path(self.storage, path))
                    .expect("input slot has an execution place");

                place
                    .value_in(&state.current)
                    .unwrap_or(ExecutionCondition::Unknown)
            }
            bray_bound_tree::BoundExecutionSite::Iterate(_)
            | bray_bound_tree::BoundExecutionSite::Next(_)
            | bray_bound_tree::BoundExecutionSite::ScopedEnter(_) => {
                let place = self
                    .input_place(invocation)
                    .expect("implicit invocation result has an execution place");

                if state.current.availability(&place) == super::super::current::ValueAvailability::Absent {
                    return ExecutionCondition::Unknown;
                }

                place
                    .value_in(&state.current)
                    .unwrap_or(ExecutionCondition::Unknown)
            }
            bray_bound_tree::BoundExecutionSite::ScopedExit(_) => ExecutionCondition::Unknown,
            bray_bound_tree::BoundExecutionSite::Cleanup(node)
            | bray_bound_tree::BoundExecutionSite::Node(node) => {
                panic!("invocation value requires an expression owner, got {node:?}")
            }
        }
    }

    pub(in crate::analysis::guarantee) fn observation_type(
        &self,
        value: &ExecutionCondition,
    ) -> Option<bray_symbols::TypeId> {
        match value {
            ExecutionCondition::ScalarValue(identity) | ExecutionCondition::PostState(_, identity) => self.observation_type(identity),
            ExecutionCondition::Input(place)
            | ExecutionCondition::Joined(_, _, place) => self
                .observed_types
                .get(place)
                .copied()
                .flatten()
                .or_else(|| match place.root {
                    crate::execution_guarantees::ExecutionInput::Storage(identity)
                        if place.projections.is_empty() =>
                    {
                        self.storage.identity_type(identity)
                    }
                    _ => None,
                }),
            ExecutionCondition::Call(instance, _, _, _, _)
                if Some(instance.definition().symbol()) == self.sequence_length =>
            {
                Some(crate::representation::representation_type(
                    self.request,
                    bray_compiler_known::RepresentationRole::ScalarUsize,
                ))
            }
            ExecutionCondition::Expression(expression) => self
                .semantics
                .types()
                .expression(*expression)
                .map(|entry| entry.ty()),
            ExecutionCondition::Literal(value) => Some(value.ty()),
            _ => None,
        }
    }

    pub(in crate::analysis::guarantee) fn unsigned_value(
        &self,
        value: &ExecutionCondition,
    ) -> bool {
        let ty = self.observation_type(value);

        ty.and_then(|ty| crate::representation::type_representation(self.request, ty))
            .and_then(bray_compiler_known::RepresentationRole::integer_representation)
            .is_some_and(|representation| {
                matches!(
                    representation,
                    bray_compiler_known::IntegerRepresentation::Unsigned(_)
                        | bray_compiler_known::IntegerRepresentation::TargetUnsigned
                )
            })
    }

    pub(in crate::analysis::guarantee) fn storage_identity_value(
        &self,
        state: &ExecutionState,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> ExecutionCondition {
        if let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, identity) {
            return place
                .value_in(&state.current)
                .unwrap_or(ExecutionCondition::Unknown);
        }

        match self
            .storage
            .identity(identity)
            .expect("checked storage retains its identity")
        {
            bray_bound_tree::StorageIdentity::Temporary(expression)
            | bray_bound_tree::StorageIdentity::Allocation(expression)
            | bray_bound_tree::StorageIdentity::Result(AnyBoundNodeId::Expression(expression)) => {
                // Physical frame storage can find the actual retained frame after
                // its expression-cache lifetime ends. A missing frame is not recreated.
                state.deferred_invocations.keys().find(|owner| {
                    matches!(owner, ExecutionCondition::Expression(source) if *source == expression)
                }).cloned().unwrap_or_else(|| self.value(state, expression))
            }
            _ => ExecutionCondition::Unknown,
        }
    }

    pub(in crate::analysis::guarantee) fn storage_path_value(
        &self,
        state: &ExecutionState,
        path: &bray_bound_tree::StoragePath,
    ) -> ExecutionCondition {
        if let Some(value) = self.current_storage_value(state, path)
        {
            return value;
        }

        path.projections.iter().fold(
            self.storage_identity_value(state, path.root),
            |value, projection| value.project(*projection),
        )
    }

    /// Reads an actual retained logical value, excluding unrepresented physical backing.
    pub(in crate::analysis::guarantee) fn current_storage_value(
        &self,
        state: &ExecutionState,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<ExecutionCondition> {
        crate::ExecutionPlace::storage_path(self.storage, path)?.value_in(&state.current)
    }

    /// Read the current source selected by the owning storage transition before
    /// this occurrence defines an evaluated result. The caller resolves borrows.
    pub(in crate::analysis) fn current_access_value(
        &self,
        state: &ExecutionState,
        path: &bray_bound_tree::StoragePath,
    ) -> ExecutionCondition {
        self.storage_path_value(state, path)
    }

    pub(in crate::analysis) fn storage_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        access: bray_bound_tree::StorageAccessId,
    ) -> ExecutionCondition {
        let (mut value, scoped) = match self
            .storage
            .root_identity(access)
            .and_then(|identity| self.storage.identity(identity))
        {
            Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }) => {
                (self.invocation_value(state, bray_bound_tree::BoundExecutionSite::ScopedEnter(expression)), true)
            }
            Some(bray_bound_tree::StorageIdentity::InvocationResult(invocation)) => {
                (self.invocation_value(state, invocation), true)
            }
            _ => (self.value(state, expression), false),
        };

        // A scoped root names the whole retained capability, even when this occurrence
        // observes one of its fields. Ordinary expression values already include their source path.
        let path = if scoped {
            self.storage.resolved_projections(access)
        } else {
            crate::storage::expression_storage_projections(self.storage, expression, access)
        };

        if let Some(path) = path {
            for projection in path {
                value = value.project(*projection);
            }
        } else {
            value = ExecutionCondition::Unknown;
        }

        value
    }
    pub(super) fn complete_memory_write(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        let Some(target) = self
            .checked_body
            .and_then(|body| body.storage_flow().memory_write_target(expression))
            .filter(|target| target.is_exact())
        else {
            return;
        };

        let Some(path) = (target).value_observation_path() else {
            return;
        };

        let Some(place) = crate::ExecutionPlace::storage_path(self.storage, &path) else {
            return;
        };

        let Some(value) = state.entries.get(&expression.into()).and_then(|entry| {
            crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1))
                .value_in(&entry.arguments)
        }) else {
            return;
        };

        state.assign(place, value, None);
    }

    pub(in crate::analysis) fn task_invocations(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<std::collections::BTreeSet<bray_bound_tree::BoundExecutionSite>> {
        let bray_bound_tree::SemanticSelection::Call(call) =
            self.semantics.selections().expression(expression)?
        else {
            return None;
        };

        crate::analysis::AnalysisTaskOperationKind::from_implementation_hook(
            call.implementation_hook()?,
        )?;

        let value = self.receiver_value(state, expression, call);

        state
            .deferred_entries(&value)?
            .known_sources(&state.assumptions)
    }

    pub(super) fn started_future(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<ExecutionCondition> {
        let bray_bound_tree::SemanticSelection::Call(call) =
            self.semantics.selections().expression(expression)?
        else {
            return None;
        };

        if call.implementation_hook() != Some(bray_compiler_known::ImplementationHook::FutureStart)
        {
            return None;
        }

        Some(self.receiver_value(state, expression, call))
    }

    pub(in crate::analysis::guarantee) fn receiver_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
    ) -> ExecutionCondition {
        if call.stages_inputs() {
            self.invocation_value(
                state,
                bray_bound_tree::BoundExecutionSite::CallInput {
                    expression,
                    ordinal: bray_symbols::SymbolOrdinal::new(0),
                },
            )
        } else {
            self.value(
                state,
                call.receiver()
                    .expect("selected receiver operation retains its receiver")
                    .expression(),
            )
        }
    }

    pub(in crate::analysis) fn inactive_future_captures(
        &self,
        state: &ExecutionState,
        ty: bray_symbols::TypeId,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<Vec<bray_symbols::TypeId>> {
        if crate::representation::type_representation(self.request, ty)
            != Some(bray_compiler_known::RepresentationRole::Future)
        {
            return None;
        }

        let value = self.storage_path_value(state, path);
        let entries = state.deferred_entries(&value)?;
        let mut captures = std::collections::BTreeSet::new();

        for source in entries.known_sources(&state.assumptions)? {
            let types = self
                .disposal
                .plan
                .future_capture_cleanup
                .get(&source.expression()?)?;

            captures.extend(types.iter().copied());
        }

        Some(captures.into_iter().collect())
    }

    pub(in crate::analysis) fn deferred_cleanup_invocations(
        &self,
        state: &ExecutionState,
        ty: bray_symbols::TypeId,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<std::collections::BTreeSet<bray_bound_tree::BoundExecutionSite>> {
        if crate::representation::type_representation(self.request, ty)
            .is_none_or(|role| !matches!(role, bray_compiler_known::RepresentationRole::Task | bray_compiler_known::RepresentationRole::Future))
        {
            return None;
        }

        state
            .deferred_entries(&self.storage_path_value(state, path))?
            .known_sources(&state.assumptions)
    }

    /// Whether a known task can run cleanup for its captured values.
    pub(in crate::analysis) fn task_has_capture_cleanup(
        &self,
        sources: &std::collections::BTreeSet<bray_bound_tree::BoundExecutionSite>,
    ) -> Option<bool> {
        let mut cleanup = false;
        for source in sources {
            cleanup |= !self.disposal.plan.future_capture_cleanup.get(&source.expression()?)?.is_empty();
        }
        Some(cleanup)
    }
}
