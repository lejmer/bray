//! Source-correlated semantic trees after name binding.

#![forbid(unsafe_code)]

mod dependency;
mod identity;
mod node;
mod origin;
mod storage;
mod template;
#[cfg(test)]
mod test_support;
mod tree;
mod unit;
mod view;

pub use dependency::{
    BoundDependencyContract, BoundDependencyContractId, BoundDependencyGuard,
    BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
    DependencyContractInstantiationContext, DependencyContractInstantiationError,
    GuardedBoundDependencyRequirement, LifecycleObligationId, ScopedCapabilityId,
};
pub use node::{
    AnyBoundNodeId, BoundBlock, BoundBlockExpression, BoundBlockId, BoundCallableBody,
    BoundCallableBodyId, BoundCallableBodyKind, BoundErrorCallableBody, BoundErrorExpression,
    BoundErrorPattern, BoundExpression, BoundExpressionId, BoundNodeKind, BoundPattern,
    BoundPatternId, ExactBoundNodeId,
};
pub use origin::{
    BoundNodeOrdinal, BoundNodeOrigin, BoundSynthesisRole, SynthesizedBoundNodeOrigin,
};
pub use storage::{
    BorrowCapability, BorrowCapabilityId, StorageAccess, StorageAccessId, StorageAccessRoot,
    StorageIdentity, StorageIdentityId, StorageProjection, StorageRelationship,
};
pub use template::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateBuildError, CheckedTemplateBuilder,
    CheckedTemplateCapability, CheckedTemplateCompletion, CheckedTemplateEffect,
    CheckedTemplateInput, CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind,
    CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateTemporary,
    CheckedTemplateTemporaryId, CheckedTemplateTrustedObligation, CheckedTemplateWitness,
};
pub use tree::{
    BoundTree, BoundTreeBuildError, BoundTreeBuilder, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, walk_bound_tree,
};
pub use unit::{
    AnonymousCallableUnitKey, BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKeyData,
    BoundUnitKind, DeclaredBoundUnitKey,
};
pub use view::BoundUnitView;
