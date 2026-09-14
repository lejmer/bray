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
    pub(crate) fn storage(
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Self> {
        let identity = storage.root_identity(access)?;

        let root = storage.bindings().iter().find_map(|(target, binding)| {
            (*binding == bray_bound_tree::StorageBinding::Identity(identity))
                .then(|| storage_binding_reference(*target))
                .flatten()
        })?;

        Self::from(root).project(storage.resolved_projections(access)?)
    }

    pub(crate) fn project(
        mut self,
        projections: &[bray_bound_tree::StorageProjection],
    ) -> Option<Self> {
        for projection in projections {
            match projection {
                bray_bound_tree::StorageProjection::ProductField(field) => {
                    self = self.field((*field).into());
                }
                bray_bound_tree::StorageProjection::ActiveUnionPayloadField { field, .. } => {
                    self = self.field((*field).into());
                }
                _ => return None,
            }
        }

        Some(self)
    }

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

pub(crate) fn storage_binding_reference(
    target: bray_bound_tree::StorageBindingTarget,
) -> Option<BoundReferenceTarget> {
    match target {
        bray_bound_tree::StorageBindingTarget::Local(id) => {
            Some(BoundReferenceTarget::Local(id.into()))
        }
        bray_bound_tree::StorageBindingTarget::Parameter(id) => {
            Some(BoundReferenceTarget::Surface(id.into()))
        }
        bray_bound_tree::StorageBindingTarget::Receiver(id) => {
            Some(BoundReferenceTarget::Surface(id.into()))
        }
        bray_bound_tree::StorageBindingTarget::AnonymousParameter(id) => {
            Some(BoundReferenceTarget::Local(id.into()))
        }
        _ => None,
    }
}
