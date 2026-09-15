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
mod lifecycle;
mod literal;
mod memory;
mod node;
mod origin;
mod pattern;
mod refinement;
mod selection;
mod semantic;
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
    AsyncAnalysisBuildError, AsyncCleanupGuard, AsyncCleanupPhases, AsyncScopeExitPlan,
    AsyncStorageCleanupRequirement, AsyncStorageExitDecision, AsyncStorageExitDisposition,
    AsyncStorageExitRecoveryCause, AsyncStorageRequirement, AsyncSuspensionKind,
    AsyncSuspensionPoint, AsyncTaskOperation, AsyncTaskOperationKind, CheckedAsync,
};
pub use behavior::{
    BodyBehaviorCall, BodyBehaviorContributions, BodyBehaviorPhase, CheckedBodyBehavior,
    TrustedCapabilityUse,
};
pub use bound_unit::{BoundUnit, BoundUnitBuildError, BoundUnitRoot};
pub use control::{CheckedControlFlow, ControlCompletion, ControlCompletionKind};
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
pub use lifecycle::{LifecycleAction, LifecycleCallable, LifecyclePhase};
pub use literal::{
    CheckedLiteralValueEntry, CheckedLiteralValueTableBuildError, CheckedLiteralValues,
};
pub use memory::{
    AtomicFetchKind, CheckedMemoryOperation, CheckedMemoryOperationKind, CheckedMemoryOperations,
    CheckedMemoryOperationsBuildError, InlineAssemblyConstraint, InlineAssemblyContract,
    InlineAssemblyOperand, InlineAssemblyOperandKind, InlineAssemblySymbol,
    MAX_INLINE_ASSEMBLY_OPERANDS, MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind,
    MemoryOffsetUnit, MemoryOperationDecision, MemoryOperationStatus, MemoryOrder, MemoryReadKind,
    PointerAddressComparison, VolatileAddressSpace,
};
pub use node::{
    AnyBoundNodeId, BoundAnonymousCallableExpression, BoundArgument, BoundAssignmentExpression,
    BoundAssignmentOperator, BoundAwaitExpression, BoundAwaitResolution, BoundBinaryExpression,
    BoundBlock, BoundBlockExpression, BoundBlockId, BoundBlockItem, BoundBoxConstructionExpression,
    BoundCallExpression, BoundCallResolution, BoundCallResult, BoundCallableBody,
    BoundCallableBodyId, BoundCallableBodyKind, BoundCallableTarget,
    BoundControlTransferExpression, BoundControlTransferKind, BoundConversionExpression,
    BoundErrorCallExpression, BoundErrorCallableBody, BoundErrorConversionExpression,
    BoundErrorExpression, BoundExpression, BoundExpressionId, BoundForExpression,
    BoundFutureComposition, BoundFutureConstruction, BoundGeneratorExpression,
    BoundGenericArgument, BoundIterationSource, BoundLeadingDotVariantExpression,
    BoundLiteralExpression, BoundLiteralKind, BoundLocalBinding, BoundLocalConstant, BoundMatchArm,
    BoundMatchExpression, BoundMemberAccessExpression, BoundMemberSelector, BoundNameExpression,
    BoundNodeKind, BoundOperator, BoundPattern, BoundPatternEntry, BoundPatternEntryKind,
    BoundPatternId, BoundPatternKind, BoundPatternLiteral, BoundPatternMode,
    BoundPatternReferenceExpression, BoundPatternTarget, BoundReferenceTarget, BoundResolvedCall,
    BoundSliceBounds, BoundStructConstructionExpression, BoundStructFieldInitializer,
    BoundStructuredExpression, BoundStructuredExpressionKind, BoundTraitQualifiedMemberExpression,
    BoundTypeReference, BoundUnaryExpression, BoundUnqualifiedVariantExpression,
    BoundUnresolvedReferenceExpression, BoundUnresolvedReferenceKind, ExactBoundNodeId,
    IterationSourceMode,
};
pub use origin::{
    BoundNodeOrdinal, BoundNodeOrigin, BoundSynthesisRole, SynthesizedBoundNodeOrigin,
};
pub use pattern::{
    CheckedPatterns, MatchCoverageEntry, PatternBindingTypeEntry, PatternCheckEntry,
    PatternLiteralPredicate, PatternOperation, PatternPredicate, PatternProjection,
    PatternRefutability,
};
pub use refinement::{
    CheckedRefinements, Refinement, RefinementKind, RefinementOccurrence, RefinementSetBuildError,
};
pub use selection::{
    CallableDeclarationTemplate, CallableParameterDefaultTemplate, CheckedSemanticSelections,
    ConstructionInputId, ConstructionTarget, ConversionTarget, DefaultValueProvider, IndexTarget,
    MemberTarget, OperatorTarget, SelectedArgument, SelectedCall, SelectedCompoundAssignment,
    SelectedConstruction, SelectedConstructionInput, SelectedConversion,
    SelectedImplementationWitness, SelectedIterationProtocolOperation, SelectedIterationSource,
    SelectedIterationTypes, SelectedOperation, SelectedPredicateApplication,
    SelectedPredicateArgument, SelectedPropagation, SelectedPropagationBoundary, SelectedReceiver,
    SelectionKind, SemanticSelection, SemanticSelectionEntry, SemanticSelectionTableBuildError,
};
pub use semantic::{
    CheckedBodySemantics, CheckedExpressionSemantics, SemanticSnapshotBuildError,
    SemanticSnapshotInputKind,
};
pub use storage::{
    BorrowCapabilityId, BorrowCapabilityOrigin, LastUse, LiveAcrossScope, LiveAcrossSuspension,
    Liveness, LivenessBuildError, OwnerRetention, PlannedBorrowCapability, StorageAccess,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot,
    StorageAlternative, StorageAlternativeId, StorageBinding, StorageBindingTarget,
    StorageCleanupPart, StorageCleanupProjection, StorageCleanupProjectionKind, StorageCleanupType,
    StorageExitDecision, StorageExitPoint, StorageFlow, StorageFlowBuildError, StorageIdentity,
    StorageIdentityId, StorageOperationDecision, StorageOperationStatus, StoragePlan,
    StoragePlanBuildError, StoragePlanBuilder, StorageProjection, StorageProtocolCall,
    StorageRelationship, StorageReplacementDecision, StorageReplacementPlan,
    StorageReplacementState, StorageScopeBuildError, StorageScopeOwners, StorageSuspensionState,
    storage_expression_republishes_destructor_receiver, storage_identity_is_destructor_receiver,
    storage_identity_transfers_at_unit_exit,
};
pub use template::{
    CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateBuildError, CheckedTemplateBuilder,
    CheckedTemplateCapability, CheckedTemplateCompletion, CheckedTemplateConstantUsage,
    CheckedTemplateEffect, CheckedTemplateExecution, CheckedTemplateExecutionRequirement,
    CheckedTemplateIndexCall, CheckedTemplateIndexDispatch, CheckedTemplateInput,
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
