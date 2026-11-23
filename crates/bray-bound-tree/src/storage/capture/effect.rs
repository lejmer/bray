use std::sync::Arc;

use crate::{StorageTarget as Target, StorageValueRetirement as Retirement, StorageMutationTarget as MutationTarget};
use bray_symbols::{DependencyMutationKind, TypeId};

/// A definite transfer of the complete value from one reached typed cell.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageDischarge {
    pub cell: Target,
    pub ty: TypeId,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageEffect {
    Unknown,
    Ambient,
    /// Invocation inputs may escape. A bound records the admitted input regions.
    Escape(Option<Arc<[Target]>>),
    Atomic,
    // The original checked access type belongs to the mutation. An alias bound
    // supplies its region, never a substitute raw access or transfer type.
    Mutation {
        address: Option<Target>,
        footprint: Target,
        discharged: Option<Arc<[StorageDischarge]>>,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    },
    UnboundedMutation {
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
        ambient: bool,
    },
}

impl StorageEffect {
    pub fn at(
        target: Target,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    ) -> Self {
        Self::through(MutationTarget::location(target), kind, access_type)
    }

    pub fn bounded(
        target: Target,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    ) -> Self {
        Self::through(MutationTarget::bounded(target), kind, access_type)
    }

    pub fn through(
        target: MutationTarget,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    ) -> Self {
        let kind = if target.address.is_some() { kind } else { kind.region_kind() };

        Self::Mutation { address: target.address, footprint: target.footprint, discharged: None, kind, access_type }
    }

    pub fn with_retirement_discharges(mut self, duties: impl IntoIterator<Item = StorageDischarge>) -> Self {
        if let Self::Mutation { address: Some(_), kind: DependencyMutationKind::Retire, discharged, .. } = &mut self {
            let duties = duties.into_iter().collect::<Vec<_>>();

            if !duties.is_empty() {
                *discharged = Some(bray_base::sorted_unique_shared_slice(duties));
            }
        }

        self
    }

    pub fn is_ambient(&self) -> bool {
        matches!(self, Self::Ambient | Self::UnboundedMutation { ambient: true, .. } | Self::Mutation { kind: DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_), .. })
    }

    pub fn may_reach_all_inputs(&self) -> bool {
        matches!(self, Self::Unknown | Self::UnboundedMutation { ambient: false, .. })
    }

    /// Raw deallocation and owning-buffer release both end a backing lifetime.
    pub fn is_unlocated_deallocation(&self) -> bool {
        matches!(self, Self::UnboundedMutation { kind: DependencyMutationKind::Retire, .. }
            | Self::Mutation { address: None, kind: DependencyMutationKind::Retire, .. })
    }

    /// Whether this effect leaves backing storage lifetimes intact.
    pub fn preserves_storage_lifetimes(&self) -> bool {
        match self {
            Self::Unknown | Self::Ambient => false,
            Self::Escape(_) | Self::Atomic => true,
            Self::Mutation { kind, .. } | Self::UnboundedMutation { kind, .. } => !matches!(
                kind,
                DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_)
                    | DependencyMutationKind::Retire
            ),
        }
    }

    pub fn unbounded(
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
        ambient: bool,
    ) -> Self {
        match kind {
            DependencyMutationKind::Atomic => Self::Atomic,
            DependencyMutationKind::Escape => Self::Escape(None),
            _ => Self::UnboundedMutation { kind, access_type, ambient },
        }
    }

    pub fn is_atomic(&self) -> bool {
        matches!(
            self,
            Self::Atomic | Self::Mutation { kind: DependencyMutationKind::Atomic, .. }
        )
    }

    pub fn retire(&mut self, current: &Target, retirement: Retirement) {
        if let Self::Mutation { address, footprint, discharged, kind, .. } = self {
            if retirement.contains(current, footprint) {
                footprint.previous = true;
            }

            if address.as_ref().is_some_and(|target| retirement.contains(current, target)) {
                *address = None;
                *discharged = None;
                *kind = kind.region_kind();
            } else if let Some(duties) = discharged
                && duties.iter().any(|duty| retirement.contains(current, &duty.cell))
            {
                let retained = duties.iter().filter(|duty| !retirement.contains(current, &duty.cell))
                    .cloned().collect::<Arc<[_]>>();

                *discharged = (!retained.is_empty()).then_some(retained);
            }
        }

        if let Self::Escape(Some(targets)) = self
            && targets.iter().any(|target| retirement.contains(current, target))
        {
            *targets = bray_base::sorted_unique_shared_slice(targets.iter().cloned().map(|mut target| {
                if retirement.contains(current, &target) {
                    target.previous = true;
                }

                target
            }));
        }
    }

    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        match self {
            Self::Mutation { address, footprint, discharged, .. } => address.iter().chain(std::iter::once(footprint))
                .chain(discharged.iter().flat_map(|duties| duties.iter().map(|duty| &duty.cell)))
                .any(|target| retirement.contains(current, target)),
            Self::Escape(Some(targets)) => targets.iter().any(|target| retirement.contains(current, target)),
            _ => false,
        }
    }

    /// Regions known to escape on this effect.
    pub fn escape_regions(&self) -> &[Target] {
        match self {
            Self::Escape(Some(regions)) => regions,
            Self::Mutation { footprint, kind: DependencyMutationKind::Escape, .. } => std::slice::from_ref(footprint),
            _ => &[],
        }
    }

    /// Returns a definite typed update to a reached cell.
    pub fn cell_change(&self) -> Option<(&Target, DependencyMutationKind)> {
        match self {
            Self::Mutation { address: Some(target), kind: kind @ (DependencyMutationKind::Replace(_) | DependencyMutationKind::Discharge(_)), .. }
                if target.is_exact() => Some((target, *kind)),
            _ => None,
        }
    }

    pub fn mutation(
        &self,
    ) -> Option<(&Target, DependencyMutationKind)> {
        match self {
            Self::Mutation { kind: DependencyMutationKind::Escape, .. } => None,
            Self::Mutation { footprint: target, kind, .. } => Some((target, *kind)),
            Self::Unknown | Self::Ambient | Self::Escape(_) | Self::Atomic | Self::UnboundedMutation { .. } => None,
        }
    }
}

/// Effects resolved before input ownership transfers, retained only until the call outcome.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCapturedEffects {
    pub mutations: Arc<[StorageEffect]>,
    pub cell_changes: Option<Arc<[StorageEffect]>>,
    pub escaped: Vec<Target>,
    /// Entry-relative escape effects captured before the input slots were consumed.
    pub escape_prefix: Arc<[bray_symbols::DependencyStorageEffect]>,
    pub invalidation: crate::StorageInvalidation,
}

impl StorageCapturedEffects {
    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        self.escaped.iter().any(|target| retirement.contains(current, target))
            || self.mutations.iter().chain(self.cell_changes.iter().flat_map(|changes| changes.iter()))
                .any(|effect| effect.references(current, retirement))
    }

    pub fn retire(&mut self, current: &Target, retirement: Retirement) {
        if self.mutations.iter().any(|effect| effect.references(current, retirement)) {
            self.mutations = bray_base::sorted_unique_shared_slice(self.mutations.iter().cloned().map(|mut effect| {
                effect.retire(current, retirement);

                effect
            }));
        }

        if let Some(changes) = self.cell_changes.as_ref()
            && changes.iter().any(|effect| effect.references(current, retirement))
        {
            let retained = changes.iter().filter(|effect| !effect.references(current, retirement)).cloned().collect::<Vec<_>>();

            self.cell_changes = Some(retained.into());
        }

        for target in &mut self.escaped {
            if retirement.contains(current, target) {
                target.previous = true;
            }
        }

        self.normalize();
    }

    fn normalize(&mut self) {
        self.escaped.sort_unstable();
        self.escaped.dedup();
    }

    pub fn merge(&mut self, incoming: &Self) {
        if self.mutations != incoming.mutations {
            self.mutations = bray_base::sorted_unique_shared_slice(self.mutations.iter().chain(incoming.mutations.iter()).cloned());
        }

        self.cell_changes = bray_symbols::common_cell_changes([self.cell_changes.as_ref(), incoming.cell_changes.as_ref()].into_iter());
        self.escaped.extend(incoming.escaped.iter().cloned());

        if self.escape_prefix != incoming.escape_prefix {
            self.escape_prefix = bray_base::sorted_unique_shared_slice(self.escape_prefix.iter().chain(&*incoming.escape_prefix).cloned());
        }

        self.normalize();
        self.invalidation.merge(&incoming.invalidation);
    }
}
