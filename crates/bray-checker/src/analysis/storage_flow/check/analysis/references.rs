mod assignment;
mod cleanup;
mod captured;
pub(super) use captured::ValuePublication;
mod entry;
mod escape;
mod invocation;
mod places;
mod outputs;
mod ownership;
mod prepare;
mod publication;
mod stored;
mod transfer;

pub(super) use outputs::InvocationOutputs;
pub(super) use escape::{ReachedReferenceRegions, ResolvedEscapes};
