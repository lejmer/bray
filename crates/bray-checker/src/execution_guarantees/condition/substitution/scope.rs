use std::collections::BTreeMap;

use super::super::ExecutionCondition;
use super::super::memo::ExecutionMemo;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum MapRole {
    Value,
    Owner,
}

pub(super) type RewriteMemo = ExecutionMemo<(MapRole, ExecutionCondition), ExecutionCondition>;

/// Reuse pure bounded rewriting across one forest and one immutable pair of maps.
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
            mapped: RewriteMemo::default(),
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};
    use bray_symbols::SymbolOrdinal;

    use super::ExecutionSubstitution;
    use crate::{ExecutionCondition, ExecutionPlace};

    #[test]
    fn scoped_rewriting_reuses_roots_without_crossing_owner_or_map_boundaries() {
        let input = |index| ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(index)));
        let values = BTreeMap::from([(input(0), input(4))]);
        let owners = BTreeMap::from([(input(0), input(2))]);
        let substitution = ExecutionSubstitution::new(&values, &owners);
        let claim = ExecutionCondition::Trusted(Arc::new(input(0)));

        assert_eq!(substitution.value(&input(0)), input(4));
        assert_eq!(substitution.owner(&input(0)), input(2));
        assert_eq!(substitution.value(&input(0)), input(4));

        let first = substitution.value(&claim);
        let second = substitution.value(&claim);
        let (ExecutionCondition::Trusted(first), ExecutionCondition::Trusted(second)) = (&first, &second) else {
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
            repeated = ExecutionCondition::Operation(
                if index % 2 == 0 { LogicalAnd } else { LogicalOr },
                vec![repeated.clone(), repeated].into(),
            );
        }

        let bounded = repeated.with_joined_values(&values, &owners);
        for _ in 0..2 {
            assert_eq!(substitution.value(&repeated), bounded, "reusing a root preserves the original bounded rewrite result");
        }
    }
}
