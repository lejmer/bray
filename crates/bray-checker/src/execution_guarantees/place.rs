use std::sync::Arc;

use bray_bound_tree::BoundReferenceTarget;
use bray_symbols::{AnySymbolId, ConstantProjectionKind};

/// An observable input or local place, including its selected component path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionPlace {
    pub(crate) root: ExecutionInput,
    pub(crate) projections: Arc<[ConstantProjectionKind]>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ExecutionInput {
    Reference(BoundReferenceTarget),
    Argument(bray_symbols::SymbolOrdinal),
}

impl From<BoundReferenceTarget> for ExecutionPlace {
    fn from(root: BoundReferenceTarget) -> Self {
        Self {
            root: ExecutionInput::Reference(root),
            projections: Arc::new([]),
        }
    }
}

impl ExecutionPlace {
    pub(crate) fn argument(ordinal: bray_symbols::SymbolOrdinal) -> Self {
        Self {
            root: ExecutionInput::Argument(ordinal),
            projections: Arc::new([]),
        }
    }

    pub(crate) const fn reference(&self) -> Option<BoundReferenceTarget> {
        match self.root {
            ExecutionInput::Reference(reference) => Some(reference),
            ExecutionInput::Argument(_) => None,
        }
    }
    pub(crate) fn storage(
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Self> {
        Self::storage_owner(storage, access)?.project(storage.resolved_projections(access)?)
    }

    pub(crate) fn storage_owner(
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Self> {
        let identity = storage.root_identity(access)?;

        let root = storage.bindings().iter().find_map(|(target, binding)| {
            (*binding == bray_bound_tree::StorageBinding::Identity(identity))
                .then(|| storage_binding_reference(*target))
                .flatten()
        })?;

        Some(Self::from(root))
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
                bray_bound_tree::StorageProjection::TupleElement(index) => {
                    self = self.component(ConstantProjectionKind::TupleElement(*index));
                }
                bray_bound_tree::StorageProjection::ElementFromStart(index) => {
                    self = self.component(ConstantProjectionKind::ArrayElementOrdinal(*index));
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
        for length in (0..=self.projections.len()).rev() {
            let prefix = Self {
                root: self.root,
                projections: self.projections[..length].into(),
            };

            if let Some(value) = values.get(&prefix) {
                // Component projections retain the same immutable snapshot as their containing value.
                let mut value = value.clone();

                for projection in self.projections.iter().skip(length) {
                    value = value.component(*projection);
                }

                return Some(value);
            }
        }

        None
    }

    pub(crate) fn field(self, field: AnySymbolId) -> Self {
        let projection = match field {
            AnySymbolId::StructField(field) => ConstantProjectionKind::ProductField(field),
            AnySymbolId::UnionPayloadField(field) => {
                ConstantProjectionKind::UnionPayloadField(field)
            }
            _ => panic!(
                "checked execution field must identify a product or union payload field: {field:?}"
            ),
        };

        self.component(projection)
    }

    pub(crate) fn component(mut self, projection: ConstantProjectionKind) -> Self {
        self.projections = self
            .projections
            .iter()
            .copied()
            .chain([projection])
            .collect();

        self
    }

    pub(crate) fn contains(&self, other: &Self) -> bool {
        self.root == other.root && other.projections.starts_with(&self.projections)
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
