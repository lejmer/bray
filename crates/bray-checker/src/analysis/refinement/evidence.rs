use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundPatternId, BoundPatternKind,
    BoundStructuredExpressionKind, BoundUnitView, CheckedPatterns, PatternPredicate,
    PatternRefutability, Refinement, RefinementKind, StorageAccessId, StorageBinding,
    StorageBindingTarget, StoragePlan, StorageRelationship,
};

pub(super) fn expression_dependencies(
    view: BoundUnitView<'_>,
    direct: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    expression: BoundExpressionId,
) -> BTreeSet<StorageAccessId> {
    let mut dependencies = BTreeSet::new();
    let mut pending = vec![expression];
    let mut visited = BTreeSet::new();

    while let Some(expression) = pending.pop() {
        if !visited.insert(expression) {
            continue;
        }

        dependencies.extend(direct.get(&expression).into_iter().flatten().copied());

        if let Some(expression) = view.expression(expression) {
            pending.extend(expression.child_expressions());
        }
    }

    dependencies
}

pub(super) fn condition_refinements(
    view: BoundUnitView<'_>,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    dependencies: &BTreeMap<BoundExpressionId, BTreeSet<StorageAccessId>>,
    expression: BoundExpressionId,
    value: bool,
) -> Vec<Refinement> {
    let mut result = vec![Refinement::new(
        RefinementKind::Condition { expression, value },
        expression_dependencies(view, dependencies, expression),
    )];

    if let Some(BoundExpression::Structured(test)) = view.expression(expression)
        && matches!(
            test.kind(),
            BoundStructuredExpressionKind::PatternTest
                | BoundStructuredExpressionKind::PatternBinding
        )
        && let ([subject], [pattern]) = (test.operands(), test.patterns())
    {
        result.extend(pattern_refinements(
            view, patterns, storage, *subject, *pattern, value,
        ));
    }

    result
}

pub(super) fn pattern_refinements(
    view: BoundUnitView<'_>,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    subject: BoundExpressionId,
    root: BoundPatternId,
    value: bool,
) -> Vec<Refinement> {
    let Some(bound) = view.pattern(root) else {
        return Vec::new();
    };

    let Some(checked) = patterns
        .pattern(root)
        .filter(|checked| !checked.is_recovered())
    else {
        return Vec::new();
    };

    if bound.kind() == BoundPatternKind::Alternative {
        let mut children = bound.children().iter().copied();

        let Some(first) = children.next() else {
            return Vec::new();
        };

        let mut result = pattern_refinements(view, patterns, storage, subject, first, value);

        for child in children {
            let child = pattern_refinements(view, patterns, storage, subject, child, value);

            if value {
                retain_common(&mut result, &child, storage);
            } else {
                result.extend(child);
            }
        }

        return result;
    }

    let predicate = checked.refinement();

    let structural_test_can_fail = predicate.is_some_and(|predicate| {
        !matches!(
            predicate,
            PatternPredicate::ProductShape(_)
                | PatternPredicate::TupleShape(_)
                | PatternPredicate::OwnedTarget
        )
    });

    let refutable_children = bound
        .children()
        .iter()
        .copied()
        .filter(|child| {
            patterns
                .pattern(*child)
                .is_some_and(|entry| entry.refutability() != PatternRefutability::Irrefutable)
        })
        .collect::<Vec<_>>();

    if !value {
        match (structural_test_can_fail, refutable_children.as_slice()) {
            (false, [child]) => {
                return pattern_refinements(view, patterns, storage, subject, *child, false);
            }
            (true, []) => {}
            _ => return Vec::new(),
        }
    }

    let mut result = Vec::new();

    if let Some(predicate) = predicate
        && let Some(StorageBinding::Access(access)) =
            storage.binding(StorageBindingTarget::PatternSubject(root))
    {
        let (predicate, value) = match (predicate, value) {
            (PatternPredicate::NullableAbsent, false) => (PatternPredicate::NullablePresent, true),
            (PatternPredicate::NullablePresent, false) => (PatternPredicate::NullableAbsent, true),
            pair => pair,
        };

        result.push(Refinement::new(
            RefinementKind::Pattern {
                subject,
                pattern: root,
                predicate,
                access,
                value,
            },
            [access],
        ));
    }

    if value {
        for child in bound.children() {
            result.extend(pattern_refinements(
                view, patterns, storage, subject, *child, true,
            ));
        }
    }

    result
}

fn retain_common(left: &mut Vec<Refinement>, right: &[Refinement], storage: &StoragePlan) {
    left.retain(|left| {
        right.iter().any(|right| match (left.kind(), right.kind()) {
            (
                RefinementKind::Pattern {
                    access: left,
                    predicate,
                    value,
                    ..
                },
                RefinementKind::Pattern {
                    access: right,
                    predicate: other,
                    value: other_value,
                    ..
                },
            ) => {
                predicate == other
                    && value == other_value
                    && storage.relationship(left, right) == StorageRelationship::Identical
            }
            (left, right) => left == right,
        })
    });
}

pub(super) fn equivalent_pattern_accesses(
    storage: &StoragePlan,
) -> BTreeMap<StorageAccessId, StorageAccessId> {
    let mut groups = BTreeMap::<_, Vec<StorageAccessId>>::new();
    let mut result = BTreeMap::new();

    for (target, binding) in storage.bindings() {
        let (StorageBindingTarget::PatternSubject(_), StorageBinding::Access(access)) =
            (target, binding)
        else {
            continue;
        };

        let (Some(root), Some(path)) = (
            storage.root_identity(*access),
            storage.resolved_projections(*access),
        ) else {
            continue;
        };

        let candidates = groups.entry((root, path.to_vec())).or_default();

        let representative = candidates
            .iter()
            .copied()
            .find(|candidate| {
                storage.relationship(*candidate, *access) == StorageRelationship::Identical
            })
            .unwrap_or_else(|| {
                candidates.push(*access);

                *access
            });

        result.insert(*access, representative);
    }

    result
}
