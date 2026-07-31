//! Source-correlated semantic trees after name binding.

#![forbid(unsafe_code)]

mod asynchronous;
mod behavior;
mod bound_unit;
mod control;
mod declared_type;
mod dependency;
mod dependency_table;
mod identity;
mod literal;
mod memory;
mod node;
mod origin;
mod pattern;
mod refinement;
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

pub use asynchronous::{
    AsyncFactsBuildError, AsyncScopeExitPlan, AsyncSuspensionPoint, AsyncTaskOperation,
    AsyncTaskOperationKind, CheckedAsyncFacts,
};
pub use behavior::{
    BodyBehaviorCall, BodyBehaviorContributions, BodyBehaviorPhase, CheckedBodyBehavior,
};
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
pub use dependency_table::{CheckedDependencyContracts, DependencyContractsBuildError};
pub use literal::{
    CheckedLiteralValueEntry, CheckedLiteralValueTableBuildError, CheckedLiteralValues,
};
pub use memory::{
    CheckedMemoryOperation, CheckedMemoryOperationKind, CheckedMemoryOperations,
    CheckedMemoryOperationsBuildError, MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind,
    MemoryOffsetUnit, MemoryReadKind,
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
    BoundPatternId, BoundPatternKind, BoundPatternLiteral, BoundPatternMode,
    BoundPatternReferenceExpression, BoundPatternTarget, BoundReferenceTarget, BoundResolvedCall,
    BoundSliceBounds, BoundStructConstructionExpression, BoundStructFieldInitializer,
    BoundStructuredExpression, BoundStructuredExpressionKind, BoundTraitQualifiedMemberExpression,
    BoundTypeReference, BoundUnaryExpression, BoundUnresolvedReferenceExpression,
    BoundUnresolvedReferenceKind, ExactBoundNodeId, IterationSourceMode,
};
pub use origin::{
    BoundNodeOrdinal, BoundNodeOrigin, BoundSynthesisRole, SynthesizedBoundNodeOrigin,
};
pub use pattern::{
    CheckedPatternFacts, MatchCoverageEntry, PatternBindingTypeEntry, PatternCheckEntry,
    PatternOperation, PatternPredicate, PatternProjection, PatternRefutability,
};
pub use refinement::{
    CheckedRefinementFacts, RefinementFact, RefinementFactKind, RefinementFactsBuildError,
    RefinementOccurrence,
};
pub use selection::{
    CheckedSemanticSelections, ConstructionDefaultProvider, ConstructionInputId,
    ConstructionTarget, ConversionTarget, IndexTarget, MemberTarget, OperatorTarget,
    SelectedArgument, SelectedCall, SelectedConstruction, SelectedConstructionInput,
    SelectedConversion, SelectedImplementationWitness, SelectedIterationProtocolOperation,
    SelectedIterationSource, SelectedIterationTypes, SelectedOperation, SelectedPropagation,
    SelectedPropagationBoundary, SelectedReceiver, SelectionKind, SemanticSelection,
    SemanticSelectionEntry, SemanticSelectionTableBuildError,
};
pub use storage::{
    BorrowCapabilityId, BorrowCapabilityOrigin, LastUse, LiveAcrossScope, LiveAcrossSuspension,
    LivenessFacts, LivenessFactsBuildError, PlannedBorrowCapability, StorageAccess,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot,
    StorageAlternative, StorageAlternativeId, StorageBinding, StorageBindingTarget,
    StorageExitDecision, StorageFlowFacts, StorageFlowFactsBuildError, StorageIdentity,
    StorageIdentityId, StorageOperationDecision, StorageOperationStatus, StoragePlan,
    StoragePlanBuildError, StoragePlanBuilder, StorageProjection, StorageRelationship,
    StorageSuspensionState,
};
pub use template::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateBuildError, CheckedTemplateBuilder,
    CheckedTemplateCapability, CheckedTemplateCompletion, CheckedTemplateConstantUsage,
    CheckedTemplateEffect, CheckedTemplateExecution, CheckedTemplateExecutionRequirement,
    CheckedTemplateInput, CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind,
    CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation,
    CheckedTemplateShortCircuitKind, CheckedTemplateTemporary, CheckedTemplateTemporaryId,
    CheckedTemplateTrustedObligation, CheckedTemplateWitness,
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
