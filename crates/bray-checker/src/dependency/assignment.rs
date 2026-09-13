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
    aliases: &BTreeMap<BoundExpressionId, BoundExpressionId>,
) -> BTreeMap<BoundExpressionId, Vec<AssignedValue>> {
    let mut reads = BTreeMap::<_, Vec<_>>::new();

    for (id, _) in unit.tree().expressions() {
        if let Some((root, path)) = value_place(unit, selections, aliases, id) {
            reads.entry(root).or_default().push((id, path));
        }
    }

    let mut inputs = BTreeMap::<_, Vec<_>>::new();

    for (_, expression) in unit.tree().expressions() {
        let BoundExpression::Assignment(assignment) = expression else {
            continue;
        };

        let [destination, value] = assignment.operands() else {
            continue;
        };

        let Some((root, destination)) = value_place(unit, selections, aliases, *destination) else {
            continue;
        };

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

            inputs.entry(*read).or_default().push(AssignedValue {
                value: *value,
                source: remaining,
                destination: destination.iter().skip(path.len()).copied().collect(),
            });
        }
    }

    inputs
}

pub(super) fn value_place(
    unit: &BoundUnit,
    selections: &CheckedSemanticSelections,
    aliases: &BTreeMap<BoundExpressionId, BoundExpressionId>,
    mut expression: BoundExpressionId,
) -> Option<(BoundReferenceTarget, Vec<Option<DependencyProjection>>)> {
    let mut path = Vec::new();
    let mut visited = BTreeSet::new();

    loop {
        if !visited.insert(expression) {
            return None;
        }

        if let Some(borrowed) = borrowed_initializer(unit, aliases, expression) {
            expression = borrowed;
            continue;
        }

        match unit.tree().expression(expression)? {
            BoundExpression::Name(name) => {
                path.reverse();

                return Some((name.target(), path));
            }
            BoundExpression::PatternReference(reference) => {
                path.reverse();

                return Some((
                    BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(reference.binding())),
                    path,
                ));
            }
            BoundExpression::MemberAccess(member) => {
                path.push(Some(member_projection(unit, selections, expression)?));
                expression = member.receiver();
            }
            BoundExpression::TraitQualifiedMember(member) => {
                path.push(Some(member_projection(unit, selections, expression)?));
                expression = member.receiver();
            }
            BoundExpression::Structured(value)
                if value.kind() == BoundStructuredExpressionKind::ElementIndex =>
            {
                path.push(None);
                expression = *value.operands().first()?;
            }
            _ => return None,
        }
    }
}

fn borrowed_initializer(
    unit: &BoundUnit,
    aliases: &BTreeMap<BoundExpressionId, BoundExpressionId>,
    mut expression: BoundExpressionId,
) -> Option<BoundExpressionId> {
    let mut visited = BTreeSet::new();

    while let Some(initializer) = aliases.get(&expression) {
        if !visited.insert(expression) {
            return None;
        }

        if let Some(BoundExpression::Structured(borrow)) = unit.tree().expression(*initializer)
            && borrow.kind() == BoundStructuredExpressionKind::Borrow
        {
            return borrow.operands().first().copied();
        }

        expression = *initializer;
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
