use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnit, CheckedSemanticSelections, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, DependencyProjection, SymbolOrdinal};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct AssignmentInputs {
    places: BTreeMap<BoundExpressionId, Vec<ValuePlace>>,
    by_root: BTreeMap<BoundReferenceTarget, Vec<Assignment>>,
}

type ValuePlace = (BoundReferenceTarget, Vec<Option<DependencyProjection>>);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Assignment {
    value: BoundExpressionId,
    destination: Vec<Option<DependencyProjection>>,
}

#[derive(Clone, Copy)]
pub(super) struct AssignedValue<'a> {
    pub(super) value: BoundExpressionId,
    read: &'a [Option<DependencyProjection>],
    destination: &'a [Option<DependencyProjection>],
}

impl<'a> AssignedValue<'a> {
    pub(super) fn source(self) -> impl Iterator<Item = DependencyProjection> + 'a {
        // Unknown indexes retain the assigned aggregate's complete value contract.
        self.read
            .iter()
            .skip(self.destination.len())
            .copied()
            .map_while(|projection| projection)
    }

    pub(super) fn matches(self, path: &[DependencyProjection]) -> bool {
        !self
            .destination
            .iter()
            .skip(self.read.len())
            .zip(path)
            .any(|(left, right)| left.is_some_and(|left| left != *right))
    }

    pub(super) fn project(
        self,
        path: &[DependencyProjection],
    ) -> Option<(BoundExpressionId, Vec<DependencyProjection>)> {
        self.matches(path).then(|| {
            (
                self.value,
                self.source()
                    .chain(
                        path.iter()
                            .skip(self.destination.len().saturating_sub(self.read.len()))
                            .copied(),
                    )
                    .collect(),
            )
        })
    }
}

impl AssignmentInputs {
    pub(super) fn new(
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        inputs: &super::ValueInputs,
    ) -> Self {
        let places = unit
            .tree()
            .expressions()
            .filter_map(|(id, _)| {
                let places = value_places(unit, selections, inputs, id)
                    .into_iter()
                    .collect::<Vec<_>>();

                (!places.is_empty()).then_some((id, places))
            })
            .collect::<BTreeMap<_, _>>();

        let mut by_root = BTreeMap::<_, Vec<_>>::new();

        for (_, expression) in unit.tree().expressions() {
            let BoundExpression::Assignment(assignment) = expression else {
                continue;
            };

            let [destination, value] = assignment.operands() else {
                continue;
            };

            for (root, path) in places.get(destination).into_iter().flatten() {
                // Each storage root owns its alternatives once, independently of its read count.
                by_root.entry(*root).or_default().push(Assignment {
                    value: *value,
                    destination: path.to_vec(),
                });
            }
        }

        Self { places, by_root }
    }

    pub(super) fn values(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = AssignedValue<'_>> {
        self.places
            .get(&expression)
            .into_iter()
            .flatten()
            .flat_map(|(root, read)| {
                self.by_root
                    .get(root)
                    .into_iter()
                    .flatten()
                    .filter_map(move |assignment| {
                        let compatible =
                            !assignment
                                .destination
                                .iter()
                                .zip(read)
                                .any(|(left, right)| {
                                    left.is_some() && right.is_some() && left != right
                                });

                        compatible.then_some(AssignedValue {
                            value: assignment.value,
                            read,
                            destination: &assignment.destination,
                        })
                    })
            })
    }
}

pub(super) fn value_places(
    unit: &BoundUnit,
    selections: &CheckedSemanticSelections,
    inputs: &super::ValueInputs,
    expression: BoundExpressionId,
) -> BTreeSet<(BoundReferenceTarget, Vec<Option<DependencyProjection>>)> {
    let mut places = BTreeSet::new();
    let mut pending = vec![(expression, Vec::new())];
    let mut visited = BTreeSet::new();

    while let Some((expression, path)) = pending.pop() {
        if !visited.insert((expression, path.clone())) {
            continue;
        }

        if let Some(borrowed) = borrowed_initializer(unit, inputs, expression) {
            for (source, projections) in borrowed {
                let (source, projections) = inputs.project(source, &projections);

                let mut source_path = projections.iter().copied().map(Some).collect::<Vec<_>>();

                source_path.extend(path.iter().copied());
                pending.push((source, source_path));
            }

            continue;
        }

        let next = match unit.tree().expression(expression) {
            Some(BoundExpression::Name(name)) => {
                places.insert((name.target(), path));
                continue;
            }
            Some(BoundExpression::PatternReference(reference)) => {
                places.insert((
                    BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(reference.binding())),
                    path,
                ));

                continue;
            }
            Some(BoundExpression::MemberAccess(member)) => {
                member_projection(unit, selections, expression)
                    .map(|projection| (member.receiver(), Some(projection)))
            }
            Some(BoundExpression::TraitQualifiedMember(member)) => {
                member_projection(unit, selections, expression)
                    .map(|projection| (member.receiver(), Some(projection)))
            }
            Some(BoundExpression::Structured(value))
                if value.kind() == BoundStructuredExpressionKind::ElementIndex =>
            {
                value.operands().first().map(|source| (*source, None))
            }
            _ => None,
        };

        if let Some((source, projection)) = next {
            let mut projected = vec![projection];

            projected.extend(path);
            pending.push((source, projected));
        }
    }

    places
}

fn borrowed_initializer(
    unit: &BoundUnit,
    inputs: &super::ValueInputs,
    mut expression: BoundExpressionId,
) -> Option<Vec<(BoundExpressionId, Vec<DependencyProjection>)>> {
    let mut visited = BTreeSet::new();

    while visited.insert(expression) {
        if let Some(BoundExpression::Structured(borrow)) = unit.tree().expression(expression)
            && borrow.kind() == BoundStructuredExpressionKind::Borrow
        {
            return Some(
                borrow
                    .operands()
                    .first()
                    .map(|source| (*source, Vec::new()))
                    .into_iter()
                    .collect(),
            );
        }

        if let Some(sources) = inputs.borrowed.get(&expression) {
            // Assignment paths own their alternatives while the worklist advances independently.
            return Some(sources.clone());
        }

        expression = *inputs.initializers.get(&expression)?;
    }

    None
}

pub(super) fn member_projection(
    unit: &BoundUnit,
    selections: &CheckedSemanticSelections,
    expression: BoundExpressionId,
) -> Option<DependencyProjection> {
    if let Some(SemanticSelection::Operation(SelectedOperation::Member(member))) =
        selections.expression(expression)
    {
        return match member.member() {
            AnySymbolId::StructField(field) => Some(DependencyProjection::ProductField(field)),
            AnySymbolId::UnionPayloadField(field) => {
                Some(DependencyProjection::UnionPayloadField(field))
            }
            _ => None,
        };
    }

    match unit.tree().expression(expression) {
        Some(BoundExpression::MemberAccess(member)) => match member.selector() {
            Some(bray_bound_tree::BoundMemberSelector::TupleElement(index)) => Some(
                DependencyProjection::TupleElement(SymbolOrdinal::new(*index)),
            ),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{AssignedValue, AssignmentInputs};
    use crate::dependency::ValueInputs;
    use crate::test_support::{
        checked_expression_types, error_type, expression_unit, push_expression,
        unselected_name_expression,
    };
    use bray_bound_tree::{
        BoundAssignmentExpression, BoundAssignmentOperator, BoundExpression, BoundUnitId,
        CheckedPatterns, CheckedSemanticSelections, ExpressionTypeResult, ExpressionTypeStatus,
    };
    use bray_symbols::{DependencyProjection, SymbolOrdinal};

    #[test]
    fn repeated_assignments_share_root_alternatives_across_reads() {
        for count in [64, 128, 256, 1024] {
            let (unit, expressions) = expression_unit(BoundUnitId::new(80), |tree, origin| {
                let mut expressions = Vec::new();

                for _ in 0..count {
                    let destination = push_expression(tree, unselected_name_expression(origin));
                    let value = push_expression(tree, unselected_name_expression(origin));

                    let assignment = push_expression(
                        tree,
                        BoundExpression::Assignment(BoundAssignmentExpression::new(
                            origin,
                            BoundAssignmentOperator::Assign,
                            [destination, value],
                            None,
                            false,
                        )),
                    );

                    expressions.extend([destination, value, assignment]);
                }

                expressions
            });

            let types = checked_expression_types(
                &unit,
                expressions.iter().copied(),
                ExpressionTypeResult::new(error_type(), ExpressionTypeStatus::Valid),
            );

            let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
                .expect("test selections must build");

            let patterns = CheckedPatterns::new(unit.unit(), unit.key().kind(), [], [], []);
            let inputs = ValueInputs::new(&unit, &selections, &patterns);
            let writes: &AssignmentInputs = &inputs.writes;

            assert_eq!(writes.by_root.len(), 1);
            assert_eq!(writes.by_root.values().map(Vec::len).sum::<usize>(), count);
            assert_eq!(writes.places.len(), count * 2);
            assert_eq!(writes.values(expressions[0]).count(), count);
            assert_eq!(writes.values(expressions[count * 3 - 2]).count(), count);
        }
    }

    #[test]
    fn assignment_projection_preserves_aggregate_fields_and_unknown_indexes() {
        let a = DependencyProjection::TupleElement(SymbolOrdinal::new(0));
        let b = DependencyProjection::TupleElement(SymbolOrdinal::new(1));

        let (_, expressions) = expression_unit(BoundUnitId::new(81), |tree, origin| {
            vec![push_expression(tree, unselected_name_expression(origin))]
        });

        let value = expressions[0];

        let cases = [
            (vec![Some(a)], vec![], vec![b], Some(vec![a, b])),
            (vec![], vec![Some(a)], vec![a, b], Some(vec![b])),
            (vec![], vec![Some(a)], vec![b], None),
            (vec![Some(a), None, Some(b)], vec![], vec![], Some(vec![a])),
            (vec![], vec![None], vec![b, a], Some(vec![a])),
            (vec![Some(a)], vec![Some(a), Some(b)], vec![b], Some(vec![])),
        ];

        for (read, destination, path, expected) in cases {
            let assigned = AssignedValue {
                value,
                read: &read,
                destination: &destination,
            };

            assert_eq!(assigned.project(&path).map(|(_, path)| path), expected);
        }
    }
}
