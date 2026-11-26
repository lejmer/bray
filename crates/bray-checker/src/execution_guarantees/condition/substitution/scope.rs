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
    pub(super) rebind_frames: bool,
}

impl RewriteMemo {
    fn new() -> Self {
        Self { values: ExecutionMemo::default(), owners: ExecutionMemo::default(), rebind_frames: false }
    }

    /// An empty memo with the same rewriting policy.
    pub(super) fn fresh(&self) -> Self {
        Self { rebind_frames: self.rebind_frames, ..Self::new() }
    }
}

/// Immutable replacement entries retain whether any mapping changes its input,
/// and the root bits a condition needs to contain one of those changing inputs.
#[derive(Clone, Copy)]
pub(super) struct RewriteMap<'a> {
    pub(super) entries: &'a BTreeMap<ExecutionCondition, ExecutionCondition>,
    pub(super) changed: bool,
    pub(super) roots: u64,
}

impl<'a> RewriteMap<'a> {
    pub(super) fn new(entries: &'a BTreeMap<ExecutionCondition, ExecutionCondition>) -> Self {
        let roots = changing_roots(entries.iter());

        Self { entries, changed: roots != 0, roots }
    }
}

/// An identity entry only stops descent into an unchanged value. A condition
/// without the root bits of any changing input therefore rewrites to itself.
/// A changing input without root bits can occur anywhere and admits every condition.
pub(super) fn changing_roots<'a>(entries: impl IntoIterator<Item = (&'a ExecutionCondition, &'a ExecutionCondition)>) -> u64 {
    entries.into_iter().filter(|(before, after)| before != after)
        .fold(0, |roots, (before, _)| if before.roots() == 0 { u64::MAX } else { roots | before.roots() })
}

/// Reuse pure bounded rewriting across one forest and one immutable pair of maps.
/// Structural identity can be shared with the body, while results stay within these maps.
pub(crate) struct ExecutionSubstitution<'a> {
    values: RewriteMap<'a>,
    owners: RewriteMap<'a>,
    mapped: RewriteMemo,
}

impl<'a> ExecutionSubstitution<'a> {
    pub(crate) fn new(values: &'a BTreeMap<ExecutionCondition, ExecutionCondition>, owners: &'a BTreeMap<ExecutionCondition, ExecutionCondition>) -> Self {
        Self {
            values: RewriteMap::new(values),
            owners: RewriteMap::new(owners),
            mapped: RewriteMemo::new(),
        }
    }

    pub(crate) fn frame_inputs(values: &'a BTreeMap<ExecutionCondition, ExecutionCondition>, owners: &'a BTreeMap<ExecutionCondition, ExecutionCondition>) -> Self {
        let mut substitution = Self::new(values, owners);

        substitution.mapped.rebind_frames = true;

        substitution
    }

    pub(crate) fn has_changes(&self) -> bool { self.values.changed || self.owners.changed }

    pub(crate) fn is_empty(&self) -> bool { self.values.entries.is_empty() && self.owners.entries.is_empty() }

    pub(crate) fn value(&self, value: &ExecutionCondition) -> ExecutionCondition {
        super::rewriting::joined_value(value, self.values, self.owners, Some(&self.mapped))
    }

    pub(crate) fn owner(&self, value: &ExecutionCondition) -> ExecutionCondition {
        super::rewriting::joined_owner(value, self.values, self.owners, Some(&self.mapped))
    }
}

/// Capture a forest against one immutable predecessor map without expanding inserted snapshots.
pub(crate) struct SnapshotSubstitution<'a> {
    snapshots: RewriteMap<'a>,
    mapped: ExecutionMemo<ExecutionCondition>,
}

impl<'a> SnapshotSubstitution<'a> {
    pub(crate) fn new(snapshots: &'a BTreeMap<ExecutionCondition, ExecutionCondition>) -> Self {
        Self { snapshots: RewriteMap::new(snapshots), mapped: ExecutionMemo::default() }
    }

    pub(crate) fn apply(&self, condition: &ExecutionCondition) -> ExecutionCondition {
        if !self.snapshots.changed { return condition.clone(); }

        let replace = |value: &ExecutionCondition| super::rewriting::snapshot_value(value, self.snapshots.entries);

        filtered_substitution(condition, &replace, self.snapshots.roots, &self.mapped)
    }
}

/// Filters a bounded substitution only when it cannot exhaust its budget. The
/// filtered rewrite consumes less work, so it keeps that cost out of the shared memo.
fn filtered_substitution(
    condition: &ExecutionCondition,
    replace: &impl Fn(&ExecutionCondition) -> Option<ExecutionCondition>,
    roots: u64,
    mapped: &ExecutionMemo<ExecutionCondition>,
) -> ExecutionCondition {
    let Some(roots) = condition.filter_roots(roots) else {
        return condition.substitute_values(replace, u64::MAX, &mut { ExecutionCondition::WORK_LIMIT }, Some(mapped));
    };

    let rewritten = condition.substitute_values(replace, roots, &mut { ExecutionCondition::WORK_LIMIT }, Some(&ExecutionMemo::default()));

    debug_assert_eq!(rewritten, condition.substitute_values(replace, u64::MAX, &mut { ExecutionCondition::WORK_LIMIT }, Some(mapped)),
        "root filtering cannot change a substitution of {condition:?}");

    rewritten
}

/// One immutable ordinary guard environment owns all of its bounded DAG rewrites.
pub(crate) struct GuardSubstitution {
    facts: HashMap<ExecutionCondition, ExecutionCondition>,
    mapped: ExecutionMemo<ExecutionCondition>,
    roots: u64,
}

impl GuardSubstitution {
    pub(crate) fn new<'a>(
        replacements: impl IntoIterator<Item = (&'a ExecutionCondition, ExecutionCondition)>,
    ) -> Self {
        let mut facts = HashMap::new();

        for (condition, replacement) in replacements { facts.insert(condition.clone(), replacement); }

        let roots = changing_roots(facts.iter());

        Self { facts, mapped: ExecutionMemo::default(), roots }
    }

    pub(crate) fn apply(&self, condition: &ExecutionCondition) -> ExecutionCondition {
        if self.roots == 0 { return condition.clone(); }

        let replace = |value: &ExecutionCondition| match value.kind() {
            crate::ExecutionConditionKind::Trusted(_) => Some(value.clone()),
            _ => self.facts.get(value).cloned(),
        };

        filtered_substitution(condition, &replace, self.roots, &self.mapped)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};
    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};

    use super::ExecutionSubstitution;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn predecessor_projection_reads_the_captured_payload() {
        let result = ExecutionCondition::new(crate::ExecutionConditionKind::Result);

        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            ExecutionPlace::argument(SymbolOrdinal::new(0))));

        let payload = ExecutionCondition::new(crate::ExecutionConditionKind::Input(
            ExecutionPlace::argument(SymbolOrdinal::new(1))));

        let nullable = result.clone().component(ConstantProjectionKind::NullableValue);
        let selected = nullable.clone().component(ConstantProjectionKind::NullableValue);

        let source = ExecutionCondition::constructed(owner, [
            (bray_bound_tree::StorageProjection::NullableValue, payload.clone()),
        ].into());

        let snapshots = BTreeMap::from([(nullable, source)]);

        let substitution = super::SnapshotSubstitution::new(&snapshots);

        assert_eq!(substitution.apply(&selected).into_component_data(), payload);
        assert_eq!(substitution.apply(&selected.borrowed()), payload.borrowed());
    }

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
            let expected = condition.substitute_values(&replace, u64::MAX, &mut expected_budget, None);
            let actual = condition.substitute_values(&replace, u64::MAX, &mut actual_budget, Some(&scope.mapped));

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
            let expected = condition.substitute_values(&replace, u64::MAX, &mut expected_budget, None);
            let actual = condition.substitute_values(&replace, u64::MAX, &mut actual_budget, Some(&scope.mapped));

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

    #[test]
    fn rewriting_an_aggregate_base_retains_unchanged_fields() {
        use crate::ExecutionConditionKind::{Constructed, Input};

        let base = ExecutionCondition::new(Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let renamed = ExecutionCondition::new(Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let projection = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let source_field = base.clone().project(projection);
        let reached_field = renamed.clone().project(projection);
        let original = ExecutionCondition::constructed(base.clone(), [(projection, ExecutionCondition::boolean(true))].into());
        let values = BTreeMap::from([(base, renamed.clone()), (reached_field.clone(), ExecutionCondition::boolean(false))]);
        let owners = BTreeMap::new();
        let substitution = ExecutionSubstitution::new(&values, &owners);
        let rewritten = substitution.value(&original);

        let (Constructed(_, original_fields), Constructed(actual, rewritten_fields)) = (original.kind(), rewritten.kind()) else {
            panic!("renaming the base preserves the explicit tuple field");
        };

        assert_eq!(actual.as_ref(), &renamed);
        assert!(Arc::ptr_eq(original_fields, rewritten_fields));
        assert_eq!(substitution.value(&source_field), reached_field, "a reached field is not a second input to the join");
    }
}
