use std::path::PathBuf;

use bray_source::{SourceInputKind, SourceSpan, TextSize};
use bray_syntax::SyntaxKind;

use super::{
    DiagnosticAlignmentKind, DiagnosticArtifactDigest, DiagnosticArtifactKind,
    DiagnosticArtifactRequirement, DiagnosticAssemblySyntaxKind, DiagnosticCallableAbi,
    DiagnosticDebugInformationMode, DiagnosticDebugOutputMode, DiagnosticDependencyRequirementKind,
    DiagnosticDependencySubjectKind, DiagnosticDocumentParseKind,
    DiagnosticEmissionArtifactOperation, DiagnosticExternalToolFailureKind,
    DiagnosticExternalToolOperation, DiagnosticIoErrorKind, DiagnosticLinkInputKind,
    DiagnosticLinkRequirement, DiagnosticLinkedArtifactKind, DiagnosticLinkedProductKind,
    DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticNativeProductFailureKind,
    DiagnosticOutputSink, DiagnosticProductKind, DiagnosticRuntimeAbiVersion,
    DiagnosticRuntimeArtifactProblem, DiagnosticSelectionKind,
    DiagnosticStandardLibraryManifestProblem, DiagnosticTargetRepresentation, DiagnosticType,
    DiagnosticVisibility,
};
use crate::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};

/// Stable name for a diagnostic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgName {
    /// Actual externally supplied count or size.
    ActualCount,
    /// Actual completed artifact byte count.
    ActualByteCount,
    /// Actual digest measured from completed artifact bytes.
    ActualArtifactDigest,
    /// Path of an external compiler artifact.
    ArtifactPath,
    /// Category of compiler artifact involved in an operation.
    ArtifactKind,
    /// Stable same-category artifact ordinal.
    ArtifactOrdinal,
    /// Target representation rejected by the selected target or ABI.
    TargetRepresentation,
    /// Selected foreign callable ABI.
    CallableAbi,
    /// Storage, allocation, or ABI alignment surface.
    AlignmentKind,
    /// Alignment required by a selected layout.
    RequiredAlignment,
    /// Maximum alignment accepted by the target surface.
    MaximumAlignment,
    /// Actual interface or language revision.
    ActualRevision,
    /// Literal category supplied to a target predicate.
    ActualTargetPredicateValueKind,
    /// Runtime ABI supplied by an artifact or dependency.
    ActualRuntimeAbi,
    /// Target identity supplied by a runtime artifact.
    ActualTargetIdentity,
    /// Semantic type found by checking.
    ActualType,
    /// Byte that participates in the diagnostic.
    Byte,
    /// Number of bytes that participate in the diagnostic.
    ByteCount,
    /// Character that participates in the diagnostic.
    Character,
    /// Start location of a block comment or another paired source construct.
    ConstructStart,
    /// Source-level declaration name.
    DeclarationName,
    /// Named or fixed-keyword trait member.
    TraitMemberName,
    /// Name spelling used by a semantic reference.
    ReferencedName,
    /// Stable identity of the selected code generation backend.
    CodegenBackendIdentity,
    /// Complete identity of the selected native linker or archiver driver.
    LinkerDriverIdentity,
    /// Semantic category required at a name reference.
    ExpectedNameKind,
    /// Expected completed artifact byte count.
    ExpectedByteCount,
    /// Expected producer-supplied artifact digest.
    ExpectedArtifactDigest,
    /// Syntax kind that was present in source.
    ActualSyntaxKind,
    /// Syntax kind that was expected by the compiler phase.
    ExpectedSyntaxKind,
    /// Declaration modifier involved in a declaration diagnostic.
    ModifierKind,
    /// Second modifier that conflicts with another declaration modifier.
    ConflictingModifierKind,
    /// Declaration directive involved in a declaration diagnostic.
    DirectiveKind,
    /// Second directive that conflicts with another declaration directive.
    ConflictingDirectiveKind,
    /// Package identity selected by package resolution.
    ExpectedPackageIdentity,
    /// Package identity encoded by an external artifact.
    ActualPackageIdentity,
    /// Package-product identity selected by package resolution.
    ExpectedProductIdentity,
    /// Package-product identity encoded by an external artifact.
    ActualProductIdentity,
    /// Exact selected target triple.
    TargetTriple,
    /// Target triple required by a request or compilation.
    ExpectedTargetTriple,
    /// Target triple that conflicted with a request or compilation.
    ActualTargetTriple,
    /// Exact structured reason native product planning could not complete.
    NativeProductFailureKind,
    /// Exact terminal phase of an unsuccessful emission operation.
    EmissionFailure,
    /// Exact artifact operation that failed during emission.
    EmissionArtifactOperation,
    /// Stable ordinal of one native link input.
    LinkInputOrdinal,
    /// Stable ordinal of one linked output staging destination.
    LinkOutputOrdinal,
    /// Stable ordinal of a conflicting linked output staging destination.
    ConflictingLinkOutputOrdinal,
    /// Native link input category found while constructing a plan.
    ActualLinkInputKind,
    /// Linked artifact category found while constructing a plan.
    ActualLinkedArtifactKind,
    /// Linked product category found while constructing a plan.
    ActualLinkedProductKind,
    /// Language-level product category found while planning emission.
    ActualProductKind,
    /// Debug-information amount rejected by a backend.
    ActualDebugInformationMode,
    /// Debug-information placement rejected by a backend.
    ActualDebugOutputMode,
    /// Assembly syntax rejected by a backend.
    ActualAssemblySyntax,
    /// Backend artifact requirement that could not be satisfied.
    ArtifactRequirement,
    /// Effective visibility established by an earlier declaration.
    ExpectedVisibility,
    /// Effective visibility supplied by the current declaration.
    ActualVisibility,
    /// Module trust state established by an earlier declaration.
    ExpectedModuleTrust,
    /// Module trust state supplied by the current declaration.
    ActualModuleTrust,
    /// Path of a source file or external artifact.
    FilePath,
    /// Zero-based source input index from the request boundary.
    InputIndex,
    /// Package-interface resource category.
    InterfaceLimit,
    /// Zero-based record index inside one package interface.
    InterfaceRecordIndex,
    /// Record index required by an interface contract.
    ExpectedInterfaceRecordIndex,
    /// Record index found in an interface record.
    ActualInterfaceRecordIndex,
    /// Second record index involved in an interface relationship.
    RelatedInterfaceRecordIndex,
    /// Package-interface dependency-table index.
    InterfaceDependencyIndex,
    /// Symbol category supplied by an artifact or request.
    ActualSymbolKind,
    /// Symbol category required by an artifact contract.
    ExpectedSymbolKind,
    /// Exact imported symbol-graph construction problem.
    InterfaceSymbolGraphProblem,
    /// Exact recursive symbol identity named by an imported artifact diagnostic.
    InterfaceSymbolIdentity,
    /// Exact imported semantic-content problem.
    InterfaceSemanticProblem,
    /// Exact invalid command selection.
    ProjectSelectionProblem,
    /// Exact structured project command failure.
    ProjectCommandFailure,
    /// Exact workspace or package manifest field.
    ProjectManifestField,
    /// Exact package or product identity participating in a project dependency cycle.
    ProjectDependencyCycleMember,
    /// Path supplied by a Bray project manifest.
    ProjectPath,
    /// Exact host-toolchain reason native product emission is unavailable.
    UnsupportedEmissionReason,
    /// Package-interface section category.
    InterfaceSection,
    /// Stable I/O error category from the host.
    IoErrorKind,
    /// Exact external-tool operation that failed.
    ExternalToolOperation,
    /// Stable external-tool failure category.
    ExternalToolFailureKind,
    /// Exact bounded completed external-tool failure result.
    ExternalToolExit,
    /// Exact native link requirement rejected by the selected driver.
    LinkRequirement,
    /// Stable syntax or schema failure while decoding a structured document.
    DocumentParseKind,
    /// Exact standard library manifest contract violation.
    StandardLibraryManifestProblem,
    /// Semantic entity whose state is required across an await.
    DependencySubjectKind,
    /// Exact state required across an await.
    DependencyRequirementKind,
    /// One-based line in a structured document.
    DocumentLine,
    /// One-based column in a structured document.
    DocumentColumn,
    /// Typed external output destination.
    OutputSink,
    /// Maximum accepted count or size.
    MaximumCount,
    /// Expected interface or language revision.
    ExpectedRevision,
    /// Literal category accepted by a target predicate property.
    ExpectedTargetPredicateValueKind,
    /// Runtime ABI required by the compilation request.
    ExpectedRuntimeAbi,
    /// Target identity required by a runtime artifact contract.
    ExpectedTargetIdentity,
    /// Typed runtime-artifact metadata or catalog failure.
    RuntimeArtifactProblem,
    /// Semantic type required by checking.
    ExpectedType,
    /// Name of a virtual, generated, or test-fixture source.
    SourceName,
    /// Number of source inputs involved in the diagnostic.
    SourceCount,
    /// Semantic operation category being selected.
    SelectionKind,
    /// Exact deterministic candidates that remain applicable.
    SelectionCandidates,
    /// Exact deterministic candidates rejected by an incompatible request.
    SelectionRejections,
    /// Exact invalid native-link directive problem.
    NativeLinkDirectiveProblem,
    /// Exact invalid native-symbol directive problem.
    NativeSymbolDirectiveProblem,
    /// Exact compiler-defined platform-service signature mismatch.
    PlatformServiceSignatureProblem,
    TraitFulfillmentMismatch,
    ImplementationOverloadProblem,
    CallableOverloadProblem,
    /// Source-level expression category participating in checking.
    ExpressionCategory,
    /// Compile-time operation rejected during constant evaluation.
    ConstantOperation,
    /// Exact reason a declared layout contract failed.
    LayoutProblem,
    /// Exact reason a union-tag contract failed.
    UnionTagProblem,
    /// Exact reason a declared copy contract failed.
    CopyContractProblem,
    /// Exact reason a source-level type cannot be stored inline.
    StoredTypeProblem,
    /// Exact reason propagation has no compatible enclosing boundary.
    PropagationProblem,
    /// Exact reason fixed-array generator cardinality is not provable.
    ArrayGeneratorCardinalityProblem,
    /// Exact configured refinement-analysis capacity violation.
    RefinementCapacity,
    /// Exact compiler-provided memory operation.
    MemoryOperation,
    /// Exact callback-state source contract failure.
    CallbackStateProblem,
    /// Exact source-semantic access rejected by storage-flow checking.
    StorageAccess,
    /// Exact missing value categories for a non-exhaustive match.
    PatternCoverage,
    /// Exact reason a match arm or pattern alternative is unreachable.
    PatternUnreachability,
    /// Stable source input category.
    SourceInputKind,
    /// Complete source-input request identity and origin.
    SourceInput,
    /// Byte offset inside a source input.
    TextOffset,
    /// Exact token source text that participates in the diagnostic.
    TokenText,
    /// URI of an LSP or other URI-backed source input.
    Uri,
    /// Source span that participates in the diagnostic.
    SourceSpan,
    /// Worker count requested at the driver or compilation boundary.
    WorkerCount,
}

impl DiagnosticArgName {
    /// Returns the stable machine key for this argument name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ActualCount => "actual_count",
            Self::ActualByteCount => "actual_byte_count",
            Self::ActualArtifactDigest => "actual_artifact_digest",
            Self::ArtifactPath => "artifact_path",
            Self::ArtifactKind => "artifact_kind",
            Self::ArtifactOrdinal => "artifact_ordinal",
            Self::TargetRepresentation => "target_representation",
            Self::CallableAbi => "callable_abi",
            Self::AlignmentKind => "alignment_kind",
            Self::RequiredAlignment => "required_alignment",
            Self::MaximumAlignment => "maximum_alignment",
            Self::ActualRevision => "actual_revision",
            Self::ActualTargetPredicateValueKind => "actual_target_predicate_value_kind",
            Self::ActualRuntimeAbi => "actual_runtime_abi",
            Self::ActualTargetIdentity => "actual_target_identity",
            Self::ActualType => "actual_type",
            Self::Byte => "byte",
            Self::ByteCount => "byte_count",
            Self::Character => "character",
            Self::ConstructStart => "construct_start",
            Self::DeclarationName => "declaration_name",
            Self::TraitMemberName => "trait_member_name",
            Self::ReferencedName => "referenced_name",
            Self::CodegenBackendIdentity => "codegen_backend_identity",
            Self::LinkerDriverIdentity => "linker_driver_identity",
            Self::ExpectedNameKind => "expected_name_kind",
            Self::ExpectedByteCount => "expected_byte_count",
            Self::ExpectedArtifactDigest => "expected_artifact_digest",
            Self::ActualSyntaxKind => "actual_syntax_kind",
            Self::ExpectedSyntaxKind => "expected_syntax_kind",
            Self::ModifierKind => "modifier_kind",
            Self::ConflictingModifierKind => "conflicting_modifier_kind",
            Self::DirectiveKind => "directive_kind",
            Self::ConflictingDirectiveKind => "conflicting_directive_kind",
            Self::ExpectedPackageIdentity => "expected_package_identity",
            Self::ActualPackageIdentity => "actual_package_identity",
            Self::ExpectedProductIdentity => "expected_product_identity",
            Self::ActualProductIdentity => "actual_product_identity",
            Self::TargetTriple => "target_triple",
            Self::ExpectedTargetTriple => "expected_target_triple",
            Self::ActualTargetTriple => "actual_target_triple",
            Self::NativeProductFailureKind => "native_product_failure_kind",
            Self::EmissionFailure => "emission_failure",
            Self::EmissionArtifactOperation => "emission_artifact_operation",
            Self::LinkInputOrdinal => "link_input_ordinal",
            Self::LinkOutputOrdinal => "link_output_ordinal",
            Self::ConflictingLinkOutputOrdinal => "conflicting_link_output_ordinal",
            Self::ActualLinkInputKind => "actual_link_input_kind",
            Self::ActualLinkedArtifactKind => "actual_linked_artifact_kind",
            Self::ActualLinkedProductKind => "actual_linked_product_kind",
            Self::ActualProductKind => "actual_product_kind",
            Self::ActualDebugInformationMode => "actual_debug_information_mode",
            Self::ActualDebugOutputMode => "actual_debug_output_mode",
            Self::ActualAssemblySyntax => "actual_assembly_syntax",
            Self::ArtifactRequirement => "artifact_requirement",
            Self::ExpectedVisibility => "expected_visibility",
            Self::ActualVisibility => "actual_visibility",
            Self::ExpectedModuleTrust => "expected_module_trust",
            Self::ActualModuleTrust => "actual_module_trust",
            Self::FilePath => "file_path",
            Self::InputIndex => "input_index",
            Self::InterfaceLimit => "interface_limit",
            Self::InterfaceRecordIndex => "interface_record_index",
            Self::ExpectedInterfaceRecordIndex => "expected_interface_record_index",
            Self::ActualInterfaceRecordIndex => "actual_interface_record_index",
            Self::RelatedInterfaceRecordIndex => "related_interface_record_index",
            Self::InterfaceDependencyIndex => "interface_dependency_index",
            Self::ActualSymbolKind => "actual_symbol_kind",
            Self::ExpectedSymbolKind => "expected_symbol_kind",
            Self::InterfaceSymbolGraphProblem => "interface_symbol_graph_problem",
            Self::InterfaceSymbolIdentity => "interface_symbol_identity",
            Self::InterfaceSemanticProblem => "interface_semantic_problem",
            Self::ProjectSelectionProblem => "project_selection_problem",
            Self::ProjectCommandFailure => "project_command_failure",
            Self::ProjectManifestField => "project_manifest_field",
            Self::ProjectDependencyCycleMember => "project_dependency_cycle_member",
            Self::ProjectPath => "project_path",
            Self::UnsupportedEmissionReason => "unsupported_emission_reason",
            Self::InterfaceSection => "interface_section",
            Self::IoErrorKind => "io_error_kind",
            Self::ExternalToolOperation => "external_tool_operation",
            Self::ExternalToolFailureKind => "external_tool_failure_kind",
            Self::ExternalToolExit => "external_tool_exit",
            Self::LinkRequirement => "link_requirement",
            Self::DocumentParseKind => "document_parse_kind",
            Self::StandardLibraryManifestProblem => "standard_library_manifest_problem",
            Self::DependencySubjectKind => "dependency_subject_kind",
            Self::DependencyRequirementKind => "dependency_requirement_kind",
            Self::DocumentLine => "document_line",
            Self::DocumentColumn => "document_column",
            Self::OutputSink => "output_sink",
            Self::MaximumCount => "maximum_count",
            Self::ExpectedRevision => "expected_revision",
            Self::ExpectedTargetPredicateValueKind => "expected_target_predicate_value_kind",
            Self::ExpectedRuntimeAbi => "expected_runtime_abi",
            Self::ExpectedTargetIdentity => "expected_target_identity",
            Self::RuntimeArtifactProblem => "runtime_artifact_problem",
            Self::ExpectedType => "expected_type",
            Self::SourceName => "source_name",
            Self::SourceCount => "source_count",
            Self::SelectionKind => "selection_kind",
            Self::SelectionCandidates => "selection_candidates",
            Self::SelectionRejections => "selection_rejections",
            Self::NativeLinkDirectiveProblem => "native_link_directive_problem",
            Self::NativeSymbolDirectiveProblem => "native_symbol_directive_problem",
            Self::PlatformServiceSignatureProblem => "platform_service_signature_problem",
            Self::TraitFulfillmentMismatch => "trait_fulfillment_mismatch",
            Self::ImplementationOverloadProblem => "implementation_overload_problem",
            Self::CallableOverloadProblem => "callable_overload_problem",
            Self::ExpressionCategory => "expression_category",
            Self::ConstantOperation => "constant_operation",
            Self::LayoutProblem => "layout_problem",
            Self::UnionTagProblem => "union_tag_problem",
            Self::CopyContractProblem => "copy_contract_problem",
            Self::StoredTypeProblem => "stored_type_problem",
            Self::PropagationProblem => "propagation_problem",
            Self::ArrayGeneratorCardinalityProblem => "array_generator_cardinality_problem",
            Self::RefinementCapacity => "refinement_capacity",
            Self::MemoryOperation => "memory_operation",
            Self::CallbackStateProblem => "callback_state_problem",
            Self::StorageAccess => "storage_access",
            Self::PatternCoverage => "pattern_coverage",
            Self::PatternUnreachability => "pattern_unreachability",
            Self::SourceInputKind => "source_input_kind",
            Self::SourceInput => "source_input",
            Self::TextOffset => "text_offset",
            Self::TokenText => "token_text",
            Self::Uri => "uri",
            Self::SourceSpan => "source_span",
            Self::WorkerCount => "worker_count",
        }
    }
}

/// Locale-neutral typed value for a diagnostic argument.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]

pub enum DiagnosticArgValue {
    /// Externally supplied or configured count.
    Count(u64),
    /// Raw source byte.
    Byte(u8),
    /// Source byte count.
    ByteCount(u64),
    /// Deterministic compiler artifact digest.
    ArtifactDigest(DiagnosticArtifactDigest),
    /// Compiler artifact category.
    ArtifactKind(DiagnosticArtifactKind),
    /// Same-category artifact ordinal.
    ArtifactOrdinal(u32),
    /// Target representation category.
    TargetRepresentation(DiagnosticTargetRepresentation),
    /// Foreign callable ABI mode.
    CallableAbi(DiagnosticCallableAbi),
    /// Alignment validation surface.
    AlignmentKind(DiagnosticAlignmentKind),
    /// Source character.
    Character(char),
    /// Source-level declaration name.
    DeclarationName(String),
    /// Name spelling used by a semantic reference.
    ReferencedName(String),
    /// Stable identity of the selected code generation backend.
    CodegenBackendIdentity(String),
    /// Complete identity of the selected native linker or archiver driver.
    LinkerDriverIdentity(crate::DiagnosticLinkerDriverIdentity),
    /// Stable package identity.
    PackageIdentity(String),
    /// Stable package-product identity.
    ProductIdentity(String),
    /// Stable symbol-category key.
    SymbolKind(String),
    /// Exact imported symbol-graph construction problem.
    InterfaceSymbolGraphProblem(crate::DiagnosticInterfaceSymbolGraphProblem),
    /// Exact recursive symbol identity retained from an imported artifact.
    InterfaceSymbolIdentity(crate::DiagnosticInterfaceSymbolIdentity),
    /// Exact imported semantic-content problem.
    InterfaceSemanticProblem(crate::DiagnosticInterfaceSemanticProblem),
    /// Exact invalid command selection.
    ProjectSelectionProblem(crate::DiagnosticProjectSelectionProblem),
    /// Exact structured project command failure.
    ProjectCommandFailure(crate::DiagnosticProjectCommandFailure),
    ProjectManifestField(crate::DiagnosticProjectManifestField),
    /// Exact package or product identity participating in a project dependency cycle.
    ProjectDependencyCycleMember(crate::DiagnosticProjectDependencyCycleMember),
    /// Target-predicate literal category.
    TargetPredicateValueKind(crate::DiagnosticTargetPredicateValueKind),
    /// Exact host-toolchain reason native product emission is unavailable.
    UnsupportedEmissionReason(crate::DiagnosticUnsupportedEmissionReason),
    /// Exact target triple.
    TargetTriple(String),
    /// Canonical compilation target identity.
    TargetIdentity(String),
    /// Exact structured reason native product planning could not complete.
    NativeProductFailureKind(DiagnosticNativeProductFailureKind),
    /// Exact terminal phase of an unsuccessful emission operation.
    EmissionFailure(crate::DiagnosticEmissionFailure),
    /// Exact artifact operation that failed during emission.
    EmissionArtifactOperation(DiagnosticEmissionArtifactOperation),
    /// Native link input category.
    LinkInputKind(DiagnosticLinkInputKind),
    /// Linked artifact category.
    LinkedArtifactKind(DiagnosticLinkedArtifactKind),
    /// Linked product category.
    LinkedProductKind(DiagnosticLinkedProductKind),
    /// Language-level product category.
    ProductKind(DiagnosticProductKind),
    /// Amount of generated debug information.
    DebugInformationMode(DiagnosticDebugInformationMode),
    /// Placement of generated debug information.
    DebugOutputMode(DiagnosticDebugOutputMode),
    /// Selected assembly syntax.
    AssemblySyntax(DiagnosticAssemblySyntaxKind),
    /// Strength of a backend artifact requirement.
    ArtifactRequirement(DiagnosticArtifactRequirement),
    /// Semantic category required at a name reference.
    NameKind(DiagnosticNameKind),
    /// Source file or external artifact path.
    FilePath(PathBuf),
    /// Zero-based source input index.
    InputIndex(u64),
    /// Package-interface resource category.
    InterfaceLimit(DiagnosticInterfaceLimit),
    /// Package-interface section category.
    InterfaceSection(DiagnosticInterfaceSection),
    /// Stable I/O error category from the host.
    IoErrorKind(DiagnosticIoErrorKind),
    /// External-tool operation category.
    ExternalToolOperation(DiagnosticExternalToolOperation),
    /// External-tool failure category.
    ExternalToolFailureKind(DiagnosticExternalToolFailureKind),
    /// Exact bounded completed external-tool failure result.
    ExternalToolExit(crate::DiagnosticExternalToolExit),
    /// Exact native link requirement rejected by the selected driver.
    LinkRequirement(DiagnosticLinkRequirement),
    /// Structured document parse category.
    DocumentParseKind(DiagnosticDocumentParseKind),
    /// Exact standard library manifest contract violation.
    StandardLibraryManifestProblem(DiagnosticStandardLibraryManifestProblem),
    /// Semantic entity whose state is required across an await.
    DependencySubjectKind(DiagnosticDependencySubjectKind),
    /// Exact state required across an await.
    DependencyRequirementKind(DiagnosticDependencyRequirementKind),
    /// Typed external output destination.
    OutputSink(DiagnosticOutputSink),
    /// Effective declaration visibility.
    Visibility(DiagnosticVisibility),
    /// Effective module trust state.
    ModuleTrust(DiagnosticModuleTrust),
    /// Source name.
    SourceName(String),
    /// Source input count.
    SourceCount(u64),
    /// Stable source input category.
    SourceInputKind(SourceInputKind),
    /// Complete source-input request identity and origin.
    SourceInput(crate::DiagnosticSourceInput),
    /// Syntax vocabulary kind.
    SyntaxKind(SyntaxKind),
    /// Byte offset inside source text.
    TextOffset(TextSize),
    /// Exact token source text.
    TokenText(String),
    /// URI string.
    Uri(String),
    /// Source span.
    SourceSpan(SourceSpan),
    /// Requested worker count.
    WorkerCount(u64),
    /// Interface or language revision.
    Revision(u64),
    /// Runtime ABI version.
    RuntimeAbi(DiagnosticRuntimeAbiVersion),
    /// Typed runtime-artifact metadata or catalog failure.
    RuntimeArtifactProblem(DiagnosticRuntimeArtifactProblem),
    /// Locale-neutral semantic type shape.
    Type(DiagnosticType),
    /// Semantic operation category being selected.
    SelectionKind(DiagnosticSelectionKind),
    /// Deterministic applicable selection candidates.
    SelectionCandidates(crate::DiagnosticSelectionCandidates),
    /// Deterministic rejected selection candidates and exact mismatch reasons.
    SelectionRejections(crate::DiagnosticSelectionRejections),
    /// Exact invalid native-link directive problem.
    NativeLinkDirectiveProblem(crate::DiagnosticNativeLinkDirectiveProblem),
    /// Exact invalid native-symbol directive problem.
    NativeSymbolDirectiveProblem(crate::DiagnosticNativeSymbolDirectiveProblem),
    /// Exact compiler-defined platform-service signature mismatch.
    PlatformServiceSignatureProblem(crate::DiagnosticPlatformServiceSignatureProblem),
    /// Exact semantic difference between a trait member requirement and fulfillment.
    TraitFulfillmentMismatch(crate::DiagnosticTraitFulfillmentMismatch),
    /// Exact invalid implementation-overload header or arm problem.
    ImplementationOverloadProblem(crate::DiagnosticImplementationOverloadProblem),
    /// Exact invalid callable-overload arm, family, or signature problem.
    CallableOverloadProblem(crate::DiagnosticCallableOverloadProblem),
    /// Source-level expression category participating in checking.
    ExpressionCategory(crate::DiagnosticExpressionCategory),
    /// Compile-time operation rejected during constant evaluation.
    ConstantOperation(crate::DiagnosticConstantOperation),
    /// Exact declared-layout failure.
    LayoutProblem(crate::DiagnosticLayoutProblem),
    /// Exact union-tag failure.
    UnionTagProblem(crate::DiagnosticUnionTagProblem),
    /// Exact declared-copy failure.
    CopyContractProblem(crate::DiagnosticCopyContractProblem),
    /// Exact invalid inline-storage type.
    StoredTypeProblem(crate::DiagnosticStoredTypeProblem),
    /// Exact propagation-boundary failure.
    PropagationProblem(crate::DiagnosticPropagationProblem),
    /// Exact fixed-array generator cardinality failure.
    ArrayGeneratorCardinalityProblem(crate::DiagnosticArrayGeneratorCardinalityProblem),
    /// Exact configured refinement-analysis capacity violation.
    RefinementCapacity(crate::DiagnosticRefinementCapacity),
    /// Exact compiler-provided memory operation.
    MemoryOperation(crate::DiagnosticMemoryOperation),
    /// Exact callback-state source contract failure.
    CallbackStateProblem(crate::DiagnosticCallbackStateProblem),
    /// Exact source-semantic access rejected by storage-flow checking.
    StorageAccess(crate::DiagnosticStorageAccess),
    /// Exact missing value categories for a non-exhaustive match.
    PatternCoverage(crate::DiagnosticPatternCoverage),
    /// Exact reason a match arm or pattern alternative is unreachable.
    PatternUnreachability(crate::DiagnosticPatternUnreachability),
}
