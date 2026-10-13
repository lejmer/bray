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

    pub(in crate::analysis::guarantee) fn complete_returned_references(
        &self,
        invocation: BoundExecutionSite,
        result: ExecutionCondition,
        arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    ) -> ExecutionCondition {
        let ExecutionCondition::Expression(expression) = result else {
            return result;
        };

        if invocation != expression.into() {
            return result;
        }

        let Some(dependencies) = self.dependencies.calls().get(&expression.into()) else {
            return result;
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            self.semantics.selections().expression(expression)
        else {
            return result;
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

            let ordinal = match relation.entry().subject_root() {
                DependencySubjectRoot::Receiver => 0,
                DependencySubjectRoot::Parameter(ordinal) => {
                    ordinal.raw() + u32::from(call.receiver().is_some())
                }
                _ => continue,
            };

            let Some(input) = project_place(
                ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(ordinal)),
                relation.entry(),
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
