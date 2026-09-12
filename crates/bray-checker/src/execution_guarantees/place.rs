use std::sync::Arc;

use bray_bound_tree::BoundReferenceTarget;
use bray_symbols::AnySymbolId;

/// An observable input or local place, including its selected field path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionPlace {
    pub(crate) root: BoundReferenceTarget,
    pub(crate) fields: Arc<[AnySymbolId]>,
}

impl From<BoundReferenceTarget> for ExecutionPlace {
    fn from(root: BoundReferenceTarget) -> Self {
        Self {
            root,
            fields: Arc::new([]),
        }
    }
}

impl ExecutionPlace {
    pub(crate) fn value_in(
        &self,
        values: &std::collections::BTreeMap<Self, super::ExecutionCondition>,
    ) -> Option<super::ExecutionCondition> {
        for length in (0..=self.fields.len()).rev() {
            let prefix = Self {
                root: self.root,
                fields: self.fields[..length].into(),
            };

            if let Some(value) = values.get(&prefix) {
                // Field projections retain the same immutable snapshot as their containing value.
                let mut value = value.clone();

                for field in self.fields.iter().skip(length) {
                    value = super::ExecutionCondition::field(*field, value);
                }

                return Some(value);
            }
        }

        None
    }

    pub(crate) fn field(mut self, field: AnySymbolId) -> Self {
        self.fields = self.fields.iter().copied().chain([field]).collect();

        self
    }

    pub(crate) fn contains(&self, other: &Self) -> bool {
        self.root == other.root && other.fields.starts_with(&self.fields)
    }

    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }
}
