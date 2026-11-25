mod effect;
mod model;
mod references;
mod transition;

#[cfg(test)]
mod tests;

pub(super) use model::{
    EffectOutcomes, Origin, Place, ReachedAccess, ReferenceBounds, ReferenceExit, ReferenceExitValue, State, Target,
};

pub(super) use effect::{CapturedEffects, Effect};
pub(super) use bray_bound_tree::StorageMutationTarget as MutationTarget;
pub(super) use references::{Source, borrow_chain_from};
