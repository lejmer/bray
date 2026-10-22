use std::collections::BTreeMap;

use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateOperation, StorageIdentityId};

use super::super::build::SourceTemplateBuilder;

pub(in crate::compilation::template) struct LoopBinding {
    pub(in crate::compilation::template) original: CheckedTemplateNodeId,
    pub(in crate::compilation::template) value: CheckedTemplateNodeId,
}

/// Owns lexical binder identities, while current values remain in the builder's single state map.
pub(in crate::compilation::template) struct LoopScope {
    pub(in crate::compilation::template) start: CheckedTemplateNodeId,
    pub(in crate::compilation::template) bindings: BTreeMap<StorageIdentityId, LoopBinding>,
}

impl LoopScope {
    pub(in crate::compilation::template) fn new(start: CheckedTemplateNodeId) -> Self {
        Self {
            start,
            bindings: BTreeMap::new(),
        }
    }
}

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(in crate::compilation::template) fn current_state(
        &mut self,
        root: StorageIdentityId,
    ) -> Option<CheckedTemplateNodeId> {
        let original = *self.state_values.get(&root)?;

        // Capture this before creating any outer binder. A first use inside a nested
        // loop must still bind a value that existed before both lexical boundaries.
        for ordinal in 0..self.loops.len() {
            let scope = &self.loops[ordinal];

            if original.raw() >= scope.start.raw() || scope.bindings.contains_key(&root) {
                continue;
            }

            let current = self.state_values[&root];

            let initial = if matches!(
                self.nodes[usize::try_from(current.raw()).expect("checked state fits host")]
                    .operation(),
                CheckedTemplateOperation::Input(_)
            ) {
                self.materialize(current)
            } else {
                current
            };

            let ty = self.nodes
                [usize::try_from(initial.raw()).expect("checked initial value fits host")]
            .ty();

            let value = self.push_node(CheckedTemplateOperation::LoopValue { initial }, ty);

            self.loops[ordinal].bindings.insert(
                root,
                LoopBinding {
                    original: current,
                    value,
                },
            );

            self.state_values.insert(root, value);
        }

        Some(self.state_values[&root])
    }
}
