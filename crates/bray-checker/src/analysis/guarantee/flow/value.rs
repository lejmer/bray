use super::model::ExecutionFlowDomain;

use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId};

use crate::CheckerRequestContext;
use crate::execution_guarantees::{ExecutionCondition, expression_condition};

use super::super::state::ExecutionState;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn transfer_scoped_result(
        &self,
        state: &mut ExecutionState,
        transfer: crate::analysis::model::ScopedResultTransfer,
    ) {
        use crate::analysis::model::ScopedResultDestination;

        let source = transfer.source.map(|identity| crate::ExecutionPlace::storage_identity(self.storage, identity)
            .expect("scoped protocol produced its raw result slot"));

        let payload = source.as_ref().map_or_else(|| {
            let ty = crate::representation::representation_type(self.request, bray_compiler_known::RepresentationRole::Unit);

            ExecutionCondition::new(crate::ExecutionConditionKind::Literal(std::sync::Arc::new(
                bray_symbols::ConstantValueData::new(ty, bray_symbols::ConstantValueKind::Unit),
            )))
        }, |source| source.value_in(&state.current).expect("scoped result selection follows completion").project(transfer.projection));

        match transfer.destination {
            ScopedResultDestination::Capability(identity) => {
                let destination = crate::ExecutionPlace::storage_identity(self.storage, identity)
                    .expect("scoped entry selection retains its capability place");

                state.assign(destination, payload, None);
            }
            ScopedResultDestination::Region { slot, displaced } => {
                if let Some(displaced) = displaced {
                    let previous = crate::ExecutionPlace::storage_identity(self.storage, slot)
                        .and_then(|place| place.value_in(&state.current))
                        .expect("replacement retains its previously produced result");

                    let temporary = crate::ExecutionPlace::storage_identity(self.storage, displaced)
                        .expect("displaced result retains its cleanup storage");

                    state.assign(temporary, previous, None);
                }

                let region = transfer.invocation.expression().expect("scoped result retains its region expression");
                let value = Self::tagged_region_payload(region, transfer.projection, payload);

                self.produce_region_result(state, region, value);
            }
            ScopedResultDestination::Incident { .. } | ScopedResultDestination::Discard => {}
        }

        if let Some(source) = source {
            state.vacate(source);
        }
    }

    pub(super) fn tagged_region_payload(
        region: BoundExpressionId,
        projection: bray_bound_tree::StorageProjection,
        payload: ExecutionCondition,
    ) -> ExecutionCondition {
        let bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, .. } = projection else {
            unreachable!("scoped region selects a checked Result payload");
        };

        ExecutionCondition::constructed(
            ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
                variant,
                std::sync::Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Expression(region))),
            )),
            vec![(projection, payload)].into(),
        )
    }

    pub(in crate::analysis) fn value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        state.expressions.get(&expression)
            .or_else(|| state.pending_results.get(&expression))
            .cloned()
            .unwrap_or_else(ExecutionCondition::unknown)
    }

    /// Reads the live operand from its planned slot. An earlier transfer can
    /// leave only the retained evaluation snapshot for a later proof query.
    pub(in crate::analysis) fn operand_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        self.storage.value_slot(expression)
            .and_then(|identity| crate::ExecutionPlace::storage_identity(self.storage, identity))
            .and_then(|place| state.current.live_value(&place))
            .unwrap_or_else(|| self.value(state, expression))
    }

    pub(in crate::analysis) fn returned_value(&self, state: &ExecutionState) -> ExecutionCondition {
        self.storage.binding(bray_bound_tree::StorageBindingTarget::Result)
            .and_then(|binding| match binding {
                bray_bound_tree::StorageBinding::Identity(identity) =>
                    crate::ExecutionPlace::storage_identity(self.storage, identity),
                _ => None,
            })
            .and_then(|place| place.value_in(&state.current))
            .unwrap_or_else(ExecutionCondition::unknown)
    }

    /// Binds one declared completion condition to the reached entry snapshot and result.
    pub(in crate::analysis::guarantee) fn completion_condition(
        &self,
        state: &ExecutionState,
        condition: &ExecutionCondition,
        result: &ExecutionCondition,
    ) -> ExecutionCondition {
        condition
            .substitute(
                &|input| {
                    input
                        .value_in(&state.current)
                        .unwrap_or_else(ExecutionCondition::unknown)
                },
                result,
                &mut { ExecutionCondition::WORK_LIMIT },
            )
            .with_reference_origins(
                self.request.semantic_values(),
                self.dependencies,
                &state.assumptions,
            )
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

    pub(super) fn produce_region_result(
        &self, state: &mut ExecutionState, expression: BoundExpressionId, value: ExecutionCondition,
    ) {
        let Some(slot) = self.storage.value_slot(expression) else {
            // Recovered and uninhabited result regions have no value slot.
            return;
        };

        let place = crate::ExecutionPlace::storage_identity(self.storage, slot)
            .expect("result storage has a current value slot");

        self.assume_produced_variant(state, expression, &value);
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
            state.assumptions.insert((ExecutionCondition::new(crate::ExecutionConditionKind::VariantTest(variant, owner.clone())), variant == active));
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

            return if contents == ExecutionCondition::unknown() {
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
    pub(super) fn private_operator_result(&self, expression: BoundExpressionId) -> Option<crate::ExecutionPlace> {
        let access = self.storage.invocation_result(expression.into())?;
        let path = self.storage.logical_path(access)?;

        matches!(self.storage.identity(path.root), Some(bray_bound_tree::StorageIdentity::InvocationResult(_)))
            .then(|| crate::ExecutionPlace::storage_path(self.storage, path)).flatten()
    }

    pub(in crate::analysis::guarantee) fn invocation_result_place(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionPlace> {
        use bray_bound_tree::BoundExecutionSite as Site;

        match invocation {
            Site::Node(AnyBoundNodeId::Expression(expression)) => self.private_operator_result(expression)
                .or_else(|| self.storage.value_slot(expression)
                    .and_then(|identity| crate::ExecutionPlace::storage_identity(self.storage, identity))),
            Site::CallInput { .. } => self.input_place(invocation),
            Site::Iterate(_) | Site::Next(_) | Site::ScopedEnter(_) | Site::ScopedExit(_) => self.storage.invocation_result(invocation)
                .and_then(|access| self.storage.logical_path(access))
                .and_then(|path| crate::ExecutionPlace::storage_path(self.storage, path)),
            _ => None,
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
            ) => self.private_operator_result(expression).and_then(|place| state.current.live_value(&place))
                .unwrap_or_else(|| self.value(state, expression)),
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
                    .unwrap_or_else(ExecutionCondition::unknown)
            }
            bray_bound_tree::BoundExecutionSite::Iterate(_)
            | bray_bound_tree::BoundExecutionSite::Next(_)
            | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
            | bray_bound_tree::BoundExecutionSite::ScopedExit(_) => {
                let Some(place) = self.invocation_result_place(invocation) else {
                    return ExecutionCondition::unknown();
                };

                if state.current.availability(&place) == super::super::current::ValueAvailability::Absent {
                    return ExecutionCondition::unknown();
                }

                place
                    .value_in(&state.current)
                    .unwrap_or_else(ExecutionCondition::unknown)
            }
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
        match value.kind() {
            crate::ExecutionConditionKind::CapturedValue { value, .. } | crate::ExecutionConditionKind::DeferredValue { value, .. } => self.observation_type(value),
            crate::ExecutionConditionKind::Result => self.storage.identity_entries().find_map(|(identity, storage)| {
                matches!(storage, bray_bound_tree::StorageIdentity::PostconditionResult(_))
                    .then(|| self.storage.storage_type(identity)).flatten()
            }),
            crate::ExecutionConditionKind::ScalarValue(identity) | crate::ExecutionConditionKind::PostState(_, identity) => self.observation_type(identity),
            crate::ExecutionConditionKind::Input(place)
            | crate::ExecutionConditionKind::Joined(_, place) => self
                .observed_types
                .get(place)
                .copied()
                .flatten()
                .map(|ty| self.request.semantic_values().unborrowed_type(ty))
                .or_else(|| match place.root {
                    crate::execution_guarantees::ExecutionInput::Storage(identity)
                        if place.projections.is_empty() =>
                    {
                        self.storage.storage_type(identity)
                    }
                    crate::execution_guarantees::ExecutionInput::FrameInput { ref source, .. }
                        if place.projections.is_empty() => self.observation_type(source),
                    crate::execution_guarantees::ExecutionInput::Pointee { ty, .. }
                        if place.projections.is_empty() => Some(ty),
                    _ => None,
                }),
            crate::ExecutionConditionKind::Projection(_, _) => value.selector_place()
                .and_then(|place| self.observed_types.get(&place).copied().flatten())
                .map(|ty| self.request.semantic_values().unborrowed_type(ty)),
            crate::ExecutionConditionKind::Call(instance, _, _, _, _)
                if self.sequence_length.is_some_and(|length| length.definition() == instance.definition()) =>
            {
                Some(crate::representation::representation_type(
                    self.request,
                    bray_compiler_known::RepresentationRole::ScalarUsize,
                ))
            }
            crate::ExecutionConditionKind::Expression(expression) => self
                .semantics
                .types()
                .expression(*expression)
                .map(|entry| entry.ty()),
            crate::ExecutionConditionKind::Literal(value) => Some(value.ty()),
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
                .unwrap_or_else(ExecutionCondition::unknown);
        }

        match self
            .storage
            .identity(identity)
            .expect("checked storage retains its identity")
        {
            bray_bound_tree::StorageIdentity::Temporary(expression)
            | bray_bound_tree::StorageIdentity::Allocation(expression)
            | bray_bound_tree::StorageIdentity::Result(AnyBoundNodeId::Expression(expression)) => {
                self.value(state, expression)
            }
            _ => ExecutionCondition::unknown(),
        }
    }

    pub(in crate::analysis) fn storage_path_value(
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
        let protocol_root = self.storage.root_identity(access).filter(|identity| {
            matches!(self.storage.identity(*identity), Some(bray_bound_tree::StorageIdentity::ScopedCapability { .. }
                | bray_bound_tree::StorageIdentity::InvocationResult(_)))
        });

        let mut value = match protocol_root {
            Some(root) => self.storage_path_value(state, &bray_bound_tree::StoragePath::root(root)),
            None => self.operand_value(state, expression),
        };

        // A scoped root names the whole retained capability, even when this occurrence
        // observes one of its fields. Ordinary expression values already include their source path.
        let path = if protocol_root.is_some() {
            self.storage.resolved_projections(access)
        } else {
            crate::storage::expression_storage_projections(self.storage, expression, access)
        };

        if let Some(path) = path {
            for projection in path {
                value = value.project(*projection);
            }
        } else {
            value = ExecutionCondition::unknown();
        }

        value
    }
    pub(super) fn complete_memory_write(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        target: &bray_bound_tree::StorageTarget,
    ) {
        if !target.is_exact() {
            return;
        }

        let Some(ty) = self.memory.operation(expression).and_then(|operation| match operation.kind() {
            bray_bound_tree::CheckedMemoryOperationKind::Write { pointee } => Some(*pointee),
            _ => None,
        }) else {
            return;
        };

        let place = if let Some(path) = target.value_observation_path() {
            let Some(place) = crate::ExecutionPlace::storage_path(self.storage, &path) else {
                return;
            };

            place
        } else {
            let place = crate::ExecutionPlace::pointee(self.storage, target, ty);

            if state.current.get(&place).is_none() {
                return;
            }

            place
        };

        let Some(value) = state.entries.get(&expression.into()).and_then(|entry| {
            crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1))
                .value_in(&entry.arguments)
        }) else {
            return;
        };

        state.assign(place, value, None);
    }

    pub(in crate::analysis) fn started_future(
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

        let entry = state.entries.get(&expression.into())?;

        crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&entry.arguments)
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

        let entries = self.deferred_cleanup_frame(state, ty, path)?;

        self.inactive_frame_capture_types(state, &entries)
    }

    pub(in crate::analysis) fn inactive_frame_capture_types(
        &self,
        state: &ExecutionState,
        entries: &crate::ExecutionDeferredValue,
    ) -> Option<Vec<bray_symbols::TypeId>> {
        let proof = state.ordinary_proof();
        let mut captures = std::collections::BTreeSet::new();

        for (source, candidate) in entries.producers.iter().filter(|(_, candidate)| {
            crate::execution_guarantees::deferred_possible(&candidate.selection, &proof)
        }) {
            let types = if let Some(source) = source.local() {
                self.disposal.future_capture_cleanup.get(&source.expression()?)?.as_slice()
            } else {
                let body = self.dependencies.frame_callees().get(candidate.callee.as_deref()?)?;

                if body.capture_cleanup.is_empty() { &[][..] } else { &body.capture_types }
            };

            captures.extend(types.iter().copied());
        }

        Some(captures.into_iter().collect())
    }

    pub(in crate::analysis) fn deferred_cleanup_frame(
        &self,
        state: &ExecutionState,
        ty: bray_symbols::TypeId,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<std::sync::Arc<crate::ExecutionDeferredValue>> {
        if crate::representation::type_representation(self.request, ty)
            .is_none_or(|role| !matches!(role, bray_compiler_known::RepresentationRole::Task | bray_compiler_known::RepresentationRole::Future))
        {
            return None;
        }

        self.storage_path_value(state, path).deferred_value()
            .filter(|frame| !frame.unknown_possible(&state.ordinary_proof())).cloned()
    }

}
