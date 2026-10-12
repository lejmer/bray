use super::ExecutionCondition;
use crate::ExecutionPlace;
use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
use std::collections::BTreeSet;
use std::sync::Arc;

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
