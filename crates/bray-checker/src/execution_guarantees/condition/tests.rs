use super::ExecutionCondition;
use crate::ExecutionPlace;
use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn changed_contents_do_not_reuse_unlisted_components_from_carrier_identity() {
    let identity = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let contents = ExecutionCondition::Result;
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let marker = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

    let value = ExecutionCondition::Constructed(
        Arc::new(identity.clone()),
        [
            (bray_bound_tree::StorageProjection::OwnedTarget, contents.clone()),
            (bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1)), marker.clone()),
        ].into(),
    );

    assert_eq!(value.owner_identity(), &identity);
    assert_eq!(value.clone().component(first), contents.clone().component(first));
    assert_eq!(value.clone().component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))), marker);
    assert_eq!(value.component(ConstantProjectionKind::NullableValue).component(first), contents.component(ConstantProjectionKind::NullableValue).component(first));
}

#[test]
fn partial_aggregate_snapshots_keep_unlisted_component_identity() {
    let root = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let first = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
    let second = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));

    let value = ExecutionCondition::Constructed(
        Arc::new(root.clone()),
        [(first, ExecutionCondition::Unknown)].into(),
    );

    assert_eq!(value.clone().project(first), ExecutionCondition::Unknown);
    assert_eq!(value.component(second), root.component(second));
}

#[test]
fn qualified_unknown_conditions_cannot_be_assumed_or_proven() {
    for condition in [
        ExecutionCondition::Unknown,
        ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Unknown)),
        ExecutionCondition::Entry {
            condition: Arc::new(ExecutionCondition::Unknown),
            captured: true,
        },
    ] {
        let mut facts = BTreeSet::new();

        condition.clone().assume(true, &mut facts);
        assert!(facts.is_empty());

        // Even a supplied set containing a recovery term cannot turn it into evidence.
        facts.insert((condition.clone(), true));
        assert_eq!(condition.prove_trusted(&facts, &BTreeSet::new()), None);

        assert_eq!(
            condition.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }),
            None
        );
    }
}

#[test]
fn observation_paths_distinguish_contents_from_containing_identity() {
    for root in [
        ExecutionCondition::Result,
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))),
    ] {
        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let child = left
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let mut changed = super::ObservationSet::default();

        changed.insert(left.clone());
        assert!(changed.overlaps(&root));
        assert!(changed.overlaps(&left));
        assert!(changed.overlaps(&child));
        assert!(!changed.overlaps(&right));
        assert!(!changed.contains(&root));
        assert!(changed.contains(&child));
        assert!(!right.clone().borrowed().observes(&left));
        assert!(root.clone().borrowed().observes(&left));
        assert!(left.clone().borrowed().observes(&root));

        assert!(
            !ExecutionCondition::Entry {
                condition: Arc::new(left),
                captured: true
            }
            .observes(&root)
        );
    }
}

#[test]
fn boolean_proofs_keep_decisive_operands_without_equating_unknowns() {
    use bray_bound_tree::BoundOperator;

    for left in [None, Some(false), Some(true)] {
        for right in [None, Some(false), Some(true)] {
            for operator in [BoundOperator::LogicalAnd, BoundOperator::LogicalOr] {
                let condition = ExecutionCondition::operation(operator, vec![
                    left.map_or(ExecutionCondition::Unknown, ExecutionCondition::Boolean),
                    right.map_or(ExecutionCondition::Unknown, ExecutionCondition::Boolean),
                ]);

                let decisive = operator == BoundOperator::LogicalOr;

                let expected = if left == Some(decisive) || right == Some(decisive) {
                    Some(decisive)
                } else if left.is_some() && right.is_some() {
                    Some(!decisive)
                } else { None };

                let facts = BTreeSet::new();

                assert_eq!(condition.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }), expected);
                assert_eq!(condition.prove_mixed(&facts, &facts, &mut { ExecutionCondition::WORK_LIMIT }), expected);
            }
        }
    }

    let known = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));

    for (operator, truth) in [(BoundOperator::LogicalOr, true), (BoundOperator::LogicalAnd, false)] {
        let condition = ExecutionCondition::operation(operator, vec![ExecutionCondition::Unknown, known.clone()]);
        let mut facts = BTreeSet::new();

        condition.clone().assume(truth, &mut facts);
        assert!(facts.is_empty());
        facts.insert((condition.clone(), truth));
        assert_eq!(condition.prove(&facts, &mut { ExecutionCondition::WORK_LIMIT }), None);
        assert_eq!(condition.prove_mixed(&facts, &facts, &mut { ExecutionCondition::WORK_LIMIT }), None);
    }
}

#[test]
fn repeated_component_observation_keeps_the_original_snapshot() {
    let original = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
    let first = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
    let nested = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1));
    let value = original.clone().component(first).component(nested);

    assert_eq!(original.with_component(&[first, nested], value), original);

    let changed = original.with_component(&[first, nested], ExecutionCondition::Boolean(true));

    assert_ne!(changed, original);
    assert_eq!(changed.clone().component(first).component(nested), ExecutionCondition::Boolean(true));
}
