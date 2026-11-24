use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, SemanticSelection};
use bray_symbols::SymbolOrdinal;

use super::super::state::ExecutionState;
use super::model::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// The checked protected read selects the reached payload of its evaluated wrapper.
    pub(in crate::analysis::guarantee) fn protected_extraction_payload(
        &self,
        invocation: BoundExecutionSite,
        entry: &ExecutionCallEvidence,
    ) -> Option<ExecutionCondition> {
        let BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) =
            invocation
        else {
            return None;
        };

        self.memory.operation(expression).filter(|operation| {
            matches!(
                operation.kind(),
                bray_bound_tree::CheckedMemoryOperationKind::UninitAssumeInitialized { .. }
                    | bray_bound_tree::CheckedMemoryOperationKind::UninitMove { .. }
            )
        })?;

        let wrapper = ExecutionPlace::argument(SymbolOrdinal::new(0))
            .value_in(&entry.arguments)
            .expect("protected extraction retains its evaluated storage");

        Some(
            wrapper
                .argument_value(false)
                .project(bray_bound_tree::StorageProjection::UninitPayload),
        )
    }

    pub(super) fn complete_implicit_invocation(
        &self,
        state: &mut ExecutionState,
        invocation: BoundExecutionSite,
        observations: &bray_bound_tree::StorageCompletion,
    ) {
        if let Some(entry) = state.admitted_call_mut(invocation) {
            entry.pending_execution = false;
        }

        let place = self.invocation_result_place(invocation);
        let storage = self
            .storage
            .invocation_result(invocation)
            .and_then(|access| self.storage.root_identity(access));

        // An infallible enter produces the capability directly. Other results keep their own type and slot.
        let result = match storage.and_then(|identity| self.storage.identity(identity)) {
            Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }) => {
                ExecutionCondition::new(crate::ExecutionConditionKind::ScopedCapability(expression))
            }
            _ => place
                .as_ref()
                .map_or_else(ExecutionCondition::unknown, |place| {
                    ExecutionCondition::post_state(
                        bray_bound_tree::StorageMutation::Operation(invocation),
                        ExecutionCondition::new(crate::ExecutionConditionKind::Input(
                            place.clone(),
                        )),
                    )
                }),
        };

        let result = self.complete_trusted_call(
            state,
            (invocation, invocation),
            None,
            place.as_ref(),
            result,
            observations,
        );

        if let Some(place) = place {
            state.assign(place, result, None);
        }
    }

    pub(in crate::analysis) fn observe_memory_read(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        target: Option<&bray_bound_tree::StorageTarget>,
    ) -> bool {
        let Some(pointee) = self
            .memory
            .operation(expression)
            .and_then(|operation| operation.kind().moving_read_type())
        else {
            return false;
        };

        let Some(target) = target else {
            return false;
        };

        let path = target.value_observation_path();

        let Some(value) = self.current_target_value(state, target, Some(pointee)) else {
            return false;
        };

        let value = if matches!(
            self.request.semantic_values().type_data(pointee).as_ref(),
            bray_symbols::TypeData::Borrow { .. }
        ) {
            value
        } else {
            value.argument_value(false)
        };

        if let Some(path) = path {
            self.observe_move(state, &path, pointee, &value);
        } else {
            state.vacate(ExecutionPlace::pointee(self.storage, target, pointee));
        }

        state.pending_results.insert(expression, value);

        true
    }

    pub(in crate::analysis::guarantee) fn observed_call_result(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<ExecutionCondition> {
        if self
            .memory
            .operation(expression)
            .is_some_and(|operation| operation.kind().moving_read_type().is_some())
        {
            if let Some(value) = state.pending_results.get(&expression) {
                return Some(value.clone());
            }
        }

        let selection = self.semantics.selections().expression(expression)?;

        if let SemanticSelection::Operation(bray_bound_tree::SelectedOperation::Construction(
            construction,
        )) = selection
            && matches!(
                construction.target(),
                bray_bound_tree::ConstructionTarget::Struct(_)
                    | bray_bound_tree::ConstructionTarget::UnionVariant(_)
            )
        {
            return Some(crate::execution_guarantees::construction_condition(
                expression,
                construction,
                |input| {
                    let (bray_bound_tree::SelectedConstructionInput::Explicit { ordinal, .. }
                    | bray_bound_tree::SelectedConstructionInput::Default { ordinal, .. }) = input;

                    if selection.stages_inputs() {
                        return self.invocation_value(
                            state,
                            BoundExecutionSite::CallInput {
                                expression,
                                ordinal: SymbolOrdinal::new(*ordinal),
                            },
                        );
                    }

                    let bray_bound_tree::SelectedConstructionInput::Explicit {
                        expression,
                        conversion,
                        ..
                    } = input
                    else {
                        unreachable!("unstaged construction retains explicit inputs");
                    };

                    super::super::completion::converted_argument(
                        self.request.semantic_values(),
                        self.operand_value(state, *expression),
                        conversion,
                    )
                },
            ));
        }

        let SemanticSelection::Call(call) = selection else {
            return None;
        };

        let entry = state.entries.get(&expression.into())?;
        let memory = self
            .memory
            .operation(expression)
            .map(|operation| operation.kind());

        if matches!(
            memory,
            Some(bray_bound_tree::CheckedMemoryOperationKind::UninitFrom { .. })
        ) {
            let payload = ExecutionPlace::argument(SymbolOrdinal::new(0))
                .value_in(&entry.arguments)
                .expect("initialized storage retains its evaluated payload");

            let wrapper =
                ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));

            return Some(ExecutionCondition::constructed(
                wrapper,
                [(bray_bound_tree::StorageProjection::UninitPayload, payload)].into(),
            ));
        }

        if let Some(payload) = self.protected_extraction_payload(expression.into(), entry) {
            return Some(payload);
        }

        if let Some(bray_bound_tree::CheckedMemoryOperationKind::AtomicCompareExchange {
            value,
            weak: false,
            ..
        }) = memory
            && crate::representation::type_representation(self.request, *value)
                .is_some_and(|role| role.integer_representation().is_some())
        {
            let expected = ExecutionPlace::argument(SymbolOrdinal::new(1))
                .value_in(&entry.arguments)
                .expect("checked compare-exchange retains its evaluated expected value");

            let result =
                ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));

            let observed =
                result
                    .clone()
                    .component(bray_symbols::ConstantProjectionKind::TupleElement(
                        SymbolOrdinal::new(0),
                    ));

            // Integer strong success is exactly equality with the observed value, independent
            // of later atomic writes. Weak failure does not establish this relationship.
            let succeeded = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::Equal,
                vec![observed, expected],
            );

            return Some(ExecutionCondition::constructed(
                result,
                [(
                    bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)),
                    succeeded,
                )]
                .into(),
            ));
        }

        if call.implementation_hook()
            == Some(bray_compiler_known::ImplementationHook::NumericTruncate)
            && let [bray_bound_tree::SelectedArgument::Explicit { conversion, .. }] =
                call.arguments()
            && let bray_bound_tree::BoundCallResult::Immediate(target) = call.resolution().result()
            && crate::representation::integer_conversion_representations(
                self.request.semantic_values(),
                self.request.available_compiler_known_symbols(),
                conversion.target_type(),
                target,
            )
            .is_some()
            && let Some((minimum, maximum)) = self.integer_type_range(target)
        {
            let argument = ExecutionPlace::argument(SymbolOrdinal::new(0))
                .value_in(&entry.arguments)
                .expect("integer truncation retains its evaluated source");

            if self
                .observed_integer_range(state, &argument, &mut Default::default(), &mut {
                    ExecutionCondition::WORK_LIMIT
                })
                .is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum)
            {
                return Some(argument);
            }
        }

        let count = call.arguments().len() + usize::from(call.receiver().is_some());

        let result = crate::execution_guarantees::selected_call_condition(
            self.request.semantic_values(),
            self.request
                .trusted_contracts()
                .map(|inputs| &inputs.predicates),
            expression,
            call,
            |_| {
                (0..count)
                    .map(|ordinal| {
                        ExecutionPlace::argument(SymbolOrdinal::new(
                            u32::try_from(ordinal).expect("input ordinal fits"),
                        ))
                        .value_in(&entry.arguments)
                        .expect("captured call retains every selected input")
                    })
                    .collect()
            },
            &mut { ExecutionCondition::WORK_LIMIT },
        );

        if matches!(result.kind(), crate::ExecutionConditionKind::Call(..))
            && let bray_bound_tree::BoundCallResult::Immediate(ty) = call.resolution().result()
            && !crate::representation::type_representation(self.request, ty)
                .is_some_and(crate::representation::is_scalar_value)
        {
            if matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            ) {
                return Some(ExecutionCondition::reference_value(
                    ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))
                        .borrowed(),
                    result,
                ));
            }

            // Const computation describes data, not the identity of an owner
            // produced by this invocation. Returned transfers refine this slot.
            return Some(ExecutionCondition::new(
                crate::ExecutionConditionKind::Expression(expression),
            ));
        }

        Some(result)
    }

    pub(in crate::analysis::guarantee) fn input_place(
        &self,
        input: BoundExecutionSite,
    ) -> Option<ExecutionPlace> {
        match input {
            BoundExecutionSite::CallInput {
                expression,
                ordinal,
            } => {
                let access = self
                    .storage
                    .invocation_input(expression, ordinal)
                    .expect("staged input has its exact storage");

                self.storage
                    .logical_path(access)
                    .and_then(|path| ExecutionPlace::storage_path(self.storage, path))
            }
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                crate::execution_guarantees::expression_place(
                    self.request.unit(),
                    self.semantics,
                    self.request.semantic_values(),
                    expression,
                )
            }
            BoundExecutionSite::Iterate(_)
            | BoundExecutionSite::Next(_)
            | BoundExecutionSite::ScopedEnter(_) => {
                let access = self
                    .storage
                    .invocation_result(input)
                    .expect("implicit input has its actual invocation result");

                self.storage
                    .logical_path(access)
                    .and_then(|path| ExecutionPlace::storage_path(self.storage, path))
            }
            _ => panic!("call input has an evaluated value site: {input:?}"),
        }
    }

    pub(in crate::analysis::guarantee) fn current_input(
        &self,
        state: &ExecutionState,
        input: BoundExecutionSite,
    ) -> ExecutionCondition {
        let value = match input {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                self.operand_value(state, expression)
            }
            BoundExecutionSite::CallInput { .. }
            | BoundExecutionSite::Iterate(_)
            | BoundExecutionSite::Next(_)
            | BoundExecutionSite::ScopedEnter(_) => self.invocation_value(state, input),
            _ => panic!("call input has an evaluated value site: {input:?}"),
        };

        self.refresh_captured_references(state, value, None)
    }
    pub(in crate::analysis::guarantee) fn staged_arguments(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        selection: &SemanticSelection,
        before: Option<SymbolOrdinal>,
    ) -> BTreeMap<ExecutionPlace, ExecutionCondition> {
        let mut arguments = BTreeMap::new();

        for input in selection.inputs() {
            let ordinal = selection
                .input_ordinal(input.root())
                .expect("selected input has its ordinal");

            if before.is_some_and(|before| ordinal >= before) {
                continue;
            }

            let value = self.invocation_value(
                state,
                BoundExecutionSite::CallInput {
                    expression,
                    ordinal,
                },
            );

            if let Some(reference) = input.reference() {
                arguments.insert(reference.into(), value.clone());
            }

            arguments.insert(ExecutionPlace::argument(ordinal), value);
        }

        arguments
    }

    pub(in crate::analysis::guarantee) fn owned_input_ordinals(
        &self,
        selection: &SemanticSelection,
    ) -> std::sync::Arc<[SymbolOrdinal]> {
        bray_base::sorted_unique_shared_slice(
            selection
                .inputs()
                .filter(|input| {
                    if let bray_bound_tree::SelectedInput::Receiver(receiver) = input {
                        return matches!(
                            receiver.mode(),
                            bray_symbols::ReceiverMode::Consuming
                                | bray_symbols::ReceiverMode::ConsumingMutable
                        );
                    }

                    let ty = input.ty(self.request.semantic_values());

                    !matches!(
                        self.request.semantic_values().type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    ) && !self.storage.copyable_types().contains(&ty)
                })
                .map(|input| {
                    selection
                        .input_ordinal(input.root())
                        .expect("checked owned input retains its ordinal")
                }),
        )
    }

    pub(in crate::analysis::guarantee) fn input_borrow_kinds(
        &self,
        selection: &SemanticSelection,
    ) -> BTreeMap<SymbolOrdinal, bray_symbols::BorrowKind> {
        selection
            .inputs()
            .filter_map(|input| {
                let ordinal = selection
                    .input_ordinal(input.root())
                    .expect("checked input retains its ordinal");
                let values = self.request.semantic_values();
                let data = values.type_data(input.ty(values));
                let bray_symbols::TypeData::Borrow { kind, .. } = data.as_ref() else {
                    return None;
                };

                Some((ordinal, *kind))
            })
            .collect()
    }

    pub(in crate::analysis::guarantee) fn input_ordinals(
        &self,
        selection: &SemanticSelection,
    ) -> BTreeMap<ExecutionPlace, SymbolOrdinal> {
        selection
            .inputs()
            .filter_map(|input| {
                let place = ExecutionPlace::from(input.reference()?);
                let ordinal = selection
                    .input_ordinal(input.root())
                    .expect("checked input retains its ordinal");

                Some((place, ordinal))
            })
            .collect()
    }

    pub(in crate::analysis::guarantee) fn input_entry(
        &self,
        state: &ExecutionState,
        invocation: BoundExecutionSite,
    ) -> ExecutionCallEvidence {
        let BoundExecutionSite::CallInput {
            expression,
            ordinal,
        } = invocation
        else {
            panic!("input entry requires an input occurrence");
        };

        let selection = self
            .semantics
            .selections()
            .expression(expression)
            .expect("input entry retains its selected invocation");

        let input = selection
            .input(ordinal)
            .expect("input entry retains its exact input");

        let arguments = if matches!(
            input.default_provider(),
            Some(bray_symbols::DefaultValueProvider::CallableParameter(_))
        ) {
            self.staged_arguments(state, expression, selection, Some(ordinal))
        } else if let Some(source) = input.expression() {
            let place = self
                .input_place(invocation)
                .expect("input materialization retains its slot");

            let identity = ExecutionCondition::post_state(
                bray_bound_tree::StorageMutation::Operation(invocation),
                ExecutionCondition::new(crate::ExecutionConditionKind::Input(place)),
            );

            let value = self
                .storage
                .occurrence_plans(invocation)
                .fold(self.operand_value(state, source), |value, plan| {
                    self.copied_access_value(identity.clone(), value, plan)
                });

            BTreeMap::from([(ExecutionPlace::argument(SymbolOrdinal::new(0)), value)])
        } else {
            BTreeMap::new()
        };

        let trusted_assumptions =
            self.call_trusted_assumptions(state, invocation, &arguments, &BTreeSet::new());

        ExecutionCallEvidence {
            failure_rows: None,
            outputs: Default::default(),
            implementation_witnesses: match selection {
                SemanticSelection::Call(call) => {
                    call.resolution().implementation_witnesses().into()
                }
                _ => Default::default(),
            },
            witness_inputs: Default::default(),
            witness_allocations: Default::default(),
            extracted_values: Default::default(),
            owned_inputs: Default::default(),
            input_borrow_kinds: Default::default(),
            input_ordinals: Default::default(),
            borrow_owners: Default::default(),
            addresses: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            storage_unchanged: false,
            memory_access: None,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions,
        }
    }

    pub(in crate::analysis::guarantee) fn complete_input(
        &self,
        state: &mut ExecutionState,
        invocation: BoundExecutionSite,
    ) -> bool {
        let BoundExecutionSite::CallInput {
            expression,
            ordinal,
        } = invocation
        else {
            panic!("input completion requires an input occurrence");
        };

        let selection = self
            .semantics
            .selections()
            .expression(expression)
            .expect("input completion retains its selected invocation");

        let input = selection
            .input(ordinal)
            .expect("input completion retains its exact input");

        let entry = state
            .entries
            .get(&invocation)
            .expect("input completion follows its captured entry");

        let value = if input.default_provider().is_some() {
            // The guarded call completion supplies the default's produced value.
            ExecutionCondition::unknown()
        } else {
            let value = ExecutionPlace::argument(SymbolOrdinal::new(0))
                .value_in(&entry.arguments)
                .expect("source input entry retains its captured value");

            match input {
                bray_bound_tree::SelectedInput::Receiver(receiver) => value.coerce(
                    self.request.semantic_values(),
                    receiver.source_type(),
                    input.ty(self.request.semantic_values()),
                ),
                _ => input.conversion().map_or(value.clone(), |conversion| {
                    super::super::completion::converted_argument(
                        self.request.semantic_values(),
                        value,
                        conversion,
                    )
                }),
            }
        };

        let place = self
            .input_place(invocation)
            .expect("input slot has an execution place");

        let value = if value == ExecutionCondition::unknown() {
            ExecutionCondition::post_state(
                bray_bound_tree::StorageMutation::Operation(invocation),
                ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())),
            )
        } else {
            value
        };

        state.assign(place, value, None);

        input.inline_materialization_path().is_some()
    }
}
