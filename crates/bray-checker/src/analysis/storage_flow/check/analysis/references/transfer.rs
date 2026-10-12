use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Place, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::storage::storage_projections;
use bray_bound_tree::{BoundExecutionSite, SemanticSelection};
use bray_symbols::{DependencySubjectRoot, TypeData};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(in crate::analysis::storage_flow::check::analysis) fn transfer_memory_result_state(
        &self,
        state: &mut StorageFlowState,
        node: bray_bound_tree::AnyBoundNodeId,
    ) {
        let bray_bound_tree::AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

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
                &bray_bound_tree::StorageInvalidation::All,
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
        let Some(expression) = occurrence.expression() else {
            return;
        };

        if self.memory.operation(expression).is_some() {
            return;
        }

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return;
        };

        let Some(SemanticSelection::Call(call)) =
            self.execution.semantics.selections().expression(expression)
        else {
            return;
        };

        state.memory.begin_transition(occurrence);

        for reference in contracts.references() {
            let relation = reference.relation();

            let output = if relation.exit().subject_root() == DependencySubjectRoot::Result {
                self.expression_result_storage(expression)
                    .map(Target::storage)
                    .and_then(|target| {
                        Some(target.project(&storage_projections(
                            self.request,
                            relation.exit().projections(),
                        )?))
                    })
            } else {
                reference.output_argument().and_then(|argument| {
                    let mut targets = self.call_reference_locations(
                        state,
                        argument,
                        relation.exit().projections(),
                        contracts.borrow_path(relation.exit())?,
                    )?;

                    (targets.len() == 1).then(|| targets.pop().expect("one output target"))
                })
            };

            let Some(destination) = output else {
                continue;
            };

            let Some(argument) =
                crate::dependency::result_argument(call, relation.entry().subject_root())
            else {
                continue;
            };

            let sources = if relation.entry().projections().is_empty()
                && (reference.addressed_type().is_none()
                    || !relation.target_projections().is_empty())
            {
                self.expression_reference(state, argument)
                    .into_iter()
                    .collect::<Vec<_>>()
            } else {
                let Some(targets) = self.call_reference_locations(
                    state,
                    argument,
                    relation.entry().projections(),
                    contracts
                        .borrow_path(relation.entry())
                        .expect("prepared reference input has a checked path"),
                ) else {
                    continue;
                };

                targets
                    .into_iter()
                    .map(super::super::memory::ReferenceSource::Value)
                    .collect()
            };

            for source in sources {
                if !relation.target_projections().is_empty() {
                    let Some(targets) = source.targets(&state.memory) else {
                        continue;
                    };

                    let Some(projections) =
                        storage_projections(self.request, relation.target_projections())
                    else {
                        continue;
                    };

                    let ty = reference
                        .addressed_type()
                        .expect("projected reference has its checked target type");

                    for target in targets {
                        state.memory.stage_address(
                            occurrence,
                            target.project(&projections),
                            destination.clone(),
                            ty,
                        );
                    }
                } else if let Some(ty) = reference.addressed_type() {
                    let super::super::memory::ReferenceSource::Value(target) = source else {
                        panic!("addressed storage has a location");
                    };

                    state
                        .memory
                        .stage_address(occurrence, target, destination.clone(), ty);
                } else {
                    source.stage(&mut state.memory, occurrence, destination.clone());
                }
            }
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn call_reference_locations(
        &self,
        state: &StorageFlowState,
        argument: bray_bound_tree::BoundExpressionId,
        projections: &[bray_symbols::DependencyProjection],
        borrows: &[(usize, bray_symbols::TypeId)],
    ) -> Option<Vec<Target>> {
        let projections = storage_projections(self.request, projections)?;
        let mut targets = vec![Target::storage(self.argument_storage(argument)?)];
        let mut copied = 0;

        for (step, (index, ty)) in borrows.iter().enumerate() {
            targets = targets
                .into_iter()
                .map(|target| target.project(&projections[copied..*index]))
                .collect();

            if step == 0 && *index == 0 {
                let borrowed = self
                    .execution
                    .semantics
                    .types()
                    .expression(argument)
                    .is_some_and(|ty| {
                        matches!(
                            self.request.semantic_values().type_data(ty.ty()).as_ref(),
                            TypeData::Borrow { .. }
                        )
                    });

                if borrowed {
                    targets = self
                        .expression_reference(state, argument)?
                        .targets(&state.memory)?;
                }

                // An implicit shared or mutable argument conversion borrows the actual value slot.
            } else {
                targets = targets
                    .iter()
                    .map(|target| state.memory.value_target(target, *ty))
                    .collect::<Option<Vec<_>>>()?;
            }

            copied = *index;
        }

        Some(
            targets
                .into_iter()
                .map(|target| target.project(&projections[copied..]))
                .collect(),
        )
    }
}
