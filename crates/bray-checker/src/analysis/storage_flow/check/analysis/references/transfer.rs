use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::storage::storage_projections;
use bray_bound_tree::BoundExecutionSite;
use bray_symbols::{DependencySubjectRoot, TypeData};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn transfer_memory_result_state(
        &self,
        state: &mut StorageFlowState,
        node: bray_bound_tree::AnyBoundNodeId,
    ) {
        if let bray_bound_tree::AnyBoundNodeId::Pattern(pattern) = node {
            for (binding, expression, access) in crate::storage::pattern_binding_sources(
                self.request.view(), self.storage, pattern,
            ) {
                let Some(bray_bound_tree::StorageBinding::Identity(destination)) = self.storage
                    .binding(bray_bound_tree::StorageBindingTarget::Local(binding))
                else { continue; };

                if self.input.initialization_destination(expression) == Some(destination) {
                    // The initializer already transferred this whole value, including formed borrows.
                    continue;
                }

                if let Some(source) = self.input.reference_target(&state.memory, self.storage, access) {
                    state.memory.transfer_value(&source, Place::root(destination));
                }
            }

            return;
        }

        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        let captures_inputs = self.dependencies.calls().get(&expression.into()).is_some_and(|call| call.deferred_effects().is_some())
            || matches!(self.execution.semantics.selections().expression(expression),
                Some(bray_bound_tree::SemanticSelection::Call(call))
                    if call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::FutureStart));

        if captures_inputs && let Some(result) = self.expression_result_storage(expression)
        {
            let captures = self.invocation_roots(state, expression.into());

            state.memory.capture_inputs(result, captures);
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
            && let Some(projections) = self.reference_wrapping(
                operand,
                self.execution
                    .semantics
                    .types()
                    .expression(*target)
                    .expect("yield target has a checked type")
                    .ty(),
            )
            && let Some(source) = self.expression_reference(state, operand)
        {
            // The result is evaluated before scope cleanup and published only at its join.
            source.stage(
                &mut state.memory,
                (*target).into(),
                Target::storage(result).project(&projections),
            );
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

    fn reference_wrapping(
        &self,
        expression: bray_bound_tree::BoundExpressionId,
        mut target: bray_symbols::TypeId,
    ) -> Option<Vec<bray_bound_tree::StorageProjection>> {
        let source_type = self
            .execution
            .semantics
            .types()
            .expression(expression)
            .expect("transferred value has a checked type")
            .ty();

        let mut projections = Vec::new();

        while target != source_type {
            let data = self.request.semantic_values().type_data(target);

            let TypeData::Nullable(inner) = data.as_ref() else {
                return None;
            };

            projections.push(bray_bound_tree::StorageProjection::NullableValue);
            target = *inner;
        }

        Some(projections)
    }

    fn transfer_reference_value(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        destination: Place,
        target: bray_symbols::TypeId,
    ) {
        if matches!(
            self.request.semantic_values().type_data(target).as_ref(),
            TypeData::Nullable(_)
        ) && let Some(execution) = &state.execution
            && matches!(self.execution.value(execution, expression), crate::ExecutionCondition::Literal(value)
                if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent))
        {
            state.memory.clear(&destination);

            state.definitely_absent.insert(
                Place {
                    root: destination.root,
                    projections: destination
                        .projections
                        .iter()
                        .copied()
                        .chain([bray_bound_tree::StorageProjection::NullableValue])
                        .collect(),
                },
                crate::analysis::storage_flow::model::StorageAbsence::NoPayload,
            );

            return;
        }

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
            state.memory.invalidate(
                &bray_bound_tree::StorageInvalidation::unknown(),
                self.storage,
                bray_symbols::DependencyMutationKind::Write,
            );

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
        if occurrence
            .expression()
            .is_some_and(|expression| self.memory.operation(expression).is_some())
        {
            return;
        }

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return;
        };

        state.memory.begin_transition(occurrence);

        for reference in contracts.references() {
            let relation = reference.relation();

            let output = if relation.exit().subject_root() == DependencySubjectRoot::Result {
                self.invocation_result(occurrence)
                    .map(Target::storage)
                    .and_then(|target| {
                        Some(target.project(&storage_projections(
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

            let sources = if relation.entry().projections().is_empty()
                && (reference.addressed_type().is_none()
                    || !relation.target_projections().is_empty())
            {
                self.invocation_reference(state, occurrence, relation.entry().subject_root())
                    .into_iter()
                    .collect::<Vec<_>>()
            } else {
                let Some(targets) = self.invocation_locations(state, occurrence, relation.entry())
                else {
                    continue;
                };

                targets
                    .into_iter()
                    .map(crate::analysis::storage_flow::memory::Source::Value)
                    .collect()
            };

            for source in sources {
                // Each alternative owns its output path while sharing immutable projections.
                self.stage_reference(
                    &mut state.memory,
                    occurrence,
                    source,
                    destination.clone(),
                    reference,
                );
            }
        }

        if let Some(destination) = self.invocation_result(occurrence) {
            let root = self.storage.storage_type(destination.root)
                .and_then(|ty| crate::representation::reference_target_type(self.request, ty))
                .map(|target| (destination.clone(), target));

            let contained = self.input.returned_references.get(&destination.root);

            if root.is_some() || contained.is_some_and(|paths| !paths.is_empty()) {
                let inputs = self.invocation_roots(state, occurrence);

                // Transitions own small place keys with shared projection paths.
                let references = contained.into_iter().flat_map(|paths| paths.iter().map(|(path, ty)| (path.clone(), *ty))).chain(root.filter(|(path, _)| !contained.is_some_and(|paths| paths.contains_key(path))));

                state.memory.stage_returned_references(occurrence, references, inputs);
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn stage_reference(
        &self,
        memory: &mut crate::analysis::storage_flow::memory::State,
        occurrence: BoundExecutionSite,
        source: crate::analysis::storage_flow::memory::Source,
        destination: Target,
        reference: &bray_bound_tree::BoundCallReference,
    ) {
        let count = reference.relation().dereferences();

        if count == 0 {
            self.stage_reference_value(memory, occurrence, source, destination, reference);
            return;
        }

        let Some(targets) = source.targets(memory) else {
            return;
        };

        let mut targets = targets
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();

        for _ in 1..count {
            let mut next = std::collections::BTreeSet::new();

            for target in targets {
                let Some(values) = memory.value_targets(&target) else {
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
                return;
            };

            let Some(projections) =
                storage_projections(self.dependencies, relation.target_projections())
            else {
                return;
            };

            let ty = reference
                .addressed_type()
                .expect("projected reference has its checked target type");

            for target in targets {
                memory.stage_address(
                    occurrence,
                    target.project(&projections),
                    destination.clone(),
                    ty,
                    relation.kind(),
                );
            }
        } else if let Some(ty) = reference.addressed_type() {
            let crate::analysis::storage_flow::memory::Source::Value(target) = source else {
                panic!("addressed storage has a location");
            };

            memory.stage_address(occurrence, target, destination.clone(), ty, relation.kind());
        } else {
            source.stage(memory, occurrence, destination.clone());
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
                        .reference_location(start.clone(), path, Some(target))
                })
                .collect(),
            None => Some(vec![state.memory.reference_location(start, path, None)?]),
        }
    }
}
