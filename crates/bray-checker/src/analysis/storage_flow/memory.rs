mod model;
mod references;
mod transition;

#[cfg(test)]
mod tests;

pub(super) use model::{
    CapturedEffects, Effect, Origin, Place, ReferenceBounds, ReferenceExit, State, Target,
};

pub(super) use references::Source;
