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

    fn returned_input_value(
        &self,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> Option<ExecutionCondition> {
        let dependencies = self.dependencies.calls().get(&invocation)?;

        let input = crate::execution_condition_from_type_term(
            self.request.semantic_values(),
            dependencies.returned_input()?,
            dependencies.input_count(),
        )
        .expect("checked returned input decodes in the same semantic store");

        let value = input.substitute(
            &|place| {
                place
                    .value_in(arguments)
                    .unwrap_or(ExecutionCondition::Unknown)
            },
            &ExecutionCondition::Unknown,
            &mut { ExecutionCondition::WORK_LIMIT },
        );

        (value != ExecutionCondition::Unknown).then_some(value)
    }

    pub(in crate::analysis::guarantee) fn returned_value_evidence(
        &self,
        state: &super::super::state::ExecutionState,
        invocation: BoundExecutionSite,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> crate::execution_guarantees::ExecutionValueEvidence {
        let Some(value) = self.returned_input_value(invocation, arguments) else {
            return Default::default();
        };

        let witnesses = state
            .witnesses
            .iter()
            .filter(|(carrier, _)| {
                value
                    .owned_values()
                    .any(|owner| carrier.is_owned_projection_of(owner))
            })
            .filter_map(|(carrier, witness)| {
                let conditions = witness
                    .conditions
                    .iter()
                    .filter(|(condition, _)| {
                        condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
                            == Some(true)
                    })
                    .map(|(condition, addresses)| (condition.clone(), addresses.clone()))
                    .collect::<BTreeMap<_, _>>();

                (!conditions.is_empty()).then(|| {
                    (
                        carrier.clone(),
                        crate::execution_guarantees::ExecutionWitness {
                            conditions,
                            dependencies: witness.dependencies.clone(),
                        },
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();

        let allocations = state
            .allocation_owners
            .iter()
            .filter(|(_, owner)| witnesses.contains_key(*owner))
            .map(|(condition, owner)| (condition.clone(), owner.clone()))
            .collect();

        crate::execution_guarantees::ExecutionValueEvidence {
            witnesses,
            allocations,
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
            ExecutionCondition::Expression(_) | ExecutionCondition::ScopedCapability(_)
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

        for condition in dependencies.returned_input_conditions() {
            let condition = crate::execution_condition_from_type_term(
                self.request.semantic_values(),
                *condition,
                dependencies.input_count(),
            )
            .expect("prepared transport condition belongs to the same semantic store")
            .substitute(
                &|place| {
                    place
                        .value_in(arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &ExecutionCondition::Unknown,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            condition.assume(true, &mut state.assumptions);
        }

        state.refine_witnesses();

        if let Some(value) = self.returned_input_value(invocation, arguments) {
            return value;
        }

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

        construct(result, fields)
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

    ExecutionCondition::Constructed(
        std::sync::Arc::new(root.clone()),
        children
            .into_iter()
            .map(|(projection, fields)| {
                (
                    projection,
                    construct(root.clone().project(projection), fields),
                )
            })
            .collect(),
    )
}
