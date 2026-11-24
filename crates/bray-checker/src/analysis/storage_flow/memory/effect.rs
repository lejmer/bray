use super::model::{Place, State, Target};
use bray_symbols::DependencyMutationKind;

pub(in crate::analysis::storage_flow) use bray_bound_tree::{StorageCapturedEffects as CapturedEffects, StorageEffect as Effect};

impl State {
    pub(in crate::analysis::storage_flow) fn may_retain_address(&self, effect: &Effect) -> bool {
        match effect {
            // Writes confined to an invocation-local slot do not publish an address.
            // Stored reference relations still track later publication of that slot.
            Effect::Mutation { footprint: target, kind, .. } => {
                kind.is_write()
                    && *kind != DependencyMutationKind::Clear
                    && target.storage_path().is_none_or(|path| {
                        self.owned_storage.binary_search(&path.root).is_err()
                            || path
                                .projections
                                .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                            || self
                                .contents
                                .get(&Target::storage(Place::root(path.root)))
                                .is_some_and(|contents| contents.escaped)
                    })
            }
            Effect::UnboundedMutation { kind, .. } => kind.is_write() && *kind != DependencyMutationKind::Clear,
            Effect::Unknown | Effect::Escape(_) => true,
            Effect::BorrowedReach(_) | Effect::Ambient | Effect::Atomic | Effect::CapturedCleanup { .. } => false,
        }
    }

}
