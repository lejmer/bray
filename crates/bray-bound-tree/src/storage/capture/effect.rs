use std::sync::Arc;

use crate::{StorageTarget as Target, StorageValueBinding, StorageValueRetirement as Retirement, StorageMutationTarget as MutationTarget};
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
    /// Exact referents reached through language borrows while applying an input effect.
    BorrowedReach(Arc<[Target]>),
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
    /// Cleanup whose generic type is still open. The sources remain portable
    /// while physical effects use the regions captured before execution.
    CapturedCleanup {
        ty: TypeId,
        sources: Arc<[Target]>,
        regions: Arc<[Target]>,
        borrowed: Arc<[Target]>,
        inline: Arc<[Target]>,
        complete: bool,
    },
}

impl StorageEffect {
    /// Reads a cleanup operand from its reached value tree or admitted snapshot.
    pub fn capture_cleanup<'a>(
        ty: TypeId, source: &Target, binding: Option<&StorageValueBinding>, include_inline: bool,
        values: impl IntoIterator<Item = &'a StorageValueBinding>,
    ) -> Option<Self> {
        if source.is_exact() && !source.previous && binding.is_some_and(StorageValueBinding::is_no_payload) {
            return None;
        }

        let mut complete = binding.is_some_and(|value| value.whole_complete() || value.reference().is_some());
        let mut regions = std::collections::BTreeSet::new();
        let mut borrowed = std::collections::BTreeSet::new();

        for value in values {
            complete &= value.collect_carried_regions(&mut regions) && value.sources.is_empty();

            if value.borrow_capability().is_some() {
                borrowed.extend(value.reference_regions().into_iter().flatten());
            }
        }

        let inline = if include_inline {
            Arc::from([source.clone()])
        } else {
            Arc::from([])
        };

        let sources = binding.and_then(StorageValueBinding::whole_sources)
            .filter(|(sources, complete)| *complete && !sources.is_empty())
            .map_or_else(|| Arc::from([]), |(sources, _)| sources.iter().cloned().collect());

        Some(Self::CapturedCleanup {
            ty,
            sources,
            regions: regions.into_iter().collect(),
            borrowed: borrowed.into_iter().collect(),
            inline,
            complete,
        })
    }

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
        matches!(self, Self::Ambient | Self::CapturedCleanup { .. } | Self::UnboundedMutation { ambient: true, .. })
            || matches!(self, Self::Mutation { kind, .. } if kind.cleanup_type().is_some())
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
            Self::Unknown | Self::Ambient | Self::CapturedCleanup { .. } => false,
            Self::Escape(_) | Self::BorrowedReach(_) | Self::Atomic => true,
            Self::Mutation { kind, .. } | Self::UnboundedMutation { kind, .. } => {
                kind.cleanup_type().is_none() && *kind != DependencyMutationKind::Retire
            }
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
            _ => Self::UnboundedMutation { kind: kind.unlocated_kind(), access_type, ambient },
        }
    }

    /// An open cleanup with a region bound and no identified value to read.
    pub fn incomplete_cleanup(ty: TypeId, regions: std::collections::BTreeSet<Target>, inline: impl IntoIterator<Item = Target>) -> Self {
        Self::CapturedCleanup {
            ty, sources: [].into(), regions: regions.into_iter().collect(), borrowed: [].into(),
            inline: bray_base::sorted_unique_shared_slice(inline), complete: false,
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

        let retire_regions = |targets: &mut Arc<[Target]>| {
            if !targets.iter().any(|target| retirement.contains(current, target)) { return; }

            *targets = bray_base::sorted_unique_shared_slice(targets.iter().cloned().map(|mut target| {
                if retirement.contains(current, &target) {
                    target.previous = true;
                }

                target
            }));
        };

        match self {
            Self::Escape(Some(targets)) => retire_regions(targets),
            Self::BorrowedReach(targets) => retire_regions(targets),
            Self::CapturedCleanup { sources, regions, borrowed, inline, .. } => {
                retire_regions(sources);
                retire_regions(regions);
                retire_regions(borrowed);
                retire_regions(inline);
            }
            _ => {}
        }
    }

    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        match self {
            Self::Mutation { address, footprint, discharged, .. } => address.iter().chain(std::iter::once(footprint))
                .chain(discharged.iter().flat_map(|duties| duties.iter().map(|duty| &duty.cell)))
                .any(|target| retirement.contains(current, target)),
            Self::Escape(Some(targets)) | Self::BorrowedReach(targets) => {
                targets.iter().any(|target| retirement.contains(current, target))
            }
            Self::CapturedCleanup { sources, regions, inline, .. } => sources.iter().chain(regions.iter()).chain(inline.iter())
                .any(|target| retirement.contains(current, target)),
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
            Self::Unknown | Self::Ambient | Self::Escape(_) | Self::BorrowedReach(_) | Self::Atomic | Self::UnboundedMutation { .. }
            | Self::CapturedCleanup { .. } => None,
        }
    }

    /// The carried regions reached by open cleanup.
    pub fn captured_cleanup_regions(effects: &[Self]) -> impl Iterator<Item = &Target> {
        effects.iter().filter_map(|effect| match effect {
            Self::CapturedCleanup { regions, .. } => Some(regions.iter()),
            _ => None,
        }).flatten()
    }

    /// Exact referents reached through safe invocation inputs.
    pub fn borrowed_reach_regions(effects: &[Self]) -> impl Iterator<Item = &Target> {
        effects.iter().filter_map(|effect| match effect {
            Self::BorrowedReach(regions) => Some(regions.iter()),
            _ => None,
        }).flatten()
    }

    /// Regions carried into open cleanup with their evaluated descriptor kinds.
    pub fn captured_cleanup_reference_regions(
        effects: &[Self],
    ) -> impl Iterator<Item = (&Target, bray_symbols::DependencyReferenceKind)> {
        effects
            .iter()
            .filter_map(|effect| match effect {
                Self::CapturedCleanup {
                    regions, borrowed, ..
                } => Some((regions.iter(), borrowed)),
                _ => None,
            })
            .flat_map(|(regions, borrowed)| {
                regions.map(move |target| {
                    let kind = if borrowed.contains(target) {
                        bray_symbols::DependencyReferenceKind::Borrow
                    } else {
                        bray_symbols::DependencyReferenceKind::Address
                    };

                    (target, kind)
                })
            })
    }

    pub fn incomplete_captured_cleanup(effects: &[Self]) -> bool {
        effects.iter().any(|effect| matches!(effect, Self::CapturedCleanup { complete: false, .. }))
    }
}

/// Effects resolved before input ownership transfers, retained only until the call outcome.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCapturedEffects {
    pub mutations: Arc<[StorageEffect]>,
    pub cell_changes: Option<Arc<[StorageEffect]>>,
    pub escaped: Vec<Target>,
    pub borrowed: Vec<Target>,
    /// Entry-relative escape effects captured before the input slots were consumed.
    pub escape_prefix: Arc<[bray_symbols::DependencyStorageEffect]>,
    pub invalidation: crate::StorageInvalidation,
}

impl StorageCapturedEffects {
    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        self.escaped.iter().chain(&self.borrowed)
            .any(|target| retirement.contains(current, target))
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

        for target in self.escaped.iter_mut().chain(&mut self.borrowed) {
            if retirement.contains(current, target) {
                target.previous = true;
            }
        }

        self.normalize();
    }

    fn normalize(&mut self) {
        self.escaped.sort_unstable();
        self.escaped.dedup();
        self.borrowed.sort_unstable();
        self.borrowed.dedup();
    }

    pub fn merge(&mut self, incoming: &Self) {
        if self.mutations != incoming.mutations {
            self.mutations = bray_base::sorted_unique_shared_slice(self.mutations.iter().chain(incoming.mutations.iter()).cloned());
        }

        self.cell_changes = bray_symbols::common_cell_changes([self.cell_changes.as_ref(), incoming.cell_changes.as_ref()].into_iter());
        self.escaped.extend(incoming.escaped.iter().cloned());
        self.borrowed.extend(incoming.borrowed.iter().cloned());

        if self.escape_prefix != incoming.escape_prefix {
            self.escape_prefix = bray_base::sorted_unique_shared_slice(self.escape_prefix.iter().chain(&*incoming.escape_prefix).cloned());
        }

        self.normalize();
        self.invalidation.merge(&incoming.invalidation);
    }
}
