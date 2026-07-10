//! Source-correlated semantic trees after name binding.

#![forbid(unsafe_code)]

mod draft;
mod node;
mod origin;
#[cfg(test)]
mod test_support;
mod unit;

pub use draft::BoundUnitDraft;
pub use node::{
    AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundNodeKind,
    BoundPatternId, ExactBoundNodeId,
};
pub use origin::{
    BoundNodeOrdinal, BoundNodeOrigin, BoundSynthesisRole, SynthesizedBoundNodeOrigin,
};
pub use unit::{
    AnonymousCallableUnitKey, BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKeyData,
    BoundUnitKind, DeclaredBoundUnitKey,
};
