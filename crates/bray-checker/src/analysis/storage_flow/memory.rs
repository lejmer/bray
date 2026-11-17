mod effect;
mod model;
mod references;
mod transition;
mod value;

#[cfg(test)]
mod tests;

pub(super) use model::{
    EffectOutcomes, Origin, Place, ReferenceBounds, ReferenceExit, ReferenceExitValue, State, Target,
};

pub(super) use effect::{CapturedEffects, Effect, MutationTarget};
pub(super) use references::Source;
