use bray_diagnostics::DiagnosticArgValue;
use serde::Serialize;

use super::emission::{
    DiagnosticEmissionFieldJson, product_query_failure_context, semantic_value_failure_context,
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
    interface_semantic_problem_json, interface_symbol_graph_problem_json, layout_problem_json,
    native_link_directive_problem_json, native_symbol_directive_problem_json,
    platform_service_signature_problem_json, propagation_problem_json, refinement_capacity_json,
    union_tag_problem_json,
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

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticNativeProductFailureJson {
    reason: &'static str,
    context: Vec<DiagnosticEmissionFieldJson>,
}

impl DiagnosticNativeProductFailureJson {
    fn from_kind(kind: &bray_diagnostics::DiagnosticNativeProductFailureKind) -> Self {
        use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

        let context = match kind {
            Kind::EvaluationSemanticValue(failure)
            | Kind::EvaluationBinding(bray_diagnostics::DiagnosticBindingFailure::SemanticValue(
                failure,
            ))
            | Kind::EvaluationChecker(bray_diagnostics::DiagnosticCheckerFailure::SemanticValue(
                failure,
            )) => semantic_value_failure_context(*failure),
            Kind::EvaluationLoweringInput(failure) => match failure.kind() {
                bray_diagnostics::DiagnosticLoweringInputFailureKind::SemanticValue(failure) => {
                    semantic_value_failure_context(failure)
                }
                _ => Vec::new(),
            },
            Kind::EvaluationLowering(failure) => match failure.kind() {
                bray_diagnostics::DiagnosticLoweringFailureKind::SemanticValue(failure) => {
                    semantic_value_failure_context(failure)
                }
                _ => Vec::new(),
            },
            Kind::EvaluationProduct(failure) => product_query_failure_context(failure),
            _ => Vec::new(),
        };

        Self {
            reason: kind.as_str(),
            context,
        }
    }
}
