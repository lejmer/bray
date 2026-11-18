use crate::{StorageTarget as Target, StorageValueRetirement as Retirement, StorageMutationTarget as MutationTarget};
use bray_symbols::{DependencyMutationKind, TypeId};

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageEffect {
    Unknown,
    Ambient,
    Escape,
    Atomic,
    // The original checked access type belongs to the mutation. An alias bound
    // supplies its region, never a substitute raw access or transfer type.
    Mutation {
        address: Option<Target>,
        footprint: Target,
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

        Self::Mutation { address: target.address, footprint: target.footprint, kind, access_type }
    }

    pub fn is_ambient(&self) -> bool {
        matches!(self, Self::Ambient | Self::UnboundedMutation { ambient: true, .. } | Self::Mutation { kind: DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_), .. })
    }

    pub fn may_reach_all_inputs(&self) -> bool {
        matches!(self, Self::Unknown | Self::UnboundedMutation { ambient: false, .. })
    }

    pub fn unbounded(
        kind: DependencyMutationKind,
        access_type: Option<TypeId>,
        ambient: bool,
    ) -> Self {
        match kind {
            DependencyMutationKind::Atomic => Self::Atomic,
            DependencyMutationKind::Escape => Self::Escape,
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
        if let Self::Mutation { address, footprint, kind, .. } = self {
            if retirement.contains(current, footprint) {
                footprint.previous = true;
            }

            if address.as_ref().is_some_and(|target| retirement.contains(current, target)) {
                *address = None;
                *kind = kind.region_kind();
            }
        }
    }

    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        match self {
            Self::Mutation { address, footprint, .. } => address.iter().chain(std::iter::once(footprint))
                .any(|target| retirement.contains(current, target)),
            _ => false,
        }
    }

    pub fn mutation(
        &self,
    ) -> Option<(&Target, DependencyMutationKind)> {
        match self {
            Self::Mutation { kind: DependencyMutationKind::Escape, .. } => None,
            Self::Mutation { footprint: target, kind, .. } => Some((target, *kind)),
            Self::Unknown | Self::Ambient | Self::Escape | Self::Atomic | Self::UnboundedMutation { .. } => None,
        }
    }
}

/// Effects resolved before input ownership transfers, retained only until the call outcome.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCapturedEffects {
    pub mutations: Vec<StorageEffect>,
    pub escaped: Vec<Target>,
    /// Entry-relative escape effects captured before the input slots were consumed.
    pub escape_prefix: std::sync::Arc<[bray_symbols::DependencyStorageEffect]>,
    pub invalidation: crate::StorageInvalidation,
}

impl StorageCapturedEffects {
    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        self.escaped.iter().any(|target| retirement.contains(current, target))
            || self.mutations.iter().any(|effect| effect.references(current, retirement))
    }

    pub fn retire(&mut self, current: &Target, retirement: Retirement) {
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

    pub fn merge(&mut self, incoming: &Self) {
        // Outcome sets copy descriptors and share immutable projection paths.
        self.mutations.extend(incoming.mutations.iter().cloned());
        self.escaped.extend(incoming.escaped.iter().cloned());

        if self.escape_prefix != incoming.escape_prefix {
            self.escape_prefix = bray_base::sorted_unique_shared_slice(self.escape_prefix.iter().chain(&*incoming.escape_prefix).cloned());
        }

        self.normalize();
        self.invalidation.merge(&incoming.invalidation);
    }
}
