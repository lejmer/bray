use super::model::{AddressValue, Place, Retirement, State, Target};
use bray_symbols::{DependencyMutationKind, TypeId};

/// One evaluated operand and its possible mutation region. Bounds never supply an address.
#[derive(Clone)]
pub(in crate::analysis::storage_flow) struct MutationTarget {
    pub(super) address: Option<Target>,
    pub(super) footprint: Target,
}

impl MutationTarget {
    pub(in crate::analysis::storage_flow) fn footprint(&self) -> &Target { &self.footprint }

    pub(in crate::analysis::storage_flow) fn location(target: Target) -> Self {
        // The address owns its descriptor while both paths share immutable projections.
        Self { address: Some(target.clone()), footprint: target }
    }

    pub(in crate::analysis::storage_flow) fn bounded(footprint: Target) -> Self {
        Self { address: None, footprint }
    }

    pub(super) fn reference(address: Option<&AddressValue>, regions: Vec<Target>) -> Vec<Self> {
        regions.into_iter().map(|footprint| {
            let actual = address.filter(|address| address.has_values()
                && address.targets.contains(&footprint) && footprint.is_exact());

            // A captured address owns its descriptor independently of later slot replacement.
            Self { address: actual.map(|_| footprint.clone()), footprint }
        }).collect()
    }

    pub(in crate::analysis::storage_flow) fn project(self, suffix: &[bray_bound_tree::StorageProjection]) -> Self {
        Self {
            address: self.address.map(|address| address.project(suffix)),
            footprint: self.footprint.project(suffix),
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis::storage_flow) enum Effect {
    Unknown,
    Ambient,
    Escape,
    Atomic,
    // The original checked access type belongs to the mutation. An alias bound
    // supplies its region, never a substitute raw access or transfer type.
    Mutation {
        address: Option<Target>,
        footprint: Target,
        kind: bray_symbols::DependencyMutationKind,
        access_type: Option<TypeId>,
    },
    UnboundedMutation {
        kind: bray_symbols::DependencyMutationKind,
        access_type: Option<TypeId>,
        ambient: bool,
    },
}

impl Effect {
    pub(in crate::analysis::storage_flow) fn at(
        target: Target,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    ) -> Self {
        Self::through(MutationTarget::location(target), kind, access_type)
    }

    pub(in crate::analysis::storage_flow) fn bounded(
        target: Target,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    ) -> Self {
        Self::through(MutationTarget::bounded(target), kind, access_type)
    }

    pub(in crate::analysis::storage_flow) fn through(
        target: MutationTarget,
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
    ) -> Self {
        let kind = match kind {
            DependencyMutationKind::Dispose(ty) if target.address.is_none() => DependencyMutationKind::Cleanup(ty),
            kind => kind,
        };

        Self::Mutation { address: target.address, footprint: target.footprint, kind, access_type }
    }

    pub(in crate::analysis::storage_flow) fn is_ambient(&self) -> bool {
        matches!(self, Self::Ambient | Self::UnboundedMutation { ambient: true, .. } | Self::Mutation { kind: bray_symbols::DependencyMutationKind::Dispose(_) | bray_symbols::DependencyMutationKind::Cleanup(_), .. })
    }

    pub(in crate::analysis::storage_flow) fn may_reach_all_inputs(&self) -> bool {
        matches!(self, Self::Unknown | Self::UnboundedMutation { ambient: false, .. })
    }

    pub(in crate::analysis::storage_flow) fn unbounded(
        kind: bray_symbols::DependencyMutationKind,
        access_type: Option<TypeId>,
        ambient: bool,
    ) -> Self {
        match kind {
            bray_symbols::DependencyMutationKind::Atomic => Self::Atomic,
            bray_symbols::DependencyMutationKind::Escape => Self::Escape,
            _ => Self::UnboundedMutation { kind, access_type, ambient },
        }
    }

    pub(in crate::analysis::storage_flow) fn is_atomic(&self) -> bool {
        matches!(
            self,
            Self::Atomic | Self::Mutation { kind: bray_symbols::DependencyMutationKind::Atomic, .. }
        )
    }

    pub(super) fn retire(&mut self, current: &Target, retirement: Retirement) {
        if let Self::Mutation { address, footprint, .. } = self {
            for target in address.iter_mut().chain(std::iter::once(footprint)) {
                if retirement.contains(current, target) { target.previous = true; }
            }
        }
    }

    pub(super) fn references(&self, current: &Target, retirement: Retirement) -> bool {
        match self {
            Self::Mutation { address, footprint, .. } => address.iter().chain(std::iter::once(footprint))
                .any(|target| retirement.contains(current, target)),
            _ => false,
        }
    }

    pub(in crate::analysis::storage_flow) fn may_retain_address(&self, memory: &State) -> bool {
        match self {
            // Writes confined to an invocation-local slot do not publish an address.
            // Stored reference relations still track later publication of that slot.
            Self::Mutation { footprint: target, kind, .. } => {
                kind.is_write()
                    && *kind != bray_symbols::DependencyMutationKind::Clear
                    && target.storage_path().is_none_or(|path| {
                        memory.owned_storage.binary_search(&path.root).is_err()
                            || path
                                .projections
                                .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                            || memory
                                .contents
                                .get(&Target::storage(Place::root(path.root)))
                                .is_some_and(|contents| contents.escaped)
                    })
            }
            Self::UnboundedMutation { kind, .. } => kind.is_write() && *kind != bray_symbols::DependencyMutationKind::Clear,
            Self::Unknown | Self::Escape => true,
            Self::Ambient | Self::Atomic => false,
        }
    }

    pub(in crate::analysis::storage_flow) fn mutation(
        &self,
    ) -> Option<(&Target, bray_symbols::DependencyMutationKind)> {
        match self {
            Self::Mutation { kind: bray_symbols::DependencyMutationKind::Escape, .. } => None,
            Self::Mutation { footprint: target, kind, .. } => Some((target, *kind)),
            Self::Unknown | Self::Ambient | Self::Escape | Self::Atomic | Self::UnboundedMutation { .. } => None,
        }
    }
}

/// Effects resolved before input ownership transfers, retained only until the call outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct CapturedEffects {
    pub(in crate::analysis::storage_flow) mutations: Vec<Effect>,
    pub(in crate::analysis::storage_flow) escaped: Vec<Target>,
    pub(in crate::analysis::storage_flow) invalidation: bray_bound_tree::StorageInvalidation,
}

impl CapturedEffects {
    pub(super) fn references(&self, current: &Target, retirement: Retirement) -> bool {
        self.escaped.iter().any(|target| retirement.contains(current, target))
            || self.mutations.iter().any(|effect| effect.references(current, retirement))
    }

    pub(super) fn retire(&mut self, current: &Target, retirement: Retirement) {
        for effect in &mut self.mutations {
            effect.retire(current, retirement);
        }

        for target in &mut self.escaped {
            if retirement.contains(current, target) {
                target.previous = true;
            }
        }

        self.normalize();
    }

    fn normalize(&mut self) {
        // Retiring or joining incarnations can merge already summarized targets.
        self.mutations.sort_unstable();
        self.mutations.dedup();
        self.escaped.sort_unstable();
        self.escaped.dedup();
    }

    pub(super) fn merge(&mut self, incoming: &Self) {
        // Outcome sets copy descriptors and share immutable projection paths.
        self.mutations.extend(incoming.mutations.iter().cloned());
        self.escaped.extend(incoming.escaped.iter().cloned());
        self.normalize();
        self.invalidation.merge(&incoming.invalidation);
    }
}
