//! Structured diagnostics and diagnostic rendering data.

#![forbid(unsafe_code)]

mod argument;
mod bag;
mod checking;
mod code;
mod diagnostic;
mod emission;
mod id;
mod inspection;
mod interface;
mod kind;
mod label;
mod linker;
mod note;
mod project;
mod quality;
mod related;
mod result;
mod severity;
mod source_input;
mod storage;
mod suggestion;

pub use storage::{DiagnosticRetainedGenerationProblem, DiagnosticStorageOperation};
mod target_predicate;
mod type_representation;

pub use argument::{
    DiagnosticAlignmentKind, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue,
    DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm, DiagnosticArtifactKind,
    DiagnosticArtifactRequirement, DiagnosticAssemblySyntaxKind, DiagnosticCallableAbi,
    DiagnosticDebugInformationMode, DiagnosticDebugOutputMode, DiagnosticDependencyRequirementKind,
    DiagnosticDependencySubjectKind, DiagnosticDocumentParseKind,
    DiagnosticEmissionArtifactOperation, DiagnosticExternalToolFailureKind,
    DiagnosticExternalToolOperation, DiagnosticIoErrorKind, DiagnosticLinkInputKind,
    DiagnosticLinkOptimizationReportProblem, DiagnosticLinkRequirement,
    DiagnosticLinkRequirementKind, DiagnosticLinkedArtifactKind, DiagnosticLinkedProductKind,
    DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticNamedType,
    DiagnosticNativeLinkInputFailure, DiagnosticNativeProductFailureDetail,
    DiagnosticNativeProductFailureKind, DiagnosticOutputSink, DiagnosticProductKind,
    DiagnosticRuntimeAbiVersion, DiagnosticRuntimeArtifactProblem,
    DiagnosticRuntimeArtifactPurpose, DiagnosticSelectionKind,
    DiagnosticStandardLibraryManifestProblem, DiagnosticStandardLibraryOptimizationMetadataProblem,
    DiagnosticTargetRepresentation, DiagnosticType, DiagnosticTypeArgument, DiagnosticVisibility,
};
pub use bag::DiagnosticBag;
pub use checking::{
    DiagnosticArrayGeneratorCardinalityProblem, DiagnosticArrayLength,
    DiagnosticCallableArgumentRejection, DiagnosticCallableBehaviorComponent,
    DiagnosticCallableBehaviorPhase, DiagnosticCallableConstness,
    DiagnosticCallableContractClauseCategory, DiagnosticCallableContractMismatch,
    DiagnosticCallableContractSurface, DiagnosticCallableExecution, DiagnosticCallableOverloadArm,
    DiagnosticCallableOverloadContext, DiagnosticCallableOverloadProblem,
    DiagnosticCallableParameterMode, DiagnosticCallablePosition, DiagnosticCallableTrust,
    DiagnosticCallbackStateProblem, DiagnosticConstantOperation, DiagnosticConstraintCategory,
    DiagnosticConstructionInputRejection, DiagnosticDirectiveArgumentProblem,
    DiagnosticExpressionCategory, DiagnosticGenericConstraintMismatch,
    DiagnosticGenericParameterCategory, DiagnosticImplementationBorrowKind,
    DiagnosticImplementationFamily, DiagnosticImplementationFamilySubject,
    DiagnosticImplementationOverloadProblem, DiagnosticMemoryOperation,
    DiagnosticNativeLinkDirectiveProblem, DiagnosticNativeLinkKind,
    DiagnosticNativeSymbolDirectiveProblem, DiagnosticPatternCoverage,
    DiagnosticPatternMissingCase, DiagnosticPatternUnreachability, DiagnosticPlatformAbiType,
    DiagnosticPlatformServiceRole, DiagnosticPlatformServiceSignatureProblem,
    DiagnosticPropagationProblem, DiagnosticReceiverCapability, DiagnosticReceiverMode,
    DiagnosticRefinementCapacity, DiagnosticRefinementCapacitySurface,
    DiagnosticRejectedSelectionCandidate, DiagnosticSelectionCandidate,
    DiagnosticSelectionCandidateIdentity, DiagnosticSelectionCandidateSignature,
    DiagnosticSelectionCandidates, DiagnosticSelectionCandidatesBuildError,
    DiagnosticSelectionRejectionReason, DiagnosticSelectionRejections, DiagnosticStorageAccess,
    DiagnosticStorageAccessPurpose, DiagnosticStorageProjection, DiagnosticStorageRoot,
    DiagnosticTraitFulfillmentMismatch, DiagnosticYieldCardinality,
};
pub use code::DiagnosticCode;
pub use diagnostic::Diagnostic;
pub use emission::{
    DiagnosticBindingFailure, DiagnosticCheckerConstantOperationFailure, DiagnosticCheckerFailure,
    DiagnosticCheckerLocal, DiagnosticCheckerNode, DiagnosticCheckerSymbol,
    DiagnosticConstantEvaluationFailure, DiagnosticEmissionArtifact,
    DiagnosticEmissionCodegenFailure, DiagnosticEmissionEvaluationFailure,
    DiagnosticEmissionFailure, DiagnosticEmissionLinkPlanFailure,
    DiagnosticEmissionPlanningFailure, DiagnosticEmissionStagingFailure, DiagnosticNativeInspectionFailure,
    DiagnosticEvaluationFailureDetail, DiagnosticFactRuntimeFailure, DiagnosticFailureField,
    DiagnosticFailureValue, DiagnosticForeignQueryFailure, DiagnosticGenericSubstitutionFailure,
    DiagnosticHostEnvironmentVariable, DiagnosticLiteralValueFailure, DiagnosticLivenessFailure,
    DiagnosticLlvmToolRole, DiagnosticMemoryOperationsFailure, DiagnosticPackageInterfaceFailure,
    DiagnosticProductQueryFailure, DiagnosticSemanticQueryFailure,
    DiagnosticSemanticSelectionFailure, DiagnosticSemanticSnapshotFailure,
    DiagnosticSemanticValueFailure, DiagnosticStorageFlowFailure, DiagnosticStoragePlanFailure,
    DiagnosticTestCatalogFailure, DiagnosticUnsupportedEmissionReason,
};
pub use id::DiagnosticId;
pub use inspection::{
    DiagnosticBoundInspectionFailure, DiagnosticDeclarationInspectionFailure,
    DiagnosticInspectionFailure, DiagnosticInspectionFailureDetail,
    DiagnosticInspectionOutputFormat, DiagnosticInspectionTarget,
    DiagnosticLoweredInspectionFailure, DiagnosticSourceInspectionFailure,
    DiagnosticSymbolInspectionFailure, DiagnosticSyntaxInspectionFailure,
    DiagnosticTokenInspectionFailure,
};
pub use interface::{
    DiagnosticCheckedTemplateProblem, DiagnosticInterfaceCompressionFailure,
    DiagnosticInterfaceDeclarationIdentity, DiagnosticInterfaceDependency,
    DiagnosticInterfaceIdentitySurfaceProblem, DiagnosticInterfaceIntegerTarget,
    DiagnosticInterfaceLimit, DiagnosticInterfaceMalformedCause, DiagnosticInterfaceProductKind,
    DiagnosticInterfaceRelationship, DiagnosticInterfaceRelationshipKind,
    DiagnosticInterfaceSection, DiagnosticInterfaceSemanticProblem,
    DiagnosticInterfaceSemanticRecordKind, DiagnosticInterfaceSymbolGraphProblem,
    DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSymbolKind,
    DiagnosticInterfaceSymbolReference, DiagnosticInterfaceSynthesizedIdentity,
    DiagnosticInterfaceUtf8Failure, DiagnosticInterfaceValidationContext,
    DiagnosticInterfaceValidationFailure, DiagnosticInterfaceValidationField,
    DiagnosticNativeArtifactCause,
    DiagnosticPackageInterfaceIdentity, DiagnosticSemanticContentProblem,
    DiagnosticSemanticValueKind,
};
pub use kind::DiagnosticKind;
pub use label::{DiagnosticLabel, DiagnosticLabelKind, DiagnosticLabelStyle};
pub use linker::{
    DiagnosticExternalToolExit, DiagnosticExternalToolStreamCapture,
    DiagnosticLinkerDriverIdentity, DiagnosticLinkerDriverKind,
};
pub use note::{DiagnosticNote, DiagnosticNoteKind};
pub use project::{
    DiagnosticInvocationBuildFailure, DiagnosticLinkerCapabilityBuildFailure,
    DiagnosticNativeLinkerBuildFailure, DiagnosticPathRequirement,
    DiagnosticProfileComparisonProblem, DiagnosticProfileContext, DiagnosticProfileDescriptorKind,
    DiagnosticProfileValidationProblem, DiagnosticProjectCommandFailure,
    DiagnosticProjectDependencyCycleMember, DiagnosticProjectManifestField,
    DiagnosticProjectOperation, DiagnosticProjectProcessFailure, DiagnosticProjectSelectionProblem,
    DiagnosticReusableBuildIdentityPart, DiagnosticTestExecutionPlanProblem,
    DiagnosticTestSchedulingProblem, DiagnosticToolProtocolFailure, DiagnosticToolStream,
};
pub use quality::{DiagnosticQualityContract, DiagnosticQualityIssue};
pub use related::{DiagnosticRelatedLocation, DiagnosticRelatedLocationKind};
pub use result::DiagnosticResult;
pub use severity::SeverityKind;
pub use source_input::{DiagnosticSourceInput, DiagnosticSourceInputOrigin};
pub use suggestion::{
    DiagnosticSourceEdit, DiagnosticSuggestion, DiagnosticSuggestionApplicability,
    DiagnosticSuggestionBuildError, DiagnosticSuggestionKind,
};
pub use target_predicate::DiagnosticTargetPredicateValueKind;
pub use type_representation::{
    DiagnosticCopyContractProblem, DiagnosticLayoutOption, DiagnosticLayoutProblem,
    DiagnosticStoredTypeProblem, DiagnosticUnionTagProblem,
};
