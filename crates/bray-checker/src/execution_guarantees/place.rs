use std::sync::Arc;

use bray_bound_tree::BoundReferenceTarget;
use bray_symbols::{AnySymbolId, ConstantProjectionKind};

/// An observable input or local place, including its selected component path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionPlace {
    pub(crate) root: ExecutionInput,
    pub(crate) projections: Arc<[ConstantProjectionKind]>,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ExecutionInput {
    Reference(BoundReferenceTarget),
    Static(Arc<bray_symbols::StaticReferenceSelection>),
    Argument(bray_symbols::SymbolOrdinal),
    Storage(bray_bound_tree::StorageIdentityId),
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
    /// Identifies one selected static instance, including its substitution and witnesses.
    pub fn static_instance(selection: bray_symbols::StaticReferenceSelection) -> Self {
        Self {
            root: ExecutionInput::Static(Arc::new(selection)),
            projections: Arc::new([]),
        }
    }

    pub(crate) fn selected_static(&self) -> Option<bray_symbols::StaticSymbolId> {
        match &self.root {
            ExecutionInput::Static(selection) => Some(selection.template().declaration()),
            _ => None,
        }
    }

    pub(crate) fn argument(ordinal: bray_symbols::SymbolOrdinal) -> Self {
        Self {
            root: ExecutionInput::Argument(ordinal),
            projections: Arc::new([]),
        }
    }

    pub(crate) const fn reference(&self) -> Option<BoundReferenceTarget> {
        match &self.root {
            ExecutionInput::Reference(reference) => Some(*reference),
            ExecutionInput::Argument(_)
            | ExecutionInput::Static(_)
            | ExecutionInput::Storage(_) => None,
        }
    }
    pub(crate) fn is_callable_input(&self) -> bool {
        matches!(
            self.reference(),
            Some(BoundReferenceTarget::Surface(
                AnySymbolId::CallableParameter(_)
                    | AnySymbolId::PredicateParameter(_)
                    | AnySymbolId::ReceiverParameter(_)
            )) | Some(BoundReferenceTarget::Local(
                bray_symbols::AnyLocalSymbolId::AnonymousCallableParameter(_)
            ))
        )
    }

    pub(crate) fn binding(
        storage: &bray_bound_tree::StoragePlan,
        target: bray_bound_tree::StorageBindingTarget,
    ) -> Option<Self> {
        if let Some(reference) = target.reference_target() {
            return Some(reference.into());
        }

        let (bray_bound_tree::StorageBindingTarget::PatternDiscard(_)
            | bray_bound_tree::StorageBindingTarget::InvocationInput { .. }) = target else {
            return None;
        };

        let bray_bound_tree::StorageBinding::Identity(identity) = storage.binding(target)? else {
            return None;
        };

        Self::storage_identity(storage, identity)
    }

    pub(crate) fn storage(
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Self> {
        Self::storage_owner(storage, access)?.project(storage.resolved_projections(access)?)
    }

    pub(crate) fn storage_path(
        storage: &bray_bound_tree::StoragePlan,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<Self> {
        Self::storage_identity(storage, path.root)?.project(&path.projections)
    }

    pub(crate) fn storage_owner(
        storage: &bray_bound_tree::StoragePlan,
        access: bray_bound_tree::StorageAccessId,
    ) -> Option<Self> {
        let identity = storage.root_identity(access)?;

        Self::storage_identity(storage, identity)
    }

    pub(crate) fn storage_identity(
        storage: &bray_bound_tree::StoragePlan,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> Option<Self> {
        let root = match storage.identity(identity)? {
            bray_bound_tree::StorageIdentity::Parameter(parameter) => {
                BoundReferenceTarget::Surface(parameter.into())
            }
            bray_bound_tree::StorageIdentity::Receiver(receiver) => {
                BoundReferenceTarget::Surface(receiver.into())
            }
            bray_bound_tree::StorageIdentity::ContractParameter(parameter) => {
                BoundReferenceTarget::Local(parameter.into())
            }
            bray_bound_tree::StorageIdentity::AnonymousParameter(parameter) => {
                BoundReferenceTarget::Local(parameter.into())
            }
            bray_bound_tree::StorageIdentity::PostconditionResult(result) => {
                BoundReferenceTarget::Local(result.into())
            }
            kind => {
                if let Some(reference) = storage
                    .binding_targets(bray_bound_tree::StorageBinding::Identity(identity))
                    .find_map(bray_bound_tree::StorageBindingTarget::reference_target)
                {
                    reference
                } else if matches!(kind,
                    bray_bound_tree::StorageIdentity::LocalOwned(_)
                    | bray_bound_tree::StorageIdentity::InvocationInput { .. }
                    | bray_bound_tree::StorageIdentity::InvocationResult(_)
                ) {
                    return Some(Self {
                        root: ExecutionInput::Storage(identity),
                        projections: Arc::new([]),
                    });
                } else {
                    return None;
                }
            }
        };

        Some(Self::from(root))
    }

    pub(crate) fn project(
        mut self,
        projections: &[bray_bound_tree::StorageProjection],
    ) -> Option<Self> {
        for projection in projections {
            self = self.component(Self::projection_kind(*projection)?);
        }

        Some(self)
    }

    pub(crate) fn storage_observation(
        storage: &bray_bound_tree::StoragePlan,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<Self> {
        let mut place = Self::storage_identity(storage, path.root)?;

        for projection in path.projections.iter() {
            let Some(projection) = Self::projection_kind(*projection) else {
                break;
            };

            place = place.component(projection);
        }

        Some(place)
    }

    fn projection_kind(
        projection: bray_bound_tree::StorageProjection,
    ) -> Option<ConstantProjectionKind> {
        use bray_bound_tree::StorageProjection;

        let projection = match projection {
            StorageProjection::ProductField(field) => ConstantProjectionKind::ProductField(field),
            StorageProjection::ActiveUnionPayloadField { field, .. } => {
                ConstantProjectionKind::UnionPayloadField(field)
            }
            StorageProjection::NullableValue => ConstantProjectionKind::NullableValue,
            StorageProjection::TupleElement(index) => ConstantProjectionKind::TupleElement(index),
            StorageProjection::ElementFromStart(index) => {
                ConstantProjectionKind::ArrayElementOrdinal(index)
            }
            _ => return None,
        };

        Some(projection)
    }

    pub(crate) fn value_in(
        &self,
        values: &std::collections::BTreeMap<Self, super::ExecutionCondition>,
    ) -> Option<super::ExecutionCondition> {
        for length in (0..=self.projections.len()).rev() {
            let prefix = Self {
                root: self.root.clone(),
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

    pub(crate) fn overlaps_any(&self, places: &std::collections::BTreeSet<Self>) -> bool {
        if places
            .range(self.clone()..)
            .next()
            .is_some_and(|place| self.contains(place))
        {
            return true;
        }

        self.is_contained_by_any(places)
    }

    pub(crate) fn is_contained_by_any(&self, places: &std::collections::BTreeSet<Self>) -> bool {
        (0..=self.projections.len()).any(|length| {
            let mut prefix = self.clone();

            prefix.projections = self.projections[..length].into();

            places.contains(&prefix)
        })
    }

    pub(crate) fn overlaps(&self, other: &Self) -> bool {
        self.contains(other) || other.contains(self)
    }
}

