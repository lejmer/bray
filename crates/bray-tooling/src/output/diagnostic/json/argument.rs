use bray_diagnostics::DiagnosticArgValue;
use serde::Serialize;

use super::emission::{
    DiagnosticEmissionFieldJson, checker_failure_context, diagnostic_failure_context,
    fact_runtime_failure_context, foreign_query_failure_context, lowering_failure_context,
    lowering_input_failure_context, native_link_input_failure_context,
    product_query_failure_context, semantic_value_failure_context, text_field,
};
use super::{
    DiagnosticArtifactDigestJson, DiagnosticCallableOverloadProblemJson,
    DiagnosticEmissionFailureJson, DiagnosticExternalToolExitJson,
    DiagnosticImplementationOverloadProblemJson, DiagnosticInterfaceSymbolIdentityJson,
    DiagnosticLinkRequirementJson, DiagnosticLinkerDriverIdentityJson, DiagnosticOutputSinkJson,
    DiagnosticPatternCoverageJson, DiagnosticProblemJson, DiagnosticProjectCommandFailureJson,
    DiagnosticProjectDependencyCycleMemberJson, DiagnosticProjectSelectionJson,
    DiagnosticRuntimeAbiVersionJson, DiagnosticRuntimeArtifactProblemJson,
    DiagnosticSelectionCandidatesJson, DiagnosticSelectionRejectionsJson,
    DiagnosticSourceInputJson, DiagnosticStorageAccessJson, DiagnosticTraitFulfillmentMismatchJson,
    DiagnosticTypeJson, DiagnosticUnsupportedEmissionReasonJson, SourceSpanJson,
    array_generator_problem_json, callback_state_problem_json, copy_contract_problem_json,
    interface_semantic_problem_json, interface_symbol_graph_problem_json,
    interface_validation_failure_json, layout_problem_json, native_link_directive_problem_json,
    native_symbol_directive_problem_json, platform_service_signature_problem_json,
    propagation_problem_json, refinement_capacity_json, union_tag_problem_json,
};
use crate::output::diagnostic::source_map::DiagnosticSourceMap;
use crate::output::path_to_output_string;

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticArgValueJson {
    Count(u64),
    Byte(u8),
    ByteCount(u64),
    ArtifactDigest(DiagnosticArtifactDigestJson),
    ArtifactKind(&'static str),
    ArtifactOrdinal(u32),
    TargetRepresentation(&'static str),
    CallableAbi(&'static str),
    AlignmentKind(&'static str),
    Character(char),
    DeclarationName(String),
    ReferencedName(String),
    CodegenBackendIdentity(String),
    CodegenVerificationStage(&'static str),
    CodegenBackendReport(String),
    LinkerDriverIdentity(DiagnosticLinkerDriverIdentityJson),
    PackageIdentity(String),
    ProductIdentity(String),
    TargetTriple(String),
    TargetIdentity(String),
    NativeProductFailureKind(DiagnosticNativeProductFailureJson),
    EmissionFailure(DiagnosticEmissionFailureJson),
    EmissionArtifactOperation(&'static str),
    StorageOperation(&'static str),
    RetainedGenerationProblem {
        problem: &'static str,
        expected_revision: Option<u32>,
        actual_revision: Option<u32>,
        expected_read_only: Option<bool>,
        actual_read_only: Option<bool>,
        expected_unix_mode: Option<u32>,
        actual_unix_mode: Option<u32>,
    },
    LinkInputKind(&'static str),
    LinkedArtifactKind(&'static str),
    LinkedProductKind(&'static str),
    ProductKind(&'static str),
    SymbolKind(String),
    DebugInformationMode(&'static str),
    DebugOutputMode(&'static str),
    AssemblySyntax(&'static str),
    ArtifactRequirement(&'static str),
    NameKind(&'static str),
    FilePath(String),
    InputIndex(u64),
    InterfaceLimit(&'static str),
    InterfaceSection(&'static str),
    InterfaceSymbolIdentity(DiagnosticInterfaceSymbolIdentityJson),
    InterfaceSymbolGraphProblem(DiagnosticProblemJson),
    InterfaceSemanticProblem(DiagnosticProblemJson),
    InterfaceValidationFailure(DiagnosticProblemJson),
    ProjectSelectionProblem(DiagnosticProjectSelectionJson),
    ProjectCommandFailure(DiagnosticProjectCommandFailureJson),
    ProjectManifestField(&'static str),
    ProjectDependencyCycleMember(DiagnosticProjectDependencyCycleMemberJson),
    TargetPredicateValueKind(&'static str),
    UnsupportedEmissionReason(DiagnosticUnsupportedEmissionReasonJson),
    IoErrorKind(&'static str),
    ExternalToolOperation(&'static str),
    ExternalToolFailureKind(&'static str),
    ExternalToolExit(DiagnosticExternalToolExitJson),
    LinkRequirement(DiagnosticLinkRequirementJson),
    LinkOptimizationReportProblem(&'static str),
    DocumentParseKind(&'static str),
    StandardLibraryManifestProblem(&'static str),
    DependencySubjectKind(&'static str),
    DependencyRequirementKind(&'static str),
    OutputSink(DiagnosticOutputSinkJson),
    Visibility(&'static str),
    ModuleTrust(&'static str),
    SourceName(String),
    SourceCount(u64),
    SourceInputKind(&'static str),
    SourceInput(DiagnosticSourceInputJson),
    SyntaxKind(&'static str),
    TextOffset(u32),
    TokenText(String),
    Uri(String),
    SourceSpan(SourceSpanJson),
    WorkerCount(u64),
    Revision(u64),
    RuntimeAbi(DiagnosticRuntimeAbiVersionJson),
    RuntimeArtifactProblem(DiagnosticRuntimeArtifactProblemJson),
    Type(DiagnosticTypeJson),
    SelectionKind(&'static str),
    SelectionCandidates(DiagnosticSelectionCandidatesJson),
    SelectionRejections(DiagnosticSelectionRejectionsJson),
    NativeLinkDirectiveProblem(DiagnosticProblemJson),
    NativeSymbolDirectiveProblem(DiagnosticProblemJson),
    PlatformServiceSignatureProblem(DiagnosticProblemJson),
    TraitFulfillmentMismatch(DiagnosticTraitFulfillmentMismatchJson),
    ImplementationOverloadProblem(DiagnosticImplementationOverloadProblemJson),
    CallableOverloadProblem(DiagnosticCallableOverloadProblemJson),
    ExpressionCategory(&'static str),
    ConstantOperation(&'static str),
    LayoutProblem(DiagnosticProblemJson),
    UnionTagProblem(DiagnosticProblemJson),
    CopyContractProblem(DiagnosticProblemJson),
    StoredTypeProblem(DiagnosticProblemJson),
    PropagationProblem(DiagnosticProblemJson),
    ArrayGeneratorCardinalityProblem(DiagnosticProblemJson),
    RefinementCapacity(DiagnosticProblemJson),
    MemoryOperation(&'static str),
    CallbackStateProblem(DiagnosticProblemJson),
    StorageAccess(DiagnosticStorageAccessJson),
    PatternCoverage(DiagnosticPatternCoverageJson),
    PatternUnreachability(&'static str),
}

impl DiagnosticArgValueJson {
    pub(in crate::output::diagnostic::json) fn from_value(
        value: &DiagnosticArgValue,
        source_map: &DiagnosticSourceMap<'_>,
    ) -> Self {
        match value {
            DiagnosticArgValue::Count(count) => Self::Count(*count),
            DiagnosticArgValue::Byte(byte) => Self::Byte(*byte),
            DiagnosticArgValue::ByteCount(byte_count) => Self::ByteCount(*byte_count),
            DiagnosticArgValue::ArtifactDigest(digest) => {
                Self::ArtifactDigest(DiagnosticArtifactDigestJson::from_digest(digest))
            }
            DiagnosticArgValue::ArtifactKind(kind) => Self::ArtifactKind((*kind).as_str()),
            DiagnosticArgValue::ArtifactOrdinal(ordinal) => Self::ArtifactOrdinal(*ordinal),
            DiagnosticArgValue::TargetRepresentation(kind) => {
                Self::TargetRepresentation((*kind).as_str())
            }
            DiagnosticArgValue::CallableAbi(abi) => Self::CallableAbi((*abi).as_str()),
            DiagnosticArgValue::AlignmentKind(kind) => Self::AlignmentKind((*kind).as_str()),
            DiagnosticArgValue::Character(character) => Self::Character(*character),
            DiagnosticArgValue::DeclarationName(name) => Self::DeclarationName(name.to_owned()),
            DiagnosticArgValue::ReferencedName(name) => Self::ReferencedName(name.to_owned()),
            DiagnosticArgValue::CodegenBackendIdentity(identity) => {
                Self::CodegenBackendIdentity(identity.to_owned())
            }
            DiagnosticArgValue::CodegenVerificationStage(stage) => {
                Self::CodegenVerificationStage((*stage).as_str())
            }
            DiagnosticArgValue::CodegenBackendReport(report) => {
                Self::CodegenBackendReport(report.to_owned())
            }
            DiagnosticArgValue::LinkerDriverIdentity(identity) => Self::LinkerDriverIdentity(
                DiagnosticLinkerDriverIdentityJson::from_identity(identity),
            ),
            DiagnosticArgValue::PackageIdentity(identity) => {
                Self::PackageIdentity(identity.to_owned())
            }
            DiagnosticArgValue::ProductIdentity(identity) => {
                Self::ProductIdentity(identity.to_owned())
            }
            DiagnosticArgValue::SymbolKind(kind) => Self::SymbolKind(kind.to_owned()),
            DiagnosticArgValue::TargetTriple(target) => Self::TargetTriple(target.to_owned()),
            DiagnosticArgValue::TargetIdentity(target) => Self::TargetIdentity(target.to_owned()),
            DiagnosticArgValue::NativeProductFailureKind(kind) => {
                Self::NativeProductFailureKind(DiagnosticNativeProductFailureJson::from_kind(kind))
            }
            DiagnosticArgValue::EmissionFailure(failure) => {
                Self::EmissionFailure(DiagnosticEmissionFailureJson::from_failure(failure))
            }
            DiagnosticArgValue::RetainedGenerationProblem(problem) => {
                retained_generation_problem_json(problem)
            }
            DiagnosticArgValue::StorageOperation(operation) => {
                Self::StorageOperation(operation.as_str())
            }
            DiagnosticArgValue::EmissionArtifactOperation(kind) => {
                Self::EmissionArtifactOperation((*kind).as_str())
            }
            DiagnosticArgValue::LinkInputKind(kind) => Self::LinkInputKind((*kind).as_str()),
            DiagnosticArgValue::LinkedArtifactKind(kind) => {
                Self::LinkedArtifactKind((*kind).as_str())
            }
            DiagnosticArgValue::LinkedProductKind(kind) => {
                Self::LinkedProductKind((*kind).as_str())
            }
            DiagnosticArgValue::ProductKind(kind) => Self::ProductKind((*kind).as_str()),
            DiagnosticArgValue::DebugInformationMode(kind) => {
                Self::DebugInformationMode((*kind).as_str())
            }
            DiagnosticArgValue::DebugOutputMode(kind) => Self::DebugOutputMode((*kind).as_str()),
            DiagnosticArgValue::AssemblySyntax(kind) => Self::AssemblySyntax((*kind).as_str()),
            DiagnosticArgValue::ArtifactRequirement(kind) => {
                Self::ArtifactRequirement((*kind).as_str())
            }
            DiagnosticArgValue::NameKind(kind) => Self::NameKind((*kind).as_str()),
            DiagnosticArgValue::FilePath(path) => Self::FilePath(path_to_output_string(path)),
            DiagnosticArgValue::InputIndex(input_index) => Self::InputIndex(*input_index),
            DiagnosticArgValue::InterfaceLimit(limit) => Self::InterfaceLimit((*limit).as_str()),
            DiagnosticArgValue::InterfaceSection(section) => {
                Self::InterfaceSection((*section).as_str())
            }
            DiagnosticArgValue::InterfaceSymbolIdentity(identity) => Self::InterfaceSymbolIdentity(
                DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            ),
            DiagnosticArgValue::InterfaceSymbolGraphProblem(problem) => {
                Self::InterfaceSymbolGraphProblem(interface_symbol_graph_problem_json(problem))
            }
            DiagnosticArgValue::InterfaceSemanticProblem(problem) => {
                Self::InterfaceSemanticProblem(interface_semantic_problem_json(problem))
            }
            DiagnosticArgValue::InterfaceValidationFailure(failure) => {
                Self::InterfaceValidationFailure(interface_validation_failure_json(failure))
            }
            DiagnosticArgValue::ProjectSelectionProblem(problem) => {
                Self::ProjectSelectionProblem(DiagnosticProjectSelectionJson::from_problem(problem))
            }
            DiagnosticArgValue::ProjectCommandFailure(failure) => Self::ProjectCommandFailure(
                DiagnosticProjectCommandFailureJson::from_failure(failure),
            ),
            DiagnosticArgValue::ProjectManifestField(field) => {
                Self::ProjectManifestField((*field).as_str())
            }
            DiagnosticArgValue::ProjectDependencyCycleMember(member) => {
                Self::ProjectDependencyCycleMember(
                    DiagnosticProjectDependencyCycleMemberJson::from_member(member),
                )
            }
            DiagnosticArgValue::TargetPredicateValueKind(kind) => {
                Self::TargetPredicateValueKind((*kind).as_str())
            }
            DiagnosticArgValue::UnsupportedEmissionReason(reason) => {
                Self::UnsupportedEmissionReason(
                    DiagnosticUnsupportedEmissionReasonJson::from_reason(reason),
                )
            }
            DiagnosticArgValue::IoErrorKind(kind) => Self::IoErrorKind((*kind).as_str()),
            DiagnosticArgValue::ExternalToolOperation(kind) => {
                Self::ExternalToolOperation((*kind).as_str())
            }
            DiagnosticArgValue::ExternalToolFailureKind(kind) => {
                Self::ExternalToolFailureKind((*kind).as_str())
            }
            DiagnosticArgValue::ExternalToolExit(exit) => {
                Self::ExternalToolExit(DiagnosticExternalToolExitJson::from_exit(exit))
            }
            DiagnosticArgValue::LinkRequirement(requirement) => {
                Self::LinkRequirement(DiagnosticLinkRequirementJson::from_requirement(requirement))
            }
            DiagnosticArgValue::LinkOptimizationReportProblem(problem) => {
                Self::LinkOptimizationReportProblem((*problem).as_str())
            }
            DiagnosticArgValue::DocumentParseKind(kind) => {
                Self::DocumentParseKind((*kind).as_str())
            }
            DiagnosticArgValue::StandardLibraryManifestProblem(problem) => {
                Self::StandardLibraryManifestProblem((*problem).as_str())
            }
            DiagnosticArgValue::DependencySubjectKind(kind) => {
                Self::DependencySubjectKind((*kind).as_str())
            }
            DiagnosticArgValue::DependencyRequirementKind(kind) => {
                Self::DependencyRequirementKind((*kind).as_str())
            }
            DiagnosticArgValue::OutputSink(sink) => {
                Self::OutputSink(DiagnosticOutputSinkJson::from_sink(sink))
            }
            DiagnosticArgValue::Visibility(visibility) => Self::Visibility((*visibility).as_str()),
            DiagnosticArgValue::ModuleTrust(trust) => Self::ModuleTrust((*trust).as_str()),
            DiagnosticArgValue::SourceName(name) => Self::SourceName(name.clone()),
            DiagnosticArgValue::SourceCount(source_count) => Self::SourceCount(*source_count),
            DiagnosticArgValue::SourceInputKind(kind) => Self::SourceInputKind((*kind).as_str()),
            DiagnosticArgValue::SourceInput(input) => {
                Self::SourceInput(DiagnosticSourceInputJson::from_input(input))
            }
            DiagnosticArgValue::SyntaxKind(kind) => Self::SyntaxKind((*kind).as_str()),
            DiagnosticArgValue::TextOffset(offset) => Self::TextOffset(offset.bytes()),
            DiagnosticArgValue::TokenText(text) => Self::TokenText(text.clone()),
            DiagnosticArgValue::Uri(uri) => Self::Uri(uri.clone()),
            DiagnosticArgValue::SourceSpan(span) => {
                Self::SourceSpan(SourceSpanJson::from_span(*span, source_map))
            }
            DiagnosticArgValue::WorkerCount(worker_count) => Self::WorkerCount(*worker_count),
            DiagnosticArgValue::Revision(revision) => Self::Revision(*revision),
            DiagnosticArgValue::RuntimeAbi(version) => {
                Self::RuntimeAbi(DiagnosticRuntimeAbiVersionJson {
                    major: version.major(),
                    minor: version.minor(),
                })
            }
            DiagnosticArgValue::RuntimeArtifactProblem(problem) => Self::RuntimeArtifactProblem(
                DiagnosticRuntimeArtifactProblemJson::from_problem(problem),
            ),
            DiagnosticArgValue::Type(ty) => Self::Type(DiagnosticTypeJson::from_type(ty)),
            DiagnosticArgValue::SelectionKind(kind) => Self::SelectionKind((*kind).as_str()),
            DiagnosticArgValue::SelectionCandidates(candidates) => Self::SelectionCandidates(
                DiagnosticSelectionCandidatesJson::from_candidates(candidates),
            ),
            DiagnosticArgValue::SelectionRejections(rejections) => Self::SelectionRejections(
                DiagnosticSelectionRejectionsJson::from_rejections(rejections),
            ),
            DiagnosticArgValue::NativeLinkDirectiveProblem(problem) => {
                Self::NativeLinkDirectiveProblem(native_link_directive_problem_json(problem))
            }
            DiagnosticArgValue::NativeSymbolDirectiveProblem(problem) => {
                Self::NativeSymbolDirectiveProblem(native_symbol_directive_problem_json(problem))
            }
            DiagnosticArgValue::PlatformServiceSignatureProblem(problem) => {
                Self::PlatformServiceSignatureProblem(platform_service_signature_problem_json(
                    problem,
                ))
            }
            DiagnosticArgValue::TraitFulfillmentMismatch(mismatch) => {
                Self::TraitFulfillmentMismatch(
                    DiagnosticTraitFulfillmentMismatchJson::from_mismatch(mismatch),
                )
            }
            DiagnosticArgValue::ImplementationOverloadProblem(problem) => {
                Self::ImplementationOverloadProblem(
                    DiagnosticImplementationOverloadProblemJson::from_problem(problem),
                )
            }
            DiagnosticArgValue::CallableOverloadProblem(problem) => Self::CallableOverloadProblem(
                DiagnosticCallableOverloadProblemJson::from_problem(problem),
            ),
            DiagnosticArgValue::ExpressionCategory(kind) => {
                Self::ExpressionCategory((*kind).as_str())
            }
            DiagnosticArgValue::ConstantOperation(operation) => {
                Self::ConstantOperation((*operation).as_str())
            }
            DiagnosticArgValue::LayoutProblem(problem) => {
                Self::LayoutProblem(layout_problem_json(problem))
            }
            DiagnosticArgValue::UnionTagProblem(problem) => {
                Self::UnionTagProblem(union_tag_problem_json(problem))
            }
            DiagnosticArgValue::CopyContractProblem(problem) => {
                Self::CopyContractProblem(copy_contract_problem_json(*problem))
            }
            DiagnosticArgValue::StoredTypeProblem(problem) => {
                Self::StoredTypeProblem(DiagnosticProblemJson {
                    reason: problem.category(),
                    context: Vec::new(),
                })
            }
            DiagnosticArgValue::PropagationProblem(problem) => {
                Self::PropagationProblem(propagation_problem_json(problem))
            }
            DiagnosticArgValue::ArrayGeneratorCardinalityProblem(problem) => {
                Self::ArrayGeneratorCardinalityProblem(array_generator_problem_json(problem))
            }
            DiagnosticArgValue::RefinementCapacity(capacity) => {
                Self::RefinementCapacity(refinement_capacity_json(*capacity))
            }
            DiagnosticArgValue::MemoryOperation(operation) => {
                Self::MemoryOperation(operation.as_str())
            }
            DiagnosticArgValue::CallbackStateProblem(problem) => {
                Self::CallbackStateProblem(callback_state_problem_json(*problem))
            }
            DiagnosticArgValue::StorageAccess(access) => {
                Self::StorageAccess(DiagnosticStorageAccessJson::from_access(access))
            }
            DiagnosticArgValue::PatternCoverage(coverage) => {
                Self::PatternCoverage(DiagnosticPatternCoverageJson::from_coverage(coverage))
            }
            DiagnosticArgValue::PatternUnreachability(reason) => {
                Self::PatternUnreachability(reason.as_str())
            }
        }
    }
}

fn retained_generation_problem_json(
    problem: &bray_diagnostics::DiagnosticRetainedGenerationProblem,
) -> DiagnosticArgValueJson {
    use bray_diagnostics::DiagnosticRetainedGenerationProblem as Problem;

    let (expected_read_only, actual_read_only) = match problem {
        Problem::ReadOnly { expected, actual } => (Some(*expected), Some(*actual)),
        _ => (None, None),
    };

    let (expected_unix_mode, actual_unix_mode) = match problem {
        Problem::UnixMode { expected, actual } => (*expected, *actual),
        _ => (None, None),
    };

    DiagnosticArgValueJson::RetainedGenerationProblem {
        problem: problem.as_str(),
        expected_revision: problem.revisions().map(|(expected, _)| expected),
        actual_revision: problem.revisions().map(|(_, actual)| actual),
        expected_read_only,
        actual_read_only,
        expected_unix_mode,
        actual_unix_mode,
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticNativeProductFailureJson {
    reason: &'static str,
    context: Vec<DiagnosticEmissionFieldJson>,
}

impl DiagnosticNativeProductFailureJson {
    fn from_kind(kind: &bray_diagnostics::DiagnosticNativeProductFailureKind) -> Self {
        use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

        let context = match kind {
            Kind::EvaluationCycle(failure) | Kind::SemanticContextFailure(failure) => {
                diagnostic_failure_context(failure.context())
            }
            Kind::EvaluationRuntime(failure) => fact_runtime_failure_context(failure),
            Kind::EvaluationSemanticQuery(failure) => {
                let mut context = vec![text_field("category", failure.category())];
                context.extend(diagnostic_failure_context(failure.context()));

                context
            }
            Kind::EvaluationSemanticValue(failure) => semantic_value_failure_context(*failure),
            Kind::EvaluationChecker(failure) => checker_failure_context(*failure),
            Kind::EvaluationBinding(failure) => match failure.semantic_value_failure() {
                Some(failure) => semantic_value_failure_context(failure),
                None => diagnostic_failure_context(failure.context()),
            },
            Kind::EvaluationLoweringInput(failure) => lowering_input_failure_context(*failure),
            Kind::EvaluationLowering(failure) => lowering_failure_context(*failure),
            Kind::EvaluationProduct(failure) => product_query_failure_context(failure),
            Kind::EvaluationForeign(failure) => foreign_query_failure_context(failure),
            Kind::InvalidNativeLinkInput(failure) => native_link_input_failure_context(failure),
            Kind::RuntimeSelectionIncompatible(detail)
            | Kind::RuntimeSelectionMissingRoleOwner(detail)
            | Kind::RuntimeSelectionMissingCapabilityOwner(detail)
            | Kind::RuntimeSelectionUnreadableArchive(detail)
            | Kind::RuntimeSelectionInvalidArchive(detail)
            | Kind::RuntimeSelectionArchiveDigestMismatch(detail)
            | Kind::PartitionMissingCompatibility(detail)
            | Kind::PartitionInvalidUnit(detail)
            | Kind::GeneratedHostMirInvalid(detail)
            | Kind::ExecutableHostDuplicateRole(detail)
            | Kind::ExecutableHostRuntimeOwnedBinding(detail)
            | Kind::ExecutableHostIncompatibleRuntime(detail)
            | Kind::ExecutableHostMissingRole(detail)
            | Kind::CodegenBackendUnsupportedArtifact(detail)
            | Kind::CodegenBackendUnsupportedTargetDetail(detail)
            | Kind::CodegenBackendInvalidConfigurationDetail(detail)
            | Kind::CodegenBackendGeneratedModuleInvariantDetail(detail)
            | Kind::CodegenBackendLibraryFailure(detail)
            | Kind::CodegenBackendToolFailure(detail)
            | Kind::CodegenBackendInvalidRuntimeMetadata(detail)
            | Kind::CodegenBackendInvalidOutcome(detail)
            | Kind::CodegenBackendRejectedModule(detail)
            | Kind::CodegenBackendArtifactConstruction(detail)
            | Kind::CodegenBackendResourceLimit(detail)
            | Kind::CodegenInvalidRequest(detail)
            | Kind::CodegenMirUnavailable(detail)
            | Kind::CodegenInvalidInstance(detail)
            | Kind::CodegenInvalidUnit(detail)
            | Kind::CodegenUnitMismatch(detail)
            | Kind::CodegenInvalidHostMir(detail)
            | Kind::CodegenInvalidLifecycleMir(detail)
            | Kind::CodegenInvalidMappings(detail)
            | Kind::CodegenMissingRuntimeRole(detail)
            | Kind::CodegenOpenConstantTerm(detail)
            | Kind::CodegenInvalidArrayLength(detail)
            | Kind::CodegenRecursiveValueType(detail)
            | Kind::CodegenUnresolvedType(detail)
            | Kind::CodegenUnsizedTypeByValue(detail)
            | Kind::CodegenUnsupportedType(detail)
            | Kind::CodegenMissingHelperInstance(detail)
            | Kind::CodegenLayoutOverflow(detail)
            | Kind::CodegenInvalidAbiMapping(Some(detail)) => {
                diagnostic_failure_context(detail.context())
            }
            Kind::CodegenBackendNotSelected
            | Kind::MissingProductRoot
            | Kind::InvalidEntryResult
            | Kind::MissingRuntime
            | Kind::LibraryCleanupRequiresMainThread
            | Kind::InvalidSymbolName
            | Kind::EvaluationCancelled
            | Kind::EvaluationSemanticValueStoreCreate
            | Kind::EvaluationConstantCallableBodyUnavailable
            | Kind::EvaluationConstantCallableRootUnavailable
            | Kind::EvaluationAtomicRepresentationTypeUnavailable
            | Kind::EvaluationAtomicRepresentationArgumentsUnavailable
            | Kind::EvaluationAtomicInitializerArgumentUnavailable
            | Kind::EvaluationAtomicInitializerResultUnavailable
            | Kind::EvaluationUninitInitializerResultUnavailable
            | Kind::EvaluationImportedExecutableTemplateMismatch
            | Kind::CheckingInfrastructureFailure
            | Kind::CodegenTargetUnsupportedProfile
            | Kind::CodegenTargetEmptyTriple
            | Kind::CodegenTargetEmptyCpu
            | Kind::CodegenTargetEmptyFeature
            | Kind::ReachabilityEmptyRoots
            | Kind::ReachabilityDuplicateInstance
            | Kind::ReachabilityUndemandedInstance
            | Kind::ReachabilityIncomplete
            | Kind::InstanceTemplateMismatch
            | Kind::InstanceTargetMismatch
            | Kind::InstanceDependencyTargetMismatch
            | Kind::UnitEmpty
            | Kind::UnitDuplicateInstance
            | Kind::UnitMissingCompatibility
            | Kind::UnitTargetMismatch
            | Kind::UnitWorkBoundExceeded
            | Kind::UnitRecipeMismatch
            | Kind::ExecutableHostMissingRuntime
            | Kind::ExecutableHostMissingMainThreadLane
            | Kind::ExecutableHostMissingProtectedFrameAbi
            | Kind::StandardLibraryUnavailable
            | Kind::EmissionBackendDuplicateUnit
            | Kind::LinkTargetEmptyTriple
            | Kind::CodegenBackendUnsupportedTarget
            | Kind::CodegenBackendInvalidConfiguration
            | Kind::CodegenBackendResourceExhausted
            | Kind::CodegenBackendGeneratedModuleInvariant
            | Kind::CodegenBackendUnavailable
            | Kind::CodegenMissingEntrypoint
            | Kind::CodegenInvalidAbiMapping(None)
            | Kind::CodegenInvalidSymbolName => Vec::new(),
        };

        Self {
            reason: kind.as_str(),
            context,
        }
    }
}
