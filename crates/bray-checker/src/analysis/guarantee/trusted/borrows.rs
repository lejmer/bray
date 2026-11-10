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
            let value = if borrowed_result { value } else { value.into_referent() };

            let ExecutionCondition::Input(input) = input else {
                unreachable!("a checked ownership source names an input projection");
            };

            let preservation = transfer.preservation().try_map_projections(&mut |projection| {
                Ok::<_, std::convert::Infallible>(crate::storage::owned_storage_projections(
                    self.dependencies, &[*projection],
                )[0])
            }).expect("checked owned data projection mapping is infallible");

            let contents = returned_contents(
                &value, &preservation, invocation, &input,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            let value = if borrowed_result || is_reference(&value) {
                ExecutionCondition::reference_value(value.owner_identity().clone(), ExecutionCondition::Unknown)
            } else {
                ExecutionCondition::owned_value(value.owner_identity().clone(), contents)
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

        for (transfer, enclosing) in dependencies.owned_result().guarded_transfers() {
            let guard = enclosing.into_iter().copied().chain(transfer.condition())
                .map(|condition| decode(condition))
                .fold(ExecutionCondition::Boolean(true), |guard, condition| {
                    ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![guard, condition])
                });

            for value in self.returned_owned_values(invocation, arguments, [transfer])
                .iter().flat_map(|(_, value)| state.owned_values(value))
            {
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

        let result_site = match invocation {
            BoundExecutionSite::ScopedExit(expression) => BoundExecutionSite::ScopedEnter(expression),
            site => site,
        };

        let result_place = self.invocation_result_place(result_site);

        state
            .current
            .live_entries()
            .filter(|(place, _)| {
                // The planner's result slot belongs to this protocol invocation.
                // At exit it is the consumed capability, not a surviving outside owner.
                if result_place.as_ref().is_some_and(|result| result.contains(place)) {
                    return false;
                }

                // Staged arguments belong to this callee after admission. Including
                // them would retain authority even when the call consumes its owner.
                !self.is_invocation_input_place(invocation, place)
            })
            .map(|(_, value)| value)
            .flat_map(|value| state.owned_values(&value))
            .collect()
    }

    fn is_invocation_input_place(&self, invocation: BoundExecutionSite, place: &ExecutionPlace) -> bool {
        let crate::execution_guarantees::ExecutionInput::Storage(identity) = place.root else {
            return false;
        };

        matches!(self.storage.identity(identity), Some(
            bray_bound_tree::StorageIdentity::InvocationInput { expression, ordinal }
        ) if invocation == expression.into()
            || invocation == (BoundExecutionSite::CallInput { expression, ordinal }))
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

    pub(in crate::analysis::guarantee) fn retire_failed_call(
        &self,
        state: &mut super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
    ) {
        let Some(entry) = state.entries.get(&invocation) else {
            return;
        };

        // Admission transfers these actual values to execution even when no
        // normal result is produced. Caller-owned borrows are not owned inputs.
        let consumed = entry.owned_arguments().collect::<Vec<_>>();
        let retained = self.surviving_call_values(state, invocation);

        let private = state.current.iter()
            .filter(|(place, _)| self.is_invocation_input_place(invocation, place))
            .map(|(place, _)| place.clone()).collect::<Vec<_>>();

        for place in private {
            state.current.retire(&place);
        }

        if let Some(place) = self.invocation_result_place(invocation) {
            state.current.retire(&place);
        }

        state.entries.remove(&invocation);
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

                let fields = self.returned_owned_values(invocation, arguments, [transfer]);

                let ExecutionCondition::Input(input) = crate::execution_condition_from_type_term(
                    self.request.semantic_values(), transfer.input(), dependencies.input_count(),
                ).expect("checked ownership source decodes in the same semantic store") else {
                    unreachable!("a checked ownership source names an input projection");
                };

                let preservation = transfer.preservation().try_map_projections(&mut |projection| {
                    Ok::<_, std::convert::Infallible>(crate::storage::owned_storage_projections(
                        self.dependencies, &[*projection],
                    )[0])
                }).expect("checked owned data projection mapping is infallible");

                let ownership = input.value_in(arguments)
                    .map(|value| state.owned_result(&value.into_referent()))
                    .unwrap_or_default()
                    .root_layout()
                    .try_map(
                        &mut |source| Ok::<_, std::convert::Infallible>(DependencyOwnedResult::Transfer(
                            source.clone().with_preservation(source.preservation().intersection(&preservation)),
                        )),
                        &mut |condition| Ok(Some(condition.clone())),
                    ).expect("returned ownership contents composition is infallible");

                for (_, value) in &fields {
                    let actual = DependencyOwnedResult::all([
                        ownership.clone(), state.owned_children_result(value),
                    ]);

                    // Preserve only the transferred top lineage and actual children.
                    state.owned_results.insert(value.clone(), actual);
                }

                Some(construct(result, fields.into_iter().collect(), transfer.condition().is_none()))
            }
            DependencyOwnedResult::All(values) => {
                // Install containing owners first so explicit surviving child
                // transfers refine their data rather than being overwritten.
                let mut ordered = values.iter().enumerate().collect::<Vec<_>>();

                ordered.sort_by_key(|(_, value)| {
                    value.transfers().map(|transfer| transfer.output().len()).min().unwrap_or(usize::MAX)
                });

                for (index, value) in ordered {
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

        let mut result = result;
        let mut fields = BTreeMap::new();

        for subject in dependencies.reference_subjects() {
            if subject.subject_root() != DependencySubjectRoot::Result {
                continue;
            }

            let ty = dependencies.reference_type(subject)
                .expect("prepared reference subject retains its value type");

            if !matches!(self.request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. }) {
                continue;
            }

            let path = crate::storage::storage_projections(self.request.semantic_values(), self.dependencies, subject.projections())
                .expect("checked result reference retains its storage projection");

            let value = path.iter().copied().fold(result.clone(), ExecutionCondition::project)
                .coerce(self.request.semantic_values(), ty, ty);

            if value != ExecutionCondition::Unknown {
                if path.is_empty() {
                    result = value;
                } else {
                    fields.insert(path, value);
                }
            }
        }

        for reference in dependencies.references() {
            let relation = reference.relation();

            if relation.exit().subject_root() != DependencySubjectRoot::Result || !reference.preserves_value() {
                continue;
            }

            let path = crate::storage::storage_projections(self.request.semantic_values(), self.dependencies, relation.exit().projections())
                .expect("checked result reference retains its storage projection");

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

            if value == ExecutionCondition::Unknown {
                continue;
            }

            let value = match relation.kind() {
                DependencyReferenceKind::Borrow => value.borrowed(),
                DependencyReferenceKind::Address => value.into_referent(),
            };

            fields.insert(path, value);
        }

        construct(result, fields, false)
    }
}

fn is_reference(value: &ExecutionCondition) -> bool {
    matches!(value, ExecutionCondition::Borrowed(_))
        || value.observation_kind() == Some(crate::execution_guarantees::ExecutionObservationKind::Reference)
}

fn returned_contents(
    source: &ExecutionCondition,
    preservation: &bray_symbols::DependencyValuePreservation<StorageProjection>,
    invocation: BoundExecutionSite,
    input: &ExecutionPlace,
    budget: &mut usize,
) -> ExecutionCondition {
    let Some(remaining) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = remaining;

    if is_reference(source) && preservation.preserves_value() {
        return ExecutionCondition::reference_value(source.owner_identity().clone(), ExecutionCondition::Unknown);
    }

    let mut contents = if preservation.preserves_value() {
        without_reached_contents(source, budget)
    } else {
        ExecutionCondition::post_state(bray_bound_tree::StorageMutation::Operation(invocation), ExecutionCondition::Input(input.clone()))
    };

    for (projection, child) in preservation.children() {
        let actual = source.clone().project(*projection);

        let place = input.clone().project(&[*projection])
            .expect("owned contents have a checked input projection");

        let value = returned_contents(&actual, child, invocation, &place, budget);

        contents = contents.with_storage_component(&[*projection], value);
    }

    contents
}

/// Entry reference descriptors survive transport independently of mutable reached data.
fn without_reached_contents(value: &ExecutionCondition, budget: &mut usize) -> ExecutionCondition {
    let Some(remaining) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = remaining;

    if is_reference(value) {
        return ExecutionCondition::reference_value(value.owner_identity().clone(), ExecutionCondition::Unknown);
    }

    if let Some(payload) = value.nullable_payload_snapshot() {
        return without_reached_contents(payload, budget);
    }

    match value {
        ExecutionCondition::Observation { owner, contents, kind } => ExecutionCondition::observation(
            owner.as_ref().clone(), without_reached_contents(contents, budget), *kind,
        ),
        ExecutionCondition::Constructed(base, fields) => ExecutionCondition::constructed(
            without_reached_contents(base, budget),
            fields.iter().map(|(projection, value)| {
                (*projection, without_reached_contents(value, budget))
            }).collect(),
        ),
        ExecutionCondition::Present(value) => without_reached_contents(value, budget).present(),
        ExecutionCondition::Tagged(variant, value) => ExecutionCondition::Tagged(
            *variant, std::sync::Arc::new(without_reached_contents(value, budget)),
        ),
        _ => value.clone(),
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

// The checked owned-result layout can establish presence and an active variant.
// Borrowed result projections update data without asserting either condition.
fn construct(
    root: ExecutionCondition,
    mut fields: BTreeMap<Vec<StorageProjection>, ExecutionCondition>,
    owns: bool,
) -> ExecutionCondition {
    if let Some(value) = fields.remove(&Vec::new()) {
        return value;
    }

    if !owns {
        return fields.into_iter().fold(root, |root, (path, value)| {
            root.with_storage_component(&path, value)
        });
    }

    let mut children =
        BTreeMap::<StorageProjection, BTreeMap<Vec<StorageProjection>, ExecutionCondition>>::new();

    for (path, value) in fields {
        let (first, rest) = path.split_first()
            .expect("whole result replacement was selected before its component layout");

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
        return ExecutionCondition::Present(std::sync::Arc::new(construct(
            root.project(StorageProjection::NullableValue),
            fields,
            owns,
        )));
    }

    let root = if let Some(StorageProjection::ActiveUnionPayloadField { variant, .. }) =
            children.keys().next()
    {
        ExecutionCondition::Tagged(*variant, std::sync::Arc::new(root))
    } else {
        root
    };

    children.into_iter().fold(root, |root, (projection, fields)| {
        let prior = root.clone().project(projection);
        let component = construct(prior, fields, owns);

        root.with_storage_component(&[projection], component)
    })
}

#[cfg(test)]
mod tests {
    use super::returned_contents;
    use crate::test_support::{expression_unit, integer_literal_expression, push_expression};
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundExecutionSite, BoundUnitId, StorageProjection};
    use bray_symbols::{DependencyValuePreservation as Mask, SymbolOrdinal};

    #[test]
    fn repeated_identity_returns_keep_current_data_and_finite_snapshots() {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(place.clone());
        let retained = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let changed = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let source = ExecutionCondition::constructed(owner.clone(), [
            (retained, ExecutionCondition::Boolean(true)),
            (changed, ExecutionCondition::Boolean(false)),
        ].into());

        let (_, sites) = expression_unit(BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| push_expression(tree, integer_literal_expression(origin, None))).collect()
        });

        let changed_mask = Mask::new(true, [(changed, Mask::none())].into());

        let contents = returned_contents(&source, &changed_mask, BoundExecutionSite::from(sites[0]),
            &place, &mut { ExecutionCondition::WORK_LIMIT });

        let mut current = ExecutionCondition::owned_value(owner.clone(), contents);
        let expected = current.clone();

        for _ in 0..16 {
            let contents = returned_contents(&current, &Mask::all(), BoundExecutionSite::from(sites[1]),
                &place, &mut { ExecutionCondition::WORK_LIMIT });

            current = ExecutionCondition::owned_value(owner.clone(), contents);
            assert_eq!(current, expected);
        }

        assert_eq!(current.clone().project(retained), ExecutionCondition::Boolean(true));
        assert_ne!(current.project(changed), ExecutionCondition::Boolean(false));
    }
}
