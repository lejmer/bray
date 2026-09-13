use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnit, CheckedSemanticSelections, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, AnySymbolId, DependencyProjection, SymbolOrdinal};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct AssignedValue {
    pub(super) value: BoundExpressionId,
    pub(super) source: Vec<DependencyProjection>,
    destination: Vec<Option<DependencyProjection>>,
}

impl AssignedValue {
    pub(super) fn project(
        &self,
        path: &[DependencyProjection],
    ) -> Option<(BoundExpressionId, Vec<DependencyProjection>)> {
        if self
            .destination
            .iter()
            .zip(path)
            .any(|(left, right)| left.is_some_and(|left| left != *right))
        {
            return None;
        }

        Some((
            self.value,
            self.source
                .iter()
                .copied()
                .chain(path.iter().skip(self.destination.len()).copied())
                .collect(),
        ))
    }
}

/// Relates each read to assignments that can contribute to its value.
pub(super) fn assignment_inputs(
    unit: &BoundUnit,
    selections: &CheckedSemanticSelections,
    inputs: &super::ValueInputs,
) -> BTreeMap<BoundExpressionId, Vec<AssignedValue>> {
    let mut reads = BTreeMap::<_, Vec<_>>::new();

    for (id, _) in unit.tree().expressions() {
        for (root, path) in value_places(unit, selections, inputs, id) {
            reads.entry(root).or_default().push((id, path));
        }
    }

    let mut assignments = BTreeMap::<_, Vec<_>>::new();

    for (_, expression) in unit.tree().expressions() {
        let BoundExpression::Assignment(assignment) = expression else {
            continue;
        };

        let [destination, value] = assignment.operands() else {
            continue;
        };

        for (root, destination) in value_places(unit, selections, inputs, *destination) {
            for (read, path) in reads.get(&root).into_iter().flatten() {
                if destination
                    .iter()
                    .zip(path)
                    .any(|(left, right)| left.is_some() && right.is_some() && left != right)
                {
                    continue;
                }

                // Unknown indexes retain the assigned aggregate's complete value contract.
                let remaining = path
                    .iter()
                    .skip(destination.len())
                    .copied()
                    .map_while(|projection| projection)
                    .collect();

                assignments.entry(*read).or_default().push(AssignedValue {
                    value: *value,
                    source: remaining,
                    destination: destination.iter().skip(path.len()).copied().collect(),
                });
            }
        }
    }

    assignments
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
