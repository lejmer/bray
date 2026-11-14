use std::collections::{BTreeMap, HashMap};

use super::super::ExecutionCondition;
use super::super::memo::ExecutionMemo;

#[derive(Clone, Copy)]
pub(super) enum MapRole {
    Value,
    Owner,
}

pub(super) struct RewriteMemo {
    pub(super) values: ExecutionMemo<ExecutionCondition>,
    pub(super) owners: ExecutionMemo<ExecutionCondition>,
}

impl RewriteMemo {
    fn new() -> Self {
        Self { values: ExecutionMemo::default(), owners: ExecutionMemo::default() }
    }
}

/// Reuse pure bounded rewriting across one forest and one immutable pair of maps.
/// Structural identity can be shared with the body, while results stay within these maps.
pub(crate) struct ExecutionSubstitution<'a> {
    values: &'a BTreeMap<ExecutionCondition, ExecutionCondition>,
    owners: &'a BTreeMap<ExecutionCondition, ExecutionCondition>,
    changed: bool,
    mapped: RewriteMemo,
}

impl<'a> ExecutionSubstitution<'a> {
    pub(crate) fn new(values: &'a BTreeMap<ExecutionCondition, ExecutionCondition>, owners: &'a BTreeMap<ExecutionCondition, ExecutionCondition>) -> Self {
        Self {
            values,
            owners,
            changed: ExecutionCondition::has_value_changes(values) || ExecutionCondition::has_value_changes(owners),
            mapped: RewriteMemo::new(),
        }
    }

    pub(crate) fn has_changes(&self) -> bool { self.changed }

    pub(crate) fn is_empty(&self) -> bool { self.values.is_empty() && self.owners.is_empty() }

    pub(crate) fn value(&self, value: &ExecutionCondition) -> ExecutionCondition {
        if !self.changed { return value.clone(); }

        super::rewriting::joined_value(value, self.values, self.owners, Some(&self.mapped))
    }

    pub(crate) fn owner(&self, value: &ExecutionCondition) -> ExecutionCondition {
        if !self.changed { return value.owner_identity().clone(); }

        super::rewriting::joined_owner(value, self.values, self.owners, Some(&self.mapped))
    }
}

/// Capture a forest against one immutable predecessor map without expanding inserted snapshots.
pub(crate) struct SnapshotSubstitution<'a> {
    snapshots: &'a BTreeMap<ExecutionCondition, ExecutionCondition>,
    changed: bool,
    mapped: ExecutionMemo<ExecutionCondition>,
}

impl<'a> SnapshotSubstitution<'a> {
    pub(crate) fn new(snapshots: &'a BTreeMap<ExecutionCondition, ExecutionCondition>) -> Self {
        Self { snapshots, changed: ExecutionCondition::has_value_changes(snapshots), mapped: ExecutionMemo::default() }
    }

    pub(crate) fn apply(&self, condition: &ExecutionCondition) -> ExecutionCondition {
        if !self.changed { return condition.clone(); }

        condition.substitute_values(&|value| super::rewriting::snapshot_value(value, self.snapshots),
            &mut { ExecutionCondition::WORK_LIMIT }, Some(&self.mapped))
    }
}

/// One immutable ordinary guard environment owns all of its bounded DAG rewrites.
pub(in crate::execution_guarantees::condition) struct GuardSubstitution {
    facts: HashMap<ExecutionCondition, ExecutionCondition>,
    mapped: ExecutionMemo<ExecutionCondition>,
    changed: bool,
}

impl GuardSubstitution {
    pub(in crate::execution_guarantees::condition) fn new<'a>(
        replacements: impl IntoIterator<Item = (&'a ExecutionCondition, ExecutionCondition)>,
    ) -> Self {
        let mut facts = HashMap::new();

        for (condition, replacement) in replacements { facts.insert(condition.clone(), replacement); }

        let changed = facts.iter().any(|(condition, replacement)| condition != replacement);

        Self { facts, mapped: ExecutionMemo::default(), changed }
    }

    pub(in crate::execution_guarantees::condition) fn apply(&self, condition: &ExecutionCondition) -> ExecutionCondition {
        if !self.changed { return condition.clone(); }

        condition.substitute_values(&|value| match value.kind() {
            crate::ExecutionConditionKind::Trusted(_) => Some(value.clone()),
            _ => self.facts.get(value).cloned(),
        }, &mut { ExecutionCondition::WORK_LIMIT }, Some(&self.mapped))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};
    use bray_symbols::SymbolOrdinal;

    use super::ExecutionSubstitution;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn snapshot_forests_preserve_capture_boundaries_and_each_original_budget() {
        let input = |index| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(index))));
        let owner = input(0);
        let inserted = ExecutionCondition::new(crate::ExecutionConditionKind::Borrowed(Arc::new(owner.clone())));
        let snapshots = BTreeMap::from([(owner.clone(), inserted.clone())]);
        let scope = super::SnapshotSubstitution::new(&snapshots);
        let captured = ExecutionCondition::new(crate::ExecutionConditionKind::Entry { condition: Arc::new(owner.clone()), captured: true });

        let mut condition = ExecutionCondition::new(crate::ExecutionConditionKind::Operation(LogicalOr,
            vec![ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(owner.clone()))), captured.clone()].into()));

        for _ in 0..8 {
            condition = ExecutionCondition::new(crate::ExecutionConditionKind::Operation(LogicalAnd, vec![condition.clone(), condition].into()));
        }

        let replace = |value: &ExecutionCondition| super::super::rewriting::snapshot_value(value, &snapshots);

        for incoming in (0..=ExecutionCondition::WORK_LIMIT).chain([1, 2, ExecutionCondition::WORK_LIMIT]) {
            let mut expected_budget = incoming;
            let mut actual_budget = incoming;
            let expected = condition.substitute_values(&replace, &mut expected_budget, None);
            let actual = condition.substitute_values(&replace, &mut actual_budget, Some(&scope.mapped));

            assert_eq!(actual, expected, "snapshot rewrite differs at budget {incoming}");
            assert_eq!(actual_budget, expected_budget);
        }

        for _ in 0..2 {
            assert_eq!(scope.apply(&owner), inserted, "inserted snapshots are not recursively expanded");
            assert_eq!(scope.apply(&captured), captured, "captured entry contents remain opaque");
        }

        let changed = BTreeMap::from([(owner.clone(), input(1))]);
        let other = super::SnapshotSubstitution::new(&changed);

        assert_eq!(other.apply(&owner), input(1));
        assert_eq!(scope.apply(&owner), inserted, "another map cannot replace this scope's answers");
    }

    #[test]
    fn guard_rewriting_preserves_each_budget_and_does_not_rewrite_qualified_atoms() {
        let input = |index| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(index))));
        let guard = input(0);
        let qualified = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(guard.clone())));
        let mut condition = ExecutionCondition::new(crate::ExecutionConditionKind::Operation(LogicalOr, vec![guard.clone(), qualified.clone()].into()));

        for _ in 0..8 {
            condition = ExecutionCondition::new(crate::ExecutionConditionKind::Operation(LogicalAnd, vec![condition.clone(), condition].into()));
        }

        let scope = super::GuardSubstitution::new([(&guard, ExecutionCondition::boolean(false))]);

        let replace = |value: &ExecutionCondition| match value.kind() {
            crate::ExecutionConditionKind::Trusted(_) => Some(value.clone()),
            _ => scope.facts.get(value).cloned(),
        };

        for incoming in (0..=ExecutionCondition::WORK_LIMIT).chain([1, 2, ExecutionCondition::WORK_LIMIT]) {
            let mut actual_budget = incoming;
            let mut expected_budget = incoming;
            let expected = condition.substitute_values(&replace, &mut expected_budget, None);
            let actual = condition.substitute_values(&replace, &mut actual_budget, Some(&scope.mapped));

            assert_eq!(actual, expected, "guard rewrite differs at budget {incoming}");
            assert_eq!(actual_budget, expected_budget);
        }

        assert_eq!(scope.apply(&qualified), qualified);

        let replaced = super::GuardSubstitution::new([(&guard, ExecutionCondition::boolean(true))]);

        assert_eq!(scope.apply(&guard), ExecutionCondition::boolean(false));
        assert_eq!(replaced.apply(&guard), ExecutionCondition::boolean(true));

        let identity = super::GuardSubstitution::new([(&guard, ExecutionCondition::boolean(false)), (&guard, guard.clone())]);

        assert_eq!(identity.apply(&condition), condition, "the final mapping determines whether rewriting is necessary");
    }

    #[test]
    fn scoped_rewriting_reuses_roots_without_crossing_owner_or_map_boundaries() {
        let input = |index| ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(index))));
        let values = BTreeMap::from([(input(0), input(4))]);
        let owners = BTreeMap::from([(input(0), input(2))]);
        let substitution = ExecutionSubstitution::new(&values, &owners);
        let claim = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(input(0))));

        assert_eq!(substitution.value(&input(0)), input(4));
        assert_eq!(substitution.owner(&input(0)), input(2));
        assert_eq!(substitution.value(&input(0)), input(4));

        let first = substitution.value(&claim);
        let second = substitution.value(&claim);

        let (crate::ExecutionConditionKind::Trusted(first), crate::ExecutionConditionKind::Trusted(second)) = (first.kind(), second.kind()) else {
            panic!("value substitution retains the occurrence qualifier");
        };

        assert_eq!(**first, input(4));
        assert!(Arc::ptr_eq(first, second), "the same root is rewritten once within its forest");

        let other_values = BTreeMap::from([(input(0), input(5))]);
        let other = ExecutionSubstitution::new(&other_values, &owners);

        assert_eq!(other.value(&input(0)), input(5));
        assert_eq!(other.owner(&input(0)), input(2));

        let mut repeated = claim;

        for index in 0..9 {
            repeated = ExecutionCondition::new(crate::ExecutionConditionKind::Operation(
                if index % 2 == 0 { LogicalAnd } else { LogicalOr },
                vec![repeated.clone(), repeated].into(),
            ));
        }

        let bounded = repeated.with_joined_values(&values, &owners);

        for _ in 0..2 {
            assert_eq!(substitution.value(&repeated), bounded, "reusing a root preserves the original bounded rewrite result");
        }
    }
}
