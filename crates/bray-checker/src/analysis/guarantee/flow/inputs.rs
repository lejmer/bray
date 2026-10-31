use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, SemanticSelection};
use bray_symbols::SymbolOrdinal;

use super::super::state::ExecutionState;
use super::model::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn observed_call_result(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<ExecutionCondition> {
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

                    if selection.evaluates_defaults() {
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
                        self.value(state, *expression),
                        conversion,
                    )
                },
            ));
        }

        let SemanticSelection::Call(call) = selection else {
            return None;
        };

        let entry = state.entries.get(&expression.into())?;

        if let Some(bray_bound_tree::CheckedMemoryOperationKind::AtomicCompareExchange {
            value, weak: false, ..
        }) = self.memory.operation(expression).map(|operation| operation.kind())
            && crate::representation::type_representation(self.request, *value)
                .is_some_and(|role| role.integer_representation().is_some())
        {
            let expected = ExecutionPlace::argument(SymbolOrdinal::new(1))
                .value_in(&entry.arguments)
                .expect("checked compare-exchange retains its evaluated expected value");

            let result = ExecutionCondition::Expression(expression);
            let observed = result.clone().component(bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

            // Integer strong success is exactly equality with the observed value, independent
            // of later atomic writes. Weak failure does not establish this relationship.
            let succeeded = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::Equal,
                vec![observed, expected],
            );

            return Some(ExecutionCondition::Constructed(
                std::sync::Arc::new(result),
                [(bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)), succeeded)].into(),
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

        Some(crate::execution_guarantees::selected_call_condition(
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
        ))
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
            BoundExecutionSite::Iterate(_) | BoundExecutionSite::Next(_) => {
                let access = self
                    .storage
                    .invocation_result(input)
                    .expect("implicit input has its actual invocation result");

                self.storage
                    .logical_path(access)
                    .and_then(|path| ExecutionPlace::storage_path(self.storage, path))
            }
            BoundExecutionSite::ScopedEnter(_) => None,
            _ => panic!("call input has an evaluated value site: {input:?}"),
        }
    }

    pub(in crate::analysis::guarantee) fn current_input(
        &self,
        state: &ExecutionState,
        input: BoundExecutionSite,
    ) -> ExecutionCondition {
        match input {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                self.borrowed_value(state, expression)
            }
            BoundExecutionSite::CallInput { .. }
            | BoundExecutionSite::Iterate(_)
            | BoundExecutionSite::Next(_)
            | BoundExecutionSite::ScopedEnter(_) => self.invocation_value(state, input),
            _ => panic!("call input has an evaluated value site: {input:?}"),
        }
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
            BTreeMap::from([(
                ExecutionPlace::argument(SymbolOrdinal::new(0)),
                self.value(state, source),
            )])
        } else {
            BTreeMap::new()
        };

        let trusted_assumptions =
            self.call_trusted_assumptions(state, invocation, &arguments, &BTreeSet::new());

        ExecutionCallEvidence {
            returned: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions,
        }
    }

    pub(in crate::analysis::guarantee) fn complete_input(
        &self,
        state: &mut ExecutionState,
        invocation: BoundExecutionSite,
    ) {
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

        let value = if let Some(provider) = input.default_provider() {
            self.observed_default(selection, provider, ordinal, &entry.arguments)
        } else {
            let value = ExecutionPlace::argument(SymbolOrdinal::new(0))
                .value_in(&entry.arguments)
                .expect("source input entry retains its captured value");

            input.conversion().map_or(value.clone(), |conversion| {
                super::super::completion::converted_argument(value, conversion)
            })
        };

        let place = self
            .input_place(invocation)
            .expect("input slot has an execution place");

        let value = if value == ExecutionCondition::Unknown {
            ExecutionCondition::PostState(
                bray_bound_tree::StorageMutation::Operation(invocation),
                place.clone(),
            )
        } else {
            value
        };

        state.assign(place, value);
    }

    fn observed_default(
        &self,
        selection: &SemanticSelection,
        provider: bray_symbols::DefaultValueProvider,
        ordinal: SymbolOrdinal,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> ExecutionCondition {
        let Some(term) = self.request.default_observed_result(provider) else {
            return ExecutionCondition::Unknown;
        };

        let values = self.request.semantic_values();

        let substitution = selection
            .input_instance()
            .map(|instance| instance.substitution())
            .or_else(
                || match values.type_data(selection.result_type()?).as_ref() {
                    bray_symbols::TypeData::Named { substitution, .. } => Some(*substitution),
                    _ => None,
                },
            );

        let term = match substitution {
            Some(substitution) => values
                .substitute_constant_term(term, substitution)
                .expect("checked default application belongs to this semantic store"),
            None => term,
        };

        let count = if matches!(
            provider,
            bray_symbols::DefaultValueProvider::CallableParameter(_)
        ) {
            usize::try_from(ordinal.raw()).expect("input ordinal fits host")
        } else {
            0
        };

        crate::execution_condition_from_type_term(values, term, count)
            .expect("checked default observation belongs to this semantic store")
            .substitute(
                &|place| {
                    place
                        .value_in(arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &ExecutionCondition::Unknown,
                &mut { ExecutionCondition::WORK_LIMIT },
            )
    }
}
