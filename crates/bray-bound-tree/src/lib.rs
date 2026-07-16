//! Source-correlated semantic trees after name binding.

#![forbid(unsafe_code)]

mod bound_unit;
mod control;
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

pub use bound_unit::{BoundUnit, BoundUnitBuildError, BoundUnitRoot};
pub use control::{CheckedControlFlowFacts, ControlCompletion, ControlCompletionKind};
pub use dependency::{
    BoundDependencyContract, BoundDependencyContractId, BoundDependencyGuard,
    BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
    DependencyContractInstantiationContext, DependencyContractInstantiationError,
    GuardedBoundDependencyRequirement, LifecycleObligationId, ScopedCapabilityId,
};
pub use node::{
    AnyBoundNodeId, BoundAnonymousCallableExpression, BoundArgument, BoundAssignmentExpression,
    BoundAwaitExpression, BoundAwaitResolution, BoundBinaryExpression, BoundBlock,
    BoundBlockExpression, BoundBlockId, BoundBlockItem, BoundCallExpression, BoundCallResolution,
    BoundCallResult, BoundCallableBody, BoundCallableBodyId, BoundCallableBodyKind,
    BoundCallableTarget, BoundControlTransferExpression, BoundControlTransferKind,
    BoundConversionExpression, BoundErrorCallExpression, BoundErrorCallableBody,
    BoundErrorConversionExpression, BoundErrorExpression, BoundExpression, BoundExpressionId,
    BoundForExpression, BoundFutureComposition, BoundFutureConstruction, BoundGeneratorExpression,
    BoundLeadingDotVariantExpression, BoundLocalBinding, BoundLocalConstant, BoundMatchArm,
    BoundMatchExpression, BoundMemberAccessExpression, BoundMemberSelector, BoundNameExpression,
    BoundNodeKind, BoundOperator, BoundPattern, BoundPatternId, BoundPatternKind, BoundPatternMode,
    BoundPatternTarget, BoundReferenceTarget, BoundResolvedCall, BoundStructConstructionExpression,
    BoundStructFieldInitializer, BoundStructuredExpression, BoundStructuredExpressionKind,
    BoundTraitQualifiedMemberExpression, BoundTypeReference, BoundUnaryExpression,
    BoundUnresolvedReferenceExpression, BoundUnresolvedReferenceKind, ExactBoundNodeId,
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
    CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation,
    CheckedTemplateShortCircuitKind, CheckedTemplateTemporary, CheckedTemplateTemporaryId,
    CheckedTemplateTrustedObligation, CheckedTemplateWitness,
};
pub use tree::{
    BoundTree, BoundTreeBuildError, BoundTreeBuilder, BoundTreeCheckpoint, BoundWalkControl,
    BoundWalkEvent, BoundWalkOutcome, walk_bound_tree,
};
pub use unit::{
    AnonymousCallableUnitKey, BoundSourceAnchor, BoundUnitId, BoundUnitIdentity, BoundUnitKey,
    BoundUnitKeyData, BoundUnitKind, DeclaredBoundUnitKey,
};
pub use view::BoundUnitView;
