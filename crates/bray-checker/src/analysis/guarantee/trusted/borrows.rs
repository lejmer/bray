use std::collections::BTreeMap;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, StorageProjection};
use bray_symbols::{
    DependencyReferenceKind, DependencySubject, DependencySubjectRoot,
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

    pub(in crate::analysis::guarantee) fn returned_owned_values<'a>(
        &self,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        transfers: impl IntoIterator<Item = &'a bray_symbols::DependencyOwnedTransfer>,
    ) -> Vec<(Vec<StorageProjection>, ExecutionCondition)> {
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

            let path = crate::storage::owned_storage_projections(self.dependencies, transfer.output());

            let borrowed_result = crate::storage::dependency_projections(&path)
                .and_then(|path| dependencies.reference_type(&DependencySubject::new(
                    DependencySubjectRoot::Result, path)))
                .is_some_and(|ty| matches!(self.request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. }));

            // A checked ownership transfer can extract the value from a borrowed
            // slot. Returning a borrow instead retains its original loan identity.
            let value = match value {
                ExecutionCondition::Borrowed(value) if !borrowed_result => value.as_ref().clone(),
                value => value,
            };

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

    /// Owners outside the completing frame keep copied observations alive.
    pub(super) fn surviving_call_values(
        &self,
        state: &super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
    ) -> std::collections::BTreeSet<ExecutionCondition> {
        if state.entries[&invocation].owned_inputs.is_empty() {
            return Default::default();
        }

        state
            .current
            .iter()
            .filter(|(place, _)| {
                let crate::execution_guarantees::ExecutionInput::Storage(identity) = place.root
                else {
                    return true;
                };

                // Staged arguments belong to this callee after admission. Including
                // them would retain authority even when the call consumes its owner.
                match self.storage.identity(identity) {
                    Some(bray_bound_tree::StorageIdentity::InvocationInput {
                        expression,
                        ordinal,
                    }) => {
                        invocation != expression.into()
                            && invocation
                                != BoundExecutionSite::CallInput {
                                    expression,
                                    ordinal,
                                }
                    }
                    Some(bray_bound_tree::StorageIdentity::InvocationResult(source)) => {
                        source != invocation
                    }
                    _ => true,
                }
            })
            .map(|(_, value)| value)
            .chain(std::iter::once(&state.result))
            .flat_map(|value| state.owned_values(value))
            .collect()
    }

    pub(super) fn retire_consumed_inputs(
        &self,
        state: &mut super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
        result: &ExecutionCondition,
        outside: &std::collections::BTreeSet<ExecutionCondition>,
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
            .chain(outside.iter().cloned())
            .chain(self.preserved_borrowed_inputs(invocation, arguments))
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
            .chain(self.preserved_borrowed_inputs(invocation, arguments))
            .collect();

        state.retire_trusted_values(consumed, &retained);
    }

    fn preserved_borrowed_inputs<'s>(
        &'s self,
        invocation: BoundExecutionSite,
        arguments: &'s BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> impl Iterator<Item = ExecutionCondition> + 's {
        let selection = invocation
            .expression()
            .and_then(|expression| self.semantics.selections().expression(expression));

        self.dependencies
            .calls()
            .get(&invocation)
            .into_iter()
            .flat_map(|dependencies| dependencies.references())
            .filter_map(move |reference| {
                let relation = reference.relation();

                if !reference.preserves_value()
                    || relation.kind() != DependencyReferenceKind::Borrow
                    || relation.entry() != Some(relation.exit())
                    || !relation.is_exact()
                    || relation.dereferences() != 0
                    || !relation.target_projections().is_empty()
                {
                    return None;
                }

                let subject = relation.exit();

                let ordinal =
                    selection?.invocation_input_ordinal(invocation, subject.subject_root())?;

                // A checked preserved reference names caller-owned storage. Consuming
                // its containing value retires the descriptor, not the referent.
                project_place(self.request.semantic_values(), self.dependencies, ExecutionPlace::argument(ordinal), subject)?.value_in(arguments)
            })
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

                let mut fields = self.returned_owned_values(invocation, arguments, [transfer]);

                for (_, value) in &mut fields {
                    if !state.witness_projection(value).1 {
                        continue;
                    }

                    let ExecutionCondition::Input(input) = crate::execution_condition_from_type_term(
                        self.request.semantic_values(), transfer.input(), dependencies.input_count(),
                    ).expect("checked ownership source decodes in the same semantic store") else {
                        unreachable!("a checked ownership source names an input projection");
                    };

                    let independent = value.ownership_ancestors().skip(1)
                        .filter(|parent| state.directly_carries_witness(parent))
                        .all(|parent| super::transfers::independent_result_owner(
                            self, invocation, &state.entries[&invocation], transfer, &input,
                            &state.witnesses[parent.owner_identity()],
                        ));

                    if independent {
                        // The checked transition publishes a complete child. Its existing
                        // identity, contents, evidence and portable lineage travel together.
                        let ownership = state.owned_result(value);

                        *value = ExecutionCondition::Constructed(std::sync::Arc::new(value.clone()), [].into());
                        state.owned_results.insert(value.clone(), ownership);
                    }
                }

                Some(construct(result, fields.into_iter().collect(), transfer.condition().is_none()))
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
        outside: &std::collections::BTreeSet<ExecutionCondition>,
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
                outside,
            )
            .expect("completed checked call has a feasible ownership layout");

        self.retire_owned_alternative(state, invocation, arguments, &result, outside);

        result
    }

    pub(in crate::analysis::guarantee) fn complete_returned_references(
        &self,
        invocation: BoundExecutionSite,
        result: ExecutionCondition,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> ExecutionCondition {
        let Some(source) = invocation.expression() else {
            return result;
        };

        let Some(dependencies) = self.dependencies.calls().get(&invocation) else {
            return result;
        };

        let selection = self
            .semantics
            .selections()
            .expression(source)
            .expect("completed checked call retains its selection");

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

            let Some(ordinal) =
                selection.invocation_input_ordinal(invocation, entry.subject_root())
            else {
                continue;
            };

            let Some(input) = project_place(self.request.semantic_values(), self.dependencies, ExecutionPlace::argument(ordinal), entry) else {
                continue;
            };

            let Some(value) = input.value_in(arguments) else {
                continue;
            };

            let Some(path) = crate::storage::storage_projections(self.request.semantic_values(), self.dependencies, relation.exit().projections())
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
    values: &bray_symbols::SemanticValueStore,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    place: ExecutionPlace,
    subject: &DependencySubject,
) -> Option<ExecutionPlace> {
    place.project(&crate::storage::storage_projections(values, dependencies, subject.projections())?)
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
