use std::collections::BTreeMap;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, StorageProjection};
use bray_symbols::{
    DependencyProjection, DependencyReferenceKind, DependencySubject, DependencySubjectRoot,
};

use super::super::flow::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn reference_value_source(
        &self,
        expression: BoundExpressionId,
    ) -> Option<BoundExpressionId> {
        use bray_bound_tree::{
            BoundExpression, BoundStructuredExpressionKind, ConversionTarget, SelectedOperation,
            SemanticSelection,
        };

        match self.request.view().expression(expression)? {
            BoundExpression::Structured(operation)
                if matches!(
                    operation.kind(),
                    BoundStructuredExpressionKind::Condition
                        | BoundStructuredExpressionKind::TrustBoundary
                ) =>
            {
                operation.operands().first().copied()
            }
            BoundExpression::Conversion(conversion) => {
                let SemanticSelection::Operation(SelectedOperation::Conversion(selected)) =
                    self.semantics.selections().expression(expression)?
                else {
                    return None;
                };

                matches!(selected.target(), ConversionTarget::Identity)
                    .then_some(conversion.operand())
            }
            // Children establish evaluation and lifetime dependencies. They do not
            // establish that a constructed value or call result reaches their storage.
            _ => None,
        }
    }

    fn returned_owned_values<'a>(
        &self,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        transfers: impl IntoIterator<Item = &'a bray_symbols::DependencyOwnedTransfer>,
    ) -> Vec<(Vec<StorageProjection>, ExecutionCondition)> {
        use bray_symbols::ConstantProjectionKind;

        let mut fields = Vec::new();

        let Some(dependencies) = self.dependencies.calls().get(&invocation) else {
            return fields;
        };

        for transfer in transfers {
            let input = crate::execution_condition_from_type_term(
                self.request.semantic_values(),
                transfer.input(),
                dependencies.input_count(),
            )
            .expect("checked owned input decodes in the same semantic store");

            let value = input.substitute(
                &|place| {
                    place
                        .value_in(arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &ExecutionCondition::Unknown,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            if value == ExecutionCondition::Unknown {
                continue;
            }

            let path = transfer
                .output()
                .iter()
                .map(|projection| match projection {
                    ConstantProjectionKind::ProductField(field) => {
                        StorageProjection::ProductField(*field)
                    }
                    ConstantProjectionKind::UnionPayloadField(field) => {
                        StorageProjection::ActiveUnionPayloadField {
                            field: *field,
                            variant: self.dependencies.projection_variant(*field),
                        }
                    }
                    ConstantProjectionKind::NullableValue => StorageProjection::NullableValue,
                    ConstantProjectionKind::TupleElement(ordinal) => {
                        StorageProjection::TupleElement(*ordinal)
                    }
                    ConstantProjectionKind::ArrayElementOrdinal(ordinal) => {
                        StorageProjection::ElementFromStart(*ordinal)
                    }
                    _ => unreachable!("checked transfer contains only owning projections"),
                })
                .collect();

            fields.push((path, value));
        }

        fields
    }

    /// Qualify live input evidence by the result layouts that actually retain it.
    pub(super) fn guard_returned_inputs(
        &self,
        state: &mut super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        result: &ExecutionCondition,
    ) {
        use bray_bound_tree::BoundOperator;
        use std::collections::BTreeSet;

        let Some(dependencies) = self.dependencies.calls().get(&invocation) else {
            return;
        };

        let decode = |term| {
            crate::execution_condition_from_type_term(
                self.request.semantic_values(),
                term,
                dependencies.input_count(),
            )
            .expect("checked ownership condition decodes in the same semantic store")
            .substitute(
                &|place| {
                    place
                        .value_in(arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                result,
                &mut { ExecutionCondition::WORK_LIMIT },
            )
        };

        let mut guards = BTreeMap::new();

        for transfer in dependencies.owned_result().transfers() {
            let guard = transfer
                .condition()
                .map(decode)
                .unwrap_or(ExecutionCondition::Boolean(true));

            for value in state.owned_values(&decode(transfer.input())) {
                let owner = value.owner_identity().clone();

                guards
                    .entry(owner)
                    .and_modify(|previous: &mut ExecutionCondition| {
                        *previous = ExecutionCondition::operation(
                            BoundOperator::LogicalOr,
                            vec![previous.clone(), guard.clone()],
                        );
                    })
                    .or_insert_with(|| guard.clone());
            }
        }

        let mut retired = BTreeSet::new();
        let mut published = Vec::new();

        for (owner, guard) in guards {
            if guard.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                == Some(true)
            {
                continue;
            }

            let Some(witness) = state.witnesses.get_mut(&owner) else {
                continue;
            };

            let mut conditions =
                BTreeMap::<_, crate::execution_guarantees::ExecutionGuarantee>::new();

            for (condition, guarantee) in &witness.conditions {
                let qualified = ExecutionCondition::operation(
                    BoundOperator::LogicalOr,
                    vec![
                        ExecutionCondition::operation(
                            BoundOperator::LogicalNot,
                            vec![guard.clone()],
                        ),
                        condition.clone(),
                    ],
                );

                if witness.available
                    && condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
                        == Some(true)
                {
                    published.push(qualified.clone());
                }

                condition.clone().assume(true, &mut retired);
                conditions.entry(qualified).or_default().extend(guarantee);
            }

            if conditions != witness.conditions {
                std::sync::Arc::make_mut(witness).conditions = conditions;
            }
        }

        state.exclude_live_witnesses(&mut retired);

        state
            .trusted_assumptions
            .retain(|fact| !retired.contains(fact));

        for condition in published {
            condition.assume(true, &mut state.trusted_assumptions);
        }
    }

    pub(super) fn retire_consumed_inputs(
        &self,
        state: &mut super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        result: &ExecutionCondition,
    ) {
        let entry = state
            .entries
            .get(&invocation)
            .expect("completed invocation retains its captured ownership");

        let consumed = entry
            .owned_inputs
            .iter()
            .filter_map(|ordinal| ExecutionPlace::argument(*ordinal).value_in(arguments))
            .collect::<Vec<_>>();

        let retained = self
            .dependencies
            .calls()
            .get(&invocation)
            .map(|dependencies| {
                self.returned_owned_values(
                    invocation,
                    arguments,
                    dependencies.owned_result().transfers(),
                )
            })
            .unwrap_or_default()
            .iter()
            .flat_map(|(_, value)| state.owned_values(value))
            .chain(state.owned_values(result))
            .collect();

        state.retire_trusted_values(consumed, &retained);
    }

    fn retire_owned_alternative(
        &self,
        state: &mut super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        result: &ExecutionCondition,
        outside: &std::collections::BTreeSet<ExecutionCondition>,
    ) {
        let consumed = state
            .entries
            .get(&invocation)
            .expect("normal return retains its captured entry")
            .owned_inputs
            .iter()
            .filter_map(|ordinal| ExecutionPlace::argument(*ordinal).value_in(arguments))
            .collect::<Vec<_>>();

        let retained = state
            .owned_values(result)
            .chain(outside.iter().cloned())
            .collect();

        state.retire_trusted_values(consumed, &retained);
    }

    fn complete_owned_result(
        &self,
        invocation: BoundExecutionSite,
        state: &mut super::super::state::ExecutionState,
        mut result: ExecutionCondition,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        ownership: &bray_symbols::DependencyOwnedResult,
        outside: &std::collections::BTreeSet<ExecutionCondition>,
    ) -> Option<ExecutionCondition> {
        use bray_symbols::DependencyOwnedResult;

        match ownership {
            DependencyOwnedResult::Case {
                condition,
                result: layout,
            } => {
                let dependencies = &self.dependencies.calls()[&invocation];

                let condition = crate::execution_condition_from_type_term(
                    self.request.semantic_values(),
                    *condition,
                    dependencies.input_count(),
                )
                .expect("checked ordinary ownership condition decodes in the same store")
                .substitute(
                    &|place| {
                        place
                            .value_in(arguments)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &result,
                    &mut { ExecutionCondition::WORK_LIMIT },
                );

                if condition.prove_trusted(&Default::default(), &state.assumptions) == Some(false) {
                    return None;
                }

                condition.assume(true, &mut state.assumptions);

                self.complete_owned_result(invocation, state, result, arguments, layout, outside)
            }
            DependencyOwnedResult::Transfer(transfer) => {
                let dependencies = &self.dependencies.calls()[&invocation];

                for term in dependencies.returned_value_conditions(transfer) {
                    let condition = crate::execution_condition_from_type_term(
                        self.request.semantic_values(),
                        *term,
                        dependencies.input_count(),
                    )
                    .expect("prepared transport condition belongs to the same semantic store")
                    .substitute(
                        &|place| {
                            place
                                .value_in(arguments)
                                .unwrap_or(ExecutionCondition::Unknown)
                        },
                        &result,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    );

                    condition.assume(true, &mut state.assumptions);
                }

                state.refine_witnesses();

                Some(construct(
                    result,
                    self.returned_owned_values(invocation, arguments, [transfer])
                        .into_iter()
                        .collect(),
                    transfer.condition().is_none(),
                ))
            }
            DependencyOwnedResult::All(values) => {
                for (index, value) in values.iter().enumerate() {
                    let mut retained = outside.clone();

                    retained.extend(
                        values
                            .iter()
                            .enumerate()
                            .filter(|(other, _)| *other != index)
                            .flat_map(|(_, sibling)| {
                                self.returned_owned_values(
                                    invocation,
                                    arguments,
                                    sibling.transfers(),
                                )
                            })
                            .flat_map(|(_, value)| state.owned_values(&value).collect::<Vec<_>>()),
                    );

                    result = self.complete_owned_result(
                        invocation, state, result, arguments, value, &retained,
                    )?;
                }

                Some(result)
            }
            DependencyOwnedResult::Choice(values) => {
                let baseline = state.clone();
                let root = result;
                let mut joined = None;

                for value in values.iter() {
                    let mut branch = baseline.clone();

                    let Some(value) = self.complete_owned_result(
                        invocation,
                        &mut branch,
                        root.clone(),
                        arguments,
                        value,
                        outside,
                    ) else {
                        continue;
                    };

                    self.retire_owned_alternative(
                        &mut branch,
                        invocation,
                        arguments,
                        &value,
                        outside,
                    );

                    match &mut joined {
                        None => joined = Some((branch, value)),
                        Some((previous, previous_value)) => {
                            *previous_value = super::super::flow::merge_returned(previous, &branch, previous_value, &value, &root, match root.owner_identity() { ExecutionCondition::Expression(expression) => *expression, _ => invocation.expression().expect("normal result alternative retains its invocation expression") });
                        }
                    }
                }

                let (completed, result) = joined?;

                *state = completed;

                Some(result)
            }
        }
    }

    pub(in crate::analysis::guarantee) fn complete_returned_values(
        &self,
        invocation: BoundExecutionSite,
        state: &mut super::super::state::ExecutionState,
        result: ExecutionCondition,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> ExecutionCondition {
        let retains_identity = matches!(
            result,
            ExecutionCondition::Expression(_)
                | ExecutionCondition::ScopedCapability(_)
                | ExecutionCondition::Projection(_, _)
        ) || matches!(&result, ExecutionCondition::PostState(
                bray_bound_tree::StorageMutation::Operation(site), _) if *site == invocation);

        if !retains_identity {
            return result;
        }

        let Some(source) = invocation.expression() else {
            return result;
        };

        let Some(dependencies) = self.dependencies.calls().get(&invocation) else {
            return result;
        };

        let result = self
            .complete_owned_result(
                invocation,
                state,
                result,
                arguments,
                dependencies.owned_result(),
                &Default::default(),
            )
            .expect("completed checked call has a feasible ownership layout");

        self.retire_owned_alternative(state, invocation, arguments, &result, &Default::default());

        let has_receiver = match self.semantics.selections().expression(source) {
            Some(bray_bound_tree::SemanticSelection::Call(call)) => call.receiver().is_some(),
            Some(bray_bound_tree::SemanticSelection::ScopedUse(scoped)) => {
                scoped.invocation(invocation).1.receiver().is_some()
            }
            _ => return result,
        };

        let mut fields = BTreeMap::new();

        for relation in dependencies
            .references()
            .iter()
            .filter(|reference| reference.preserves_value())
            .map(|reference| reference.relation())
        {
            if relation.exit().subject_root() != DependencySubjectRoot::Result {
                continue;
            }

            let entry = relation
                .entry()
                .expect("value-preserving reference has an exact input origin");

            let ordinal = match entry.subject_root() {
                DependencySubjectRoot::Receiver => 0,
                DependencySubjectRoot::Parameter(ordinal) => {
                    ordinal.raw() + u32::from(has_receiver)
                }
                _ => continue,
            };

            let Some(input) = project_place(
                ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal)),
                entry,
            ) else {
                continue;
            };

            let Some(value) = input.value_in(arguments) else {
                continue;
            };

            let Some(path) = relation
                .exit()
                .projections()
                .iter()
                .copied()
                .map(projection)
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };

            if value == ExecutionCondition::Unknown {
                continue;
            }

            let value = match relation.kind() {
                DependencyReferenceKind::Borrow => match value {
                    ExecutionCondition::Borrowed(_) => value,
                    value => value.borrowed(),
                },
                DependencyReferenceKind::Address => value,
            };

            fields.insert(path, value);
        }

        construct(result, fields, false)
    }
}

pub(in crate::analysis::guarantee) fn project_place(
    place: ExecutionPlace,
    subject: &DependencySubject,
) -> Option<ExecutionPlace> {
    let path = subject
        .projections()
        .iter()
        .copied()
        .map(projection)
        .collect::<Option<Vec<_>>>()?;

    place.project(&path)
}

fn projection(projection: DependencyProjection) -> Option<StorageProjection> {
    match projection {
        DependencyProjection::ProductField(field) => Some(StorageProjection::ProductField(field)),
        DependencyProjection::TupleElement(index) => Some(StorageProjection::TupleElement(index)),
        DependencyProjection::NullableValue => Some(StorageProjection::NullableValue),
        DependencyProjection::UnionPayloadField(_)
        | DependencyProjection::Element(_)
        | DependencyProjection::OwnedTarget
        | DependencyProjection::UninitPayload => None,
    }
}

fn construct(
    root: ExecutionCondition,
    fields: BTreeMap<Vec<StorageProjection>, ExecutionCondition>,
    owns: bool,
) -> ExecutionCondition {
    let mut children =
        BTreeMap::<StorageProjection, BTreeMap<Vec<StorageProjection>, ExecutionCondition>>::new();

    for (path, value) in fields {
        let Some((first, rest)) = path.split_first() else {
            return value;
        };

        children
            .entry(*first)
            .or_default()
            .insert(rest.to_vec(), value);
    }

    if children.is_empty() {
        return root;
    }

    if children.len() == 1
        && let Some(fields) = children.remove(&StorageProjection::NullableValue)
    {
        if owns || matches!(root, ExecutionCondition::Present(_)) {
            return ExecutionCondition::Present(std::sync::Arc::new(construct(
                root.project(StorageProjection::NullableValue),
                fields,
                owns,
            )));
        }

        children.insert(StorageProjection::NullableValue, fields);
    }

    let root = if owns
        && let Some(StorageProjection::ActiveUnionPayloadField { variant, .. }) =
            children.keys().next()
    {
        ExecutionCondition::Tagged(*variant, std::sync::Arc::new(root))
    } else {
        root
    };

    let (base, mut components) = match root {
        ExecutionCondition::Constructed(base, fields) => {
            (base, fields.iter().cloned().collect::<BTreeMap<_, _>>())
        }
        root => (std::sync::Arc::new(root), BTreeMap::new()),
    };

    for (projection, fields) in children {
        let prior = components
            .remove(&projection)
            .unwrap_or_else(|| base.as_ref().clone().project(projection));

        components.insert(projection, construct(prior, fields, owns));
    }

    ExecutionCondition::Constructed(base, components.into_iter().collect())
}
