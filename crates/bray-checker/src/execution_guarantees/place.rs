use std::sync::Arc;

use bray_bound_tree::BoundReferenceTarget;
use bray_symbols::{AnySymbolId, ConstantProjectionKind};

/// Value observations used by construction and read-only normalization.
pub(crate) trait ExecutionValues {
    fn get_value(&self, place: &ExecutionPlace) -> Option<&super::ExecutionCondition>;
    fn get_value_mut(&mut self, place: &ExecutionPlace) -> Option<&mut super::ExecutionCondition>;

    fn value_at(&self, place: &ExecutionPlace) -> Option<super::ExecutionCondition> {
        for length in (0..=place.projections.len()).rev() {
            let prefix = ExecutionPlace {
                root: place.root.clone(),
                projections: place.projections[..length].into(),
            };

            if let Some(value) = self.get_value(&prefix) {
                return Some(place.projections[length..].iter().copied()
                    .fold(value.clone(), super::ExecutionCondition::component));
            }
        }

        None
    }

    /// Declaration normalization may introduce an unbound formal once. An
    /// execution-state reader must not recover an entry after losing its value.
    fn formal_input(&self, place: &ExecutionPlace) -> Option<super::ExecutionCondition> {
        if place.is_pointee() {
            return None;
        }

        Some(super::ExecutionCondition::new(super::ExecutionConditionKind::Input(place.clone())))
    }
}

impl ExecutionValues for std::collections::BTreeMap<ExecutionPlace, super::ExecutionCondition> {
    fn get_value(&self, place: &ExecutionPlace) -> Option<&super::ExecutionCondition> {
        self.get(place)
    }

    fn get_value_mut(&mut self, place: &ExecutionPlace) -> Option<&mut super::ExecutionCondition> {
        self.get_mut(place)
    }
}

/// An observable input or local place, including its selected component path.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionPlace {
    pub(crate) root: ExecutionInput,
    pub(crate) projections: Arc<[ConstantProjectionKind]>,
}

/// An evaluated inline view is distinct from storage reached through its owner.
pub(crate) enum ExecutionStorageSelection {
    Inline(ExecutionPlace),
    Reached,
    UnresolvedInline(ExecutionPlace),
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum ExecutionInput {
    Reference(BoundReferenceTarget),
    Constant(bray_symbols::ConstantTermId),
    Static(Arc<bray_symbols::StaticReferenceSelection>),
    Argument(bray_symbols::SymbolOrdinal),
    FrameInput {
        holder: Arc<ExecutionPlace>,
        argument: bray_symbols::SymbolOrdinal,
        source: Arc<super::ExecutionCondition>,
    },
    Storage(bray_bound_tree::StorageIdentityId),
    Pointee {
        target: bray_bound_tree::StorageTarget,
        ty: bray_symbols::TypeId,
        producer: Option<bray_bound_tree::BoundExecutionSite>,
    },
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
    pub(crate) fn pointee(
        storage: &bray_bound_tree::StoragePlan,
        target: &bray_bound_tree::StorageTarget,
        ty: bray_symbols::TypeId,
    ) -> Self {
        let identity = match &target.origin {
            bray_bound_tree::StorageOrigin::Returned { reference, .. } => Some(reference.root),
            bray_bound_tree::StorageOrigin::Allocation { allocation, .. } => Some(allocation.root),
            _ => None,
        };

        let producer = identity.and_then(|identity| storage.identity(identity))
            .and_then(|identity| identity.definition_occurrence());

        Self {
            root: ExecutionInput::Pointee { target: target.clone(), ty, producer },
            projections: Arc::new([]),
        }
    }

    pub(crate) fn is_pointee(&self) -> bool {
        matches!(self.root, ExecutionInput::Pointee { .. })
    }

    pub(crate) fn depends_on(&self, mutation: bray_bound_tree::StorageMutation) -> bool {
        matches!(&self.root, ExecutionInput::Pointee { producer: Some(producer), .. }
            if mutation == bray_bound_tree::StorageMutation::Operation(*producer))
    }

    /// Selects retained inline data without extending its place into reached storage.
    /// Unresolved selectors name only the containing view, never a guessed child.
    pub(crate) fn select_storage_value(
        storage: &bray_bound_tree::StoragePlan,
        path: &bray_bound_tree::StoragePath,
        values: &(impl ExecutionValues + ?Sized),
        mut index: impl FnMut(&Self, bray_bound_tree::StorageProjection) -> Option<bray_symbols::SymbolOrdinal>,
    ) -> Option<ExecutionStorageSelection> {
        use bray_bound_tree::StorageProjection;

        if path.projections.contains(&StorageProjection::OwnedTarget) {
            return Some(ExecutionStorageSelection::Reached);
        }

        let mut place = Self::storage_identity(storage, path.root)?;

        for projection in path.projections.iter().copied() {
            let actual = place.value_in(values);

            if actual.as_ref().and_then(super::ExecutionCondition::ownership_identity)
                .is_some_and(|(kind, _)| kind == super::ExecutionObservationKind::Reference)
            {
                return Some(ExecutionStorageSelection::Reached);
            }

            let projection = Self::projection_kind(projection).or_else(|| {
                let ordinal = match projection {
                    StorageProjection::ElementFromEnd(offset) => actual.as_ref()
                        .and_then(literal_array_extent)
                        .and_then(|length| length.checked_sub(usize::try_from(offset.raw()).ok()?.checked_add(1)?))
                        .and_then(|index| u32::try_from(index).ok())
                        .map(bray_symbols::SymbolOrdinal::new)
                        .or_else(|| index(&place, projection)),
                    StorageProjection::Element(_) => index(&place, projection),
                    _ => None,
                }?;

                Some(ConstantProjectionKind::ArrayElementOrdinal(ordinal))
            });

            let Some(projection) = projection else {
                return Some(ExecutionStorageSelection::UnresolvedInline(place));
            };

            place = place.component(projection);
        }

        Some(ExecutionStorageSelection::Inline(place))
    }

    /// Identifies one associated constant application, including its generic arguments.
    pub fn constant_reference(term: bray_symbols::ConstantTermId) -> Self {
        Self {
            root: ExecutionInput::Constant(term),
            projections: Arc::new([]),
        }
    }

    pub(crate) const fn constant_term(&self) -> Option<bray_symbols::ConstantTermId> {
        match self.root {
            ExecutionInput::Constant(term) => Some(term),
            _ => None,
        }
    }

    /// Identifies one selected static instance, including its substitution and witnesses.
    pub fn static_instance(selection: bray_symbols::StaticReferenceSelection) -> Self {
        Self {
            root: ExecutionInput::Static(Arc::new(selection)),
            projections: Arc::new([]),
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
            ExecutionInput::Constant(_)
            | ExecutionInput::Argument(_)
            | ExecutionInput::FrameInput { .. }
            | ExecutionInput::Static(_)
            | ExecutionInput::Storage(_)
            | ExecutionInput::Pointee { .. } => None,
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
        | bray_bound_tree::StorageBindingTarget::InvocationInput { .. }) = target
        else {
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
        Self::storage_identity(storage, storage.root_identity(access)?)?
            .project(storage.resolved_projections(access)?)
    }

    pub(crate) fn storage_path(
        storage: &bray_bound_tree::StoragePlan,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<Self> {
        Self::storage_identity(storage, path.root)?.project(&path.projections)
    }

    /// Selects the containing storage that bounds a checked path's lifetime.
    /// Dynamic elements and ranges keep their containing owner without naming a scalar value.
    pub(crate) fn storage_owner(
        storage: &bray_bound_tree::StoragePlan,
        path: &bray_bound_tree::StoragePath,
    ) -> Option<Self> {
        Some(Self::storage_identity(storage, path.root)?.project_observation(&path.projections).0)
    }

    pub(crate) fn storage_identity(
        storage: &bray_bound_tree::StoragePlan,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> Option<Self> {
        let root = match storage.identity(identity)? {
            bray_bound_tree::StorageIdentity::Static(selection) => {
                return Some(Self { root: ExecutionInput::Static(selection), projections: Arc::new([]) });
            }
            bray_bound_tree::StorageIdentity::Parameter(parameter) => {
                BoundReferenceTarget::Surface(parameter.into())
            }
            bray_bound_tree::StorageIdentity::PredicateParameter(parameter) => {
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
                    .or_else(|| storage.root_access(identity).and_then(|access| {
                        storage.binding_targets(bray_bound_tree::StorageBinding::Access(access))
                            .find_map(bray_bound_tree::StorageBindingTarget::reference_target)
                    }))
                {
                    reference
                } else if Self::tracks_local_storage(&kind) {
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

    pub(crate) fn tracks_local_storage(kind: &bray_bound_tree::StorageIdentity) -> bool {
        matches!(kind,
            bray_bound_tree::StorageIdentity::LocalOwned(_)
                | bray_bound_tree::StorageIdentity::Temporary(_)
                | bray_bound_tree::StorageIdentity::Evaluated(_)
                | bray_bound_tree::StorageIdentity::Result(_)
                | bray_bound_tree::StorageIdentity::InvocationInput { .. }
                | bray_bound_tree::StorageIdentity::InvocationResult(_)
                | bray_bound_tree::StorageIdentity::ScopedCapability { .. }
                | bray_bound_tree::StorageIdentity::ScopedResultReplacement(_)
        )
    }

    pub(crate) fn project(
        self,
        projections: &[bray_bound_tree::StorageProjection],
    ) -> Option<Self> {
        let (place, observed) = self.project_observation(projections);

        (observed.len() == projections.len()).then_some(place)
    }

    /// Selects the nearest observable prefix of a checked physical path.
    pub(crate) fn project_observation<'path>(
        self,
        path: &'path [bray_bound_tree::StorageProjection],
    ) -> (Self, &'path [bray_bound_tree::StorageProjection]) {
        if path.is_empty() {
            return (self, path);
        }

        let mut length = 0;
        let components = path.iter().copied().map_while(Self::projection_kind).inspect(|_| length += 1);
        let projections = self.projections.iter().copied().chain(components).collect();

        (Self { root: self.root, projections }, &path[..length])
    }

    pub(crate) fn projection_kind(
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
        values: &(impl ExecutionValues + ?Sized),
    ) -> Option<super::ExecutionCondition> {
        values.value_at(self)
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
}

fn literal_array_extent(mut value: &super::ExecutionCondition) -> Option<usize> {
    for _ in 0..super::ExecutionCondition::WORK_LIMIT {
        match value.kind() {
            super::ExecutionConditionKind::CapturedValue { value: contents, .. } | super::ExecutionConditionKind::DeferredValue { value: contents, .. } => value = contents,
            super::ExecutionConditionKind::Observation { contents, kind: super::ExecutionObservationKind::Owned | super::ExecutionObservationKind::Scalar, .. } => value = contents,
            super::ExecutionConditionKind::Constructed(base, _) => value = base,
            super::ExecutionConditionKind::Literal(value) => return match value.kind() {
                bray_symbols::ConstantValueKind::Array(elements) => Some(elements.len()),
                _ => None,
            },
            _ => return None,
        }
    }

    None
}
