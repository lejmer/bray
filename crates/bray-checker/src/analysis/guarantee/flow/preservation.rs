use std::collections::BTreeMap;

use bray_bound_tree::{StorageMutation, StorageProjection};

use crate::{ExecutionCondition, ExecutionPlace};

/// The checked physical-survival mask for one actual storage value. It holds
/// no object data or evidence and is applied to the retained source once.
#[derive(Default)]
pub(super) struct ValuePreservation {
    pub(super) value: bool,
    pub(super) descriptor: bool,
    children: BTreeMap<StorageProjection, Self>,
}

impl ValuePreservation {
    pub(super) fn at_mut(&mut self, path: &[StorageProjection]) -> &mut Self {
        path.iter().fold(self, |node, projection| node.children.entry(*projection).or_default())
    }

    pub(super) fn at(&self, path: &[StorageProjection]) -> Option<&Self> {
        let mut node = self;

        for projection in path {
            if node.value { return Some(node); }
            node = node.children.get(projection)?;
        }

        Some(node)
    }

    /// A mutation produces unknown new contents and carries only actual source
    /// nodes selected by physical survival. It never recovers data from witnesses,
    /// source syntax, another slot or a marker constructed after the operation.
    pub(super) fn apply(
        &self,
        source: &ExecutionCondition,
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
        wrap: &impl Fn(&ExecutionPlace, ExecutionCondition, ExecutionCondition) -> ExecutionCondition,
    ) -> ExecutionCondition {
        self.apply_bounded(source, place, mutation, wrap, &mut { ExecutionCondition::WORK_LIMIT })
    }

    fn apply_bounded(
        &self,
        source: &ExecutionCondition,
        place: Option<&ExecutionPlace>,
        mutation: StorageMutation,
        wrap: &impl Fn(&ExecutionPlace, ExecutionCondition, ExecutionCondition) -> ExecutionCondition,
        budget: &mut usize,
    ) -> ExecutionCondition {
        let Some(remaining) = budget.checked_sub(1) else {
            return ExecutionCondition::Unknown;
        };

        *budget = remaining;

        if self.value { return source.clone(); }

        let mut contents = place.map_or(ExecutionCondition::Unknown, |place| {
            ExecutionCondition::PostState(mutation, place.clone())
        });

        for (projection, preservation) in &self.children {
            let child = source.clone().project(*projection);
            let child_place = place.and_then(|place| place.clone().project(&[*projection]));
            let next = preservation.apply_bounded(&child, child_place.as_ref(), mutation, wrap, budget);

            contents = contents.with_storage_component(&[*projection], next);
        }

        if self.descriptor {
            if let Some(place) = place {
                return wrap(place, source.owner_identity().clone(), contents);
            }
        }

        contents
    }
}

#[cfg(test)]
mod tests {
    use super::ValuePreservation;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, BoundUnitId, StorageMutation, StorageProjection};
    use bray_symbols::SymbolOrdinal;

    #[test]
    fn repeated_effects_keep_actual_nested_owners_and_only_unchanged_data() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let marker = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
        let child = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let progress = StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let mut value = ExecutionCondition::constructed(ExecutionCondition::Input(root.clone()), [
            (child, ExecutionCondition::constructed(owner.clone(), [(child, marker.clone())].into())),
            (progress, ExecutionCondition::Boolean(true)),
        ].into());
        let mut mask = ValuePreservation::default();

        mask.descriptor = true;
        mask.at_mut(&[child]).descriptor = true;
        mask.at_mut(&[child, child]).value = true;

        for index in [0, 1] {
            let site = BoundExpressionId::from_slot(BoundUnitId::new(0), index);

            value = mask.apply(&value, Some(&root), StorageMutation::Operation(BoundExecutionSite::from(site)),
                &|_, owner, contents| ExecutionCondition::owned_value(owner, contents));
            let retained = value.clone().project(child);

            assert_eq!(retained.owner_identity(), &owner);
            assert_eq!(retained.project(child), marker);
            assert_ne!(value.clone().project(progress), ExecutionCondition::Boolean(true));
        }
    }
}
