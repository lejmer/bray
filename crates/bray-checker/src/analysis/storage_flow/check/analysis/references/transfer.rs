use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::storage::storage_projections;
use bray_bound_tree::BoundExecutionSite;
use bray_symbols::DependencySubjectRoot;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn transfer_memory_result_state(
        &self,
        state: &mut StorageFlowState,
        node: bray_bound_tree::AnyBoundNodeId,
    ) {
        if let bray_bound_tree::AnyBoundNodeId::Pattern(pattern) = node {
            for (binding, expression, access) in
                crate::storage::pattern_binding_sources(self.request.view(), self.storage, pattern)
            {
                let Some(bray_bound_tree::StorageBinding::Identity(destination)) =
                    self.storage.binding(binding)
                else {
                    continue;
                };

                if self.input.initialization_destination(expression) == Some(destination) {
                    // The initializer already transferred this whole value, including formed borrows.
                    continue;
                }

                let source = if let Some(result) = self.expression_result_storage(expression) {
                    let selected = self.storage.access(access).expect("pattern source access is planned");
                    let base = self.storage.node_plans(expression.into())
                        .filter_map(|plan| self.storage.access(plan.access()))
                        .filter(|base| base.root() == selected.root()
                            && selected.projections().starts_with(base.projections()))
                        .max_by_key(|base| base.projections().len());

                    base.map(|base| Target::storage(result)
                        .project(&selected.projections()[base.projections().len()..]))
                } else {
                    self.input.reference_target(self.request.semantic_values(), &state.memory, self.storage, access)
                };

                if let Some(source) = source {
                    state
                        .memory
                        .transfer_value(&source, Place::root(destination));
                }
            }

            return;
        }

        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        if let Some(result) = self.expression_result_storage(expression)
            && matches!(self.storage.identity(result.root), Some(bray_bound_tree::StorageIdentity::Evaluated(_)))
        {
            let source = self.storage.node_plans(expression.into()).find_map(|plan| {
                matches!(self.storage.effective_purpose(plan),
                    bray_bound_tree::StorageAccessPurpose::Copy
                        | bray_bound_tree::StorageAccessPurpose::Move
                        | bray_bound_tree::StorageAccessPurpose::ValueTransfer)
                    .then(|| self.input.reference_target(
                        self.request.semantic_values(), &state.memory, self.storage, plan.access()))
                    .flatten()
            });

            if let Some(source) = source {
                // The move's existing evaluated payload survives until finish_value_transfers.
                state.memory.transfer_value(&source, result);
            }
        }

        if let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
            self.execution.semantics.selections().expression(expression)
            && let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
                self.request.view().expression(expression)
            && let Some(result) = self.expression_result_storage(expression)
            && let Some(crate::analysis::storage_flow::memory::Source::Value(source)) = self
                .expression_reference(
                    state,
                    *operation
                        .operands()
                        .first()
                        .expect("checked propagation retains its operand"),
                )
        {
            // Normal continuation transfers the selected payload into a new slot.
            // Reuse the checked projection, including nullable and RunResult cases.
            state
                .memory
                .transfer_value(&source.project(&[selection.success_projection()]), result);
        }

        let captures_inputs = self
            .dependencies
            .calls()
            .get(&expression.into())
            .is_some_and(|call| call.deferred_effects().is_some())
            || matches!(self.execution.semantics.selections().expression(expression),
                Some(bray_bound_tree::SemanticSelection::Call(call))
                    if call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::FutureStart));

        if captures_inputs && let Some(result) = self.expression_result_storage(expression) {
            let captures = self.invocation_roots(state, expression.into());

            if matches!(self.execution.semantics.selections().expression(expression),
            Some(bray_bound_tree::SemanticSelection::Call(call))
                if matches!(call.implementation_hook(), Some(
                    bray_compiler_known::ImplementationHook::TaskJoin
                        | bray_compiler_known::ImplementationHook::TaskCancel
                )))
            {
                state
                    .memory
                    .publish_deferred_inputs(expression.into(), result.clone());
            }

            state.memory.capture_inputs(result.clone(), captures);

            if self
                .dependencies
                .calls()
                .get(&expression.into())
                .is_some_and(|call| call.deferred_effects().is_some())
            {
                state
                    .memory
                    .capture_deferred_transition(expression.into(), result);
            }
        }

        if let Some(fields) = self.input.reference_constructions.get(&expression) {
            for (operand, destination, ty) in fields {
                // Each transition owns its destination while sharing the prepared projection path.
                self.transfer_reference_value(state, *operand, destination.clone(), *ty);
            }
        }

        if self.memory.operation(expression).is_none()
            && let Some(result) = self.expression_result_storage(expression)
            && let Some(source) = self.execution.reference_value_source(expression)
            && let Some(source) = self.expression_reference(state, source)
        {
            source.transfer(&mut state.memory, result);
        }

        if let Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) =
            self.request.view().expression(expression)
            && let Some(target) = self.execution.yield_targets.get(&expression)
            && let Some(operand) = transfer.operand()
            && let Some(result) = self.expression_result_storage(*target)
        {
            let ty = self
                .execution
                .semantics
                .types()
                .expression(*target)
                .expect("yield target has a checked type")
                .ty();

            let absent = self.observe_transferred_payloads(state, operand, &result, ty);

            if !absent
                && let Some(projections) = self.reference_wrapping(operand, ty)
                && let Some(source) = self.expression_reference(state, operand)
            {
                // The result is evaluated before scope cleanup and published only at its join.
                source.stage(
                    &mut state.memory,
                    (*target).into(),
                    Target::storage(result.clone()).project(&projections),
                );
            }

            // An inactive payload is an evaluated null reference value. Stage it
            // with the result so source lifetime retirement cannot erase it.
            for (path, reason) in &state.definitely_absent {
                if *reason == crate::analysis::storage_flow::model::StorageAbsence::NoPayload
                    && result.contains(path)
                {
                    self.stage_absent_references(&mut state.memory, (*target).into(), path);
                }
            }
        }

        if let Some(bray_bound_tree::BoundExpression::ControlTransfer(transfer)) =
            self.request.view().expression(expression)
            && transfer.kind() == bray_bound_tree::BoundControlTransferKind::Return
        {
            let Some(operand) = transfer.operand() else {
                return;
            };

            let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
                .storage
                .binding(bray_bound_tree::StorageBindingTarget::Result)
            else {
                panic!("planned callable has result storage");
            };

            let ty = self
                .execution
                .semantics
                .types()
                .callable_result_type()
                .expect("checked return has a callable result type");

            self.transfer_reference_value(state, operand, Place::root(result), ty);
        } else if matches!(self.request.root(), crate::CheckerUnitRoot::Expression(root) if root == expression)
        {
            let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
                .storage
                .binding(bray_bound_tree::StorageBindingTarget::Result)
            else {
                panic!("planned expression body has result storage");
            };

            let ty = self
                .execution
                .semantics
                .types()
                .expression(expression)
                .expect("expression body has a checked result type")
                .ty();

            let result = Place::root(result);

            state.initialize_path(&result);
            self.transfer_reference_value(state, expression, result, ty);
        } else if let Some(destination) = self.input.initialization_destination(expression) {
            let ty = self
                .storage
                .identity_type(destination)
                .expect("initialized local has a declared type");

            self.transfer_reference_value(state, expression, Place::root(destination), ty);
        }
    }

    pub(in crate::analysis::storage_flow) fn propagate_reference_result(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        use bray_bound_tree::{
            SelectedPropagation, SelectedPropagationBoundary, StorageProjection,
        };

        let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
            self.execution.semantics.selections().expression(expression)
        else {
            unreachable!("propagation edge retains its checked selection");
        };

        let (destination, pending) = match selection.boundary() {
            Some(SelectedPropagationBoundary::Callable) => {
                let Some(bray_bound_tree::StorageBinding::Identity(result)) = self
                    .storage
                    .binding(bray_bound_tree::StorageBindingTarget::Result)
                else {
                    panic!("propagation retains its callable result storage");
                };

                (Place::root(result), None)
            }
            Some(SelectedPropagationBoundary::YieldRegion(_)) => {
                let target = self
                    .execution
                    .yield_targets
                    .get(&expression)
                    .expect("propagation retains its selected yield target");

                (
                    self.expression_result_storage(*target)
                        .expect("yield propagation retains its result storage"),
                    Some(*target),
                )
            }
            None => return,
        };

        if pending.is_none() {
            state.initialize_path(&destination);
            state.memory.clear(&destination);
        }

        let absent = Target::storage(destination.clone())
            .project(&[selection.success_projection()])
            .storage_path()
            .expect("inline result payload retains its storage path")
            .clone();

        if let Some(pending) = pending {
            self.stage_absent_references(&mut state.memory, pending.into(), &absent);
        } else {
            state.definitely_absent.insert(
                absent,
                crate::analysis::storage_flow::model::StorageAbsence::NoPayload,
            );
        }

        let SelectedPropagation::Result {
            error: (variant, field),
            error_conversion,
            ..
        } = selection
        else {
            return;
        };

        let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
            self.request.view().expression(expression)
        else {
            unreachable!("checked propagation retains its structured expression");
        };

        let operand = *operation
            .operands()
            .first()
            .expect("propagation retains its operand");

        let projection = StorageProjection::ActiveUnionPayloadField {
            variant: *variant,
            field: *field,
        };

        let mut projections = vec![projection];

        match error_conversion.target() {
            bray_bound_tree::ConversionTarget::Identity
            | bray_bound_tree::ConversionTarget::CallableContract => {}
            bray_bound_tree::ConversionTarget::NullablePresent => {
                projections.push(StorageProjection::NullableValue)
            }
            // A value-changing conversion does not establish the source's reference identity.
            _ => return,
        }

        if let Some(crate::analysis::storage_flow::memory::Source::Value(source)) =
            self.expression_reference(state, operand)
        {
            let destination = Place {
                root: destination.root,
                projections: destination
                    .projections
                    .iter()
                    .copied()
                    .chain(projections)
                    .collect(),
            };

            let source = source.project(&[projection]);

            if let Some(pending) = pending {
                state
                    .memory
                    .stage_value(pending.into(), &source, Target::storage(destination));
            } else {
                state.memory.transfer_value(&source, destination);
            }
        }
    }

    fn stage_absent_references(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        absent: &Place,
    ) {
        // Pending joins compare exact slots. An absent aggregate must carry emptiness
        // for every prepared reference it contains, without making result storage live.
        for path in self.input.reference_slots(absent) {
            memory.stage_null(occurrence, Target::storage(path));
        }
    }

    pub(super) fn reference_wrapping(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
        target: bray_symbols::TypeId,
    ) -> Option<Vec<bray_bound_tree::StorageProjection>> {
        let source_type = self
            .execution
            .semantics
            .types()
            .expression(expression)
            .expect("transferred value has a checked type")
            .ty();

        crate::storage::nullable_wrapping(self.request.semantic_values(), source_type, target)
    }

    fn observe_transferred_payloads(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        destination: &Place,
        target: bray_symbols::TypeId,
    ) -> bool {
        if let Some(execution) = state.execution.as_ref() {
            let value = self.execution.value(execution, expression);

            for payload in self
                .input
                .variant_payloads
                .get(destination)
                .into_iter()
                .flatten()
            {
                if let Some(bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                    variant,
                    ..
                }) = payload.projections.last()
                    && value.clone().matches_variant(*variant)
                        == crate::ExecutionCondition::Boolean(false)
                {
                    self.mark_absent_references(state, payload);
                }
            }
        }

        if state
            .execution
            .as_ref()
            .is_some_and(|execution| self.nullable_is_absent(execution, expression, target))
        {
            self.mark_absent_references(
                state,
                &Place {
                    root: destination.root,
                    projections: destination
                        .projections
                        .iter()
                        .copied()
                        .chain([bray_bound_tree::StorageProjection::NullableValue])
                        .collect(),
                },
            );

            return true;
        }

        false
    }

    fn mark_absent_references(&self, state: &mut StorageFlowState, absent: &Place) {
        state.definitely_absent.insert(
            absent.clone(),
            crate::analysis::storage_flow::model::StorageAbsence::NoPayload,
        );

        // Empty reference values move and stage with the containing inactive payload.
        for path in self.input.reference_slots(absent) {
            state.memory.null(path);
        }
    }

    fn transfer_reference_value(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        destination: Place,
        target: bray_symbols::TypeId,
    ) {
        // The transfer owns its converted destination path.
        if let Some(projections) = self.reference_wrapping(expression, target)
            && let Some(source) = self.expression_reference(state, expression)
        {
            source.transfer(
                &mut state.memory,
                Place {
                    root: destination.root,
                    projections: destination
                        .projections
                        .iter()
                        .copied()
                        .chain(projections)
                        .collect(),
                },
            );
        } else {
            state.memory.clear(&destination);
        }

        self.observe_transferred_payloads(state, expression, &destination, target);
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_assignment_reference(
        &self,
        state: &mut StorageFlowState,
        operation: crate::analysis::model::AnalysisOperationKind,
        destination: Option<&Target>,
    ) {
        let crate::analysis::model::AnalysisOperationKind::Bound(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = operation
        else {
            return;
        };

        let Some(bray_bound_tree::BoundExpression::Assignment(assignment)) =
            self.request.view().expression(expression)
        else {
            return;
        };

        state.memory.begin_transition(operation.occurrence());

        let Some(destination) = destination else {
            // The assignment transition applies its write after input capture.
            // An unresolved destination provides no reference output to stage.
            return;
        };

        let [target, source] = assignment.operands() else {
            return;
        };

        if assignment.operator() == bray_bound_tree::BoundAssignmentOperator::Assign {
            let ty = self
                .execution
                .semantics
                .types()
                .expression(*target)
                .expect("assignment target has a checked type")
                .ty();

            // Pending output owns this path until the assignment completes.
            if let Some(projections) = self.reference_wrapping(*source, ty)
                && let Some(source) = self.expression_reference(state, *source)
            {
                source.stage(
                    &mut state.memory,
                    operation.occurrence(),
                    destination.project(&projections),
                );

                return;
            }
        }

        state
            .memory
            .stage_unknown(operation.occurrence(), destination.clone());
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_call_references(
        &self,
        state: &mut StorageFlowState,
        occurrence: BoundExecutionSite,
    ) {
        if matches!(occurrence, BoundExecutionSite::Node(_))
            && occurrence
                .expression()
                .is_some_and(|expression| self.memory.operation(expression).is_some())
        {
            return;
        }

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return;
        };

        state.memory.begin_transition(occurrence);

        if let BoundExecutionSite::CallInput {
            expression,
            ordinal,
        } = occurrence
            && let Some(selection) = self.execution.semantics.selections().expression(expression)
        {
            let input = selection
                .input(ordinal)
                .expect("input reference transfer retains its selection");

            let projections = match input.conversion() {
                None if input.expression().is_some() => Some(&[][..]),
                Some(conversion) => match conversion.target() {
                    bray_bound_tree::ConversionTarget::Identity
                    | bray_bound_tree::ConversionTarget::CallableContract => Some(&[][..]),
                    bray_bound_tree::ConversionTarget::NullablePresent => {
                        Some(&[bray_bound_tree::StorageProjection::NullableValue][..])
                    }
                    _ => None,
                },
                None => None,
            };

            if let Some(projections) = projections
                && let Some(source) =
                    self.invocation_reference(state, occurrence, DependencySubjectRoot::Receiver)
                && let Some(destination) = self.invocation_result(occurrence)
            {
                source.stage(
                    &mut state.memory,
                    occurrence,
                    Target::storage(destination).project(projections),
                );
            }
        }

        // Seed independent alternatives before merging reference transfers from inputs.
        for reference in contracts
            .references()
            .iter()
            .filter(|reference| reference.relation().entry().is_none())
            .chain(
                contracts
                    .references()
                    .iter()
                    .filter(|reference| reference.relation().entry().is_some()),
            )
        {
            let relation = reference.relation();

            let output = if relation.exit().subject_root() == DependencySubjectRoot::Result {
                self.invocation_result(occurrence)
                    .map(Target::storage)
                    .and_then(|target| {
                        Some(target.project(&storage_projections(
                            self.request.semantic_values(),
                            self.dependencies,
                            relation.exit().projections(),
                        )?))
                    })
            } else if contracts
                .reference_path(relation.exit())
                .is_some_and(|path| !path.borrows().is_empty())
            {
                self.invocation_locations(state, occurrence, relation.exit())
                    .and_then(|mut targets| {
                        (targets.len() == 1).then(|| targets.pop().expect("one output target"))
                    })
            } else {
                None
            };

            let Some(destination) = output else {
                continue;
            };

            let logical = self
                .invocation_logical_path(occurrence, relation.exit())
                .expect("prepared reference output retains its adapted logical storage");

            let Some(entry) = relation.entry() else {
                state.memory.stage_returned_reference(
                    occurrence,
                    destination,
                    logical,
                    reference
                        .addressed_type()
                        .expect("independent reference retains its target type"),
                    [],
                );

                continue;
            };

            let Some(sources) = self.invocation_sources(state, occurrence, entry) else {
                state.memory.stage_unknown(occurrence, destination);
                continue;
            };

            if sources.is_empty() {
                state.memory.stage_unknown(occurrence, destination.clone());
            }

            for source in sources {
                // Each alternative owns its output path while sharing immutable projections.
                self.stage_reference(
                    &mut state.memory,
                    occurrence,
                    source,
                    destination.clone(),
                    &logical,
                    reference,
                );
            }
        }

        self.prepare_result_references(state, occurrence);

        if let Some(destination) = self.invocation_result(occurrence) {
            self.prepare_returned_frames(state, occurrence, contracts, &destination);
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn stage_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        source: crate::analysis::storage_flow::memory::Source,
        destination: Target,
        logical: &Place,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        let relation = reference.relation();

        if relation.provenance() == bray_symbols::DependencyReferenceProvenance::InputRegions {
            let Some(mut regions) = source.regions(memory) else {
                memory.stage_unknown(occurrence, destination);
                return;
            };

            for _ in 0..relation.dereferences() {
                let Some(next) = regions
                    .iter()
                    .map(|target| memory.referenced_regions(target))
                    .collect::<Option<Vec<_>>>()
                else {
                    memory.stage_unknown(occurrence, destination);
                    return;
                };

                regions = next.into_iter().flatten().collect();
            }

            let projections = storage_projections(
                self.request.semantic_values(),
                self.dependencies,
                relation.target_projections(),
            )
            .expect("prepared reference regions retain their projections");

            memory.stage_returned_reference(
                occurrence,
                destination,
                logical.clone(),
                reference
                    .addressed_type()
                    .expect("region bound retains its output target type"),
                regions
                    .into_iter()
                    .map(|target| target.project(&projections)),
            );

            return;
        }

        let count = relation.dereferences();

        if count == 0 {
            self.stage_reference_value(memory, occurrence, source, destination, reference);
            return;
        }

        let Some(targets) = source.targets(memory) else {
            memory.stage_unknown(occurrence, destination);
            return;
        };

        let mut targets = targets
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();

        for _ in 1..count {
            let mut next = std::collections::BTreeSet::new();

            for target in targets {
                let Some(values) = memory.value_targets(&target) else {
                    memory.stage_unknown(occurrence, destination);
                    return;
                };

                next.extend(values);
            }

            targets = next;
        }

        for target in targets {
            // Each possible origin contributes to the same independently owned output.
            self.stage_reference_value(
                memory,
                occurrence,
                crate::analysis::storage_flow::memory::Source::Value(target),
                destination.clone(),
                reference,
            );
        }
    }

    fn stage_reference_value(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        source: crate::analysis::storage_flow::memory::Source,
        destination: Target,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        let relation = reference.relation();

        if !relation.target_projections().is_empty() {
            let Some(targets) = source.targets(memory) else {
                memory.stage_unknown(occurrence, destination);
                return;
            };

            let Some(projections) = storage_projections(
                self.request.semantic_values(),
                self.dependencies,
                relation.target_projections(),
            ) else {
                memory.stage_unknown(occurrence, destination);
                return;
            };

            for target in targets {
                memory.stage_address(
                    occurrence,
                    target.project(&projections),
                    destination.clone(),
                    reference,
                );
            }
        } else if reference.addressed_type().is_some() {
            let crate::analysis::storage_flow::memory::Source::Value(target) = source else {
                panic!("addressed storage has a location");
            };

            memory.stage_address(occurrence, target, destination.clone(), reference);
        } else {
            source.stage(memory, occurrence, destination.clone());

            if !relation.is_exact() {
                memory.bound_staged_reference(occurrence, &destination);
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn call_reference_locations(
        &self,
        state: &StorageFlowState,
        argument: bray_bound_tree::BoundExpressionId,
        path: &bray_bound_tree::StorageReferencePath,
    ) -> Option<Vec<Target>> {
        let start = self.expression_target(state, argument)?;

        let root_borrow = if path.borrows().first().is_some_and(|(index, _)| *index == 0) {
            let borrowed = self.expression_is_borrowed(argument);

            Some(if borrowed {
                self.expression_reference(state, argument)?
                    .targets(&state.memory)?
            } else {
                // An implicit argument borrow starts at the already resolved value location.
                vec![start.clone()]
            })
        } else {
            None
        };

        match root_borrow {
            Some(targets) => targets
                .into_iter()
                .map(|target| {
                    // Candidate roots share the prepared starting path's immutable projections.
                    state
                        .memory
                        .reference_location(self.request.semantic_values(), start.clone(), path, Some(target))
                })
                .collect(),
            None => Some(vec![state.memory.reference_location(self.request.semantic_values(), start, path, None)?]),
        }
    }
    pub(in crate::analysis::storage_flow::check::analysis) fn prepare_result_references(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {
        if let Some(destination) = self.invocation_result(occurrence) {
            let root = self
                .storage
                .storage_type(destination.root)
                .and_then(|ty| crate::representation::reference_target_type(self.request, ty))
                .map(|target| (destination.clone(), target));

            let contained = self.input.value_references.get(&destination.root);

            if root.is_some() || contained.is_some_and(|paths| !paths.is_empty()) {
                let operation = occurrence
                    .expression()
                    .and_then(|expression| self.memory.operation(expression));

                let inputs = match operation {
                    Some(operation)
                        if matches!(
                            operation.kind(),
                            bray_bound_tree::CheckedMemoryOperationKind::BorrowFrom { .. }
                        ) =>
                    {
                        // The first argument retains the loan's lifetime. Only the
                        // pointer argument identifies storage that the loan can reach.
                        self.memory_argument_reference(state, operation, operation.arguments()[1])
                            .and_then(|source| source.regions(&state.memory))
                            .unwrap_or_default()
                    }
                    _ => self.invocation_roots(state, occurrence),
                };

                // Transitions own small place keys with shared projection paths.
                let references = contained
                    .into_iter()
                    .flat_map(|paths| paths.iter())
                    .filter(|(path, _)| path.projections.is_empty() || !state.definitely_absent.iter().any(|(absent, reason)| {
                        *reason == crate::analysis::storage_flow::model::StorageAbsence::NoPayload && absent.contains(path)
                    }))
                    .map(|(path, ty)| (path.clone(), *ty))
                    .chain(root.filter(|(path, _)| {
                        !contained.is_some_and(|paths| paths.contains_key(path))
                    }));

                state
                    .memory
                    .stage_returned_references(occurrence, references, inputs);
            }
        }
    }
}
