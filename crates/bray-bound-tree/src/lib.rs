//! Source-correlated semantic trees after name binding.

#![forbid(unsafe_code)]

mod bound_unit;
mod control;
mod declared_type;
mod dependency;
mod identity;
mod node;
mod origin;
mod pattern;
mod selection;
mod storage;
mod template;
#[cfg(test)]
mod test_support;
#[cfg(any(test, feature = "test-support"))]
pub mod testing;
mod tree;
mod typing;
mod unit;
mod view;

pub use bound_unit::{BoundUnit, BoundUnitBuildError, BoundUnitRoot};
pub use control::{CheckedControlFlowFacts, ControlCompletion, ControlCompletionKind};
pub use declared_type::{
    DeclaredValueTypeConstraint, DeclaredValueTypeConstraintKind, DeclaredValueTypeEvidence,
    DeclaredValueTypeTemplates, DeclaredValueTypeTerm,
};
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
    BoundGenericArgument, BoundIterationSource, BoundLeadingDotVariantExpression,
    BoundLiteralExpression, BoundLiteralKind, BoundLocalBinding, BoundLocalConstant, BoundMatchArm,
    BoundMatchExpression, BoundMemberAccessExpression, BoundMemberSelector, BoundNameExpression,
    BoundNodeKind, BoundOperator, BoundPattern, BoundPatternEntry, BoundPatternEntryKind,
    BoundPatternId, BoundPatternKind, BoundPatternLiteral, BoundPatternMode, BoundPatternTarget,
    BoundReferenceTarget, BoundResolvedCall, BoundSliceBounds, BoundStructConstructionExpression,
    BoundStructFieldInitializer, BoundStructuredExpression, BoundStructuredExpressionKind,
    BoundTraitQualifiedMemberExpression, BoundTypeReference, BoundUnaryExpression,
    BoundUnresolvedReferenceExpression, BoundUnresolvedReferenceKind, ExactBoundNodeId,
    IterationSourceMode,
};
pub use origin::{
    BoundNodeOrdinal, BoundNodeOrigin, BoundSynthesisRole, SynthesizedBoundNodeOrigin,
};
pub use pattern::{
    CheckedPatternFacts, MatchCoverageEntry, PatternBindingTypeEntry, PatternCheckEntry,
    PatternOperation, PatternPredicate, PatternProjection, PatternRefutability,
};
pub use selection::{
    CheckedSemanticSelections, ConstructionDefaultProvider, ConstructionInputId,
    ConstructionTarget, ConversionTarget, IndexTarget, MemberTarget, OperatorTarget,
    SelectedArgument, SelectedCall, SelectedConstruction, SelectedConstructionInput,
    SelectedConversion, SelectedImplementationWitness, SelectedIterationProtocolOperation,
    SelectedIterationSource, SelectedIterationTypes, SelectedOperation, SelectionKind,
    SemanticSelection, SemanticSelectionEntry, SemanticSelectionTableBuildError,
};
pub use storage::{
    BorrowCapability, BorrowCapabilityId, StorageAccess, StorageAccessId, StorageAccessRoot,
    StorageIdentity, StorageIdentityId, StorageProjection, StorageRelationship,
};
pub use template::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateBuildError, CheckedTemplateBuilder,
    CheckedTemplateCapability, CheckedTemplateCompletion, CheckedTemplateEffect,
    CheckedTemplateExecution, CheckedTemplateExecutionRequirement, CheckedTemplateInput,
    CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind, CheckedTemplateNode,
    CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateShortCircuitKind,
    CheckedTemplateTemporary, CheckedTemplateTemporaryId, CheckedTemplateTrustedObligation,
    CheckedTemplateWitness,
};
pub use tree::{
    BoundTree, BoundTreeBuildError, BoundTreeBuilder, BoundTreeCheckpoint, BoundWalkControl,
    BoundWalkEvent, BoundWalkOutcome, walk_bound_tree, walk_bound_unit_view,
};
pub use typing::{
    CheckedExpressionTypes, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
};
pub use unit::{
    AnonymousCallableUnitKey, BoundSourceAnchor, BoundUnitId, BoundUnitIdentity, BoundUnitKey,
    BoundUnitKeyData, BoundUnitKind, DeclaredBoundUnitKey,
};
pub use view::BoundUnitView;
