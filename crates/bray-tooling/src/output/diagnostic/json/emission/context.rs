use super::super::DiagnosticOutputSinkJson;
use super::failure::{
    DiagnosticEmissionFieldJson, DiagnosticEmissionFieldValueJson, artifact_field, count_field,
    digest_field, field, text_field,
};
use crate::output::path_to_output_string;

pub(super) fn planning_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionPlanningFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionPlanningFailure as Failure;

    match failure {
        Failure::ProductArtifactMismatch { product, artifact } => vec![
            field(
                "product_kind",
                DiagnosticEmissionFieldValueJson::ProductKind(product.as_str()),
            ),
            field(
                "artifact_kind",
                DiagnosticEmissionFieldValueJson::ArtifactKind(artifact.as_str()),
            ),
        ],
        Failure::PackageInterfaceProductMismatch { expected, actual } => vec![
            text_field("expected_product", expected),
            text_field("actual_product", actual),
        ],
        Failure::MissingRootFrame(identity) => vec![digest_field("root_frame", identity)],
        Failure::UnsupportedBackendTarget(target) => vec![text_field("target", target)],
        Failure::UnsupportedBackendArtifact(artifact)
        | Failure::MissingSerializationArtifact(artifact)
        | Failure::MissingOutputName(artifact)
        | Failure::InvalidGeneratedFileName(artifact)
        | Failure::ExplicitOutputSuffixMismatch(artifact)
        | Failure::ArtifactOrdinalOverflow(artifact) => vec![field(
            "artifact_kind",
            DiagnosticEmissionFieldValueJson::ArtifactKind(artifact.as_str()),
        )],
        Failure::UnsupportedDebugInformation(mode) => vec![field(
            "debug_information",
            DiagnosticEmissionFieldValueJson::DebugInformationMode(mode.as_str()),
        )],
        Failure::UnsupportedDebugOutput(mode) => vec![field(
            "debug_output",
            DiagnosticEmissionFieldValueJson::DebugOutputMode(mode.as_str()),
        )],
        Failure::UnsupportedAssemblySyntax(syntax) => vec![field(
            "assembly_syntax",
            DiagnosticEmissionFieldValueJson::AssemblySyntax(syntax.as_str()),
        )],
        Failure::InvalidDebugOutput {
            information,
            output,
        } => vec![
            field(
                "debug_information",
                DiagnosticEmissionFieldValueJson::DebugInformationMode(information.as_str()),
            ),
            field(
                "debug_output",
                DiagnosticEmissionFieldValueJson::DebugOutputMode(output.as_str()),
            ),
        ],
        Failure::OutputCollision(sink) => vec![field(
            "output_sink",
            DiagnosticEmissionFieldValueJson::OutputSink(DiagnosticOutputSinkJson::from_sink(sink)),
        )],
        Failure::BackendRequestForeignUnit(artifact)
        | Failure::BackendRequestDuplicateIdentity(artifact) => {
            vec![artifact_field("artifact", *artifact)]
        }
        Failure::BackendRequestMissingLinkableArtifact { kind, requirement } => vec![
            field(
                "artifact_kind",
                DiagnosticEmissionFieldValueJson::ArtifactKind(kind.as_str()),
            ),
            field(
                "requirement",
                DiagnosticEmissionFieldValueJson::ArtifactRequirement(requirement.as_str()),
            ),
        ],
        Failure::Incomplete
        | Failure::MissingPackageInterfaceArtifact
        | Failure::UnexpectedPackageInterfaceArtifact
        | Failure::MissingLinkedProduct
        | Failure::MultipleLinkedProducts
        | Failure::LinkedCompanionRequirementMismatch
        | Failure::MissingBackend
        | Failure::MissingCodegenUnits
        | Failure::MissingExecutableHost
        | Failure::MissingRequiredDebugCompanion
        | Failure::UnexpectedDebugCompanion
        | Failure::MissingLinkableArtifact
        | Failure::InvalidProductName
        | Failure::MultipleArtifactsForSingleSink
        | Failure::MissingExplicitFileName
        | Failure::InvalidExplicitFileName
        | Failure::ManagedProductDestinationRequired
        | Failure::BackendRequestEmpty
        | Failure::BackendRequestMissingRequiredDebugCompanion
        | Failure::BackendRequestUnexpectedDebugCompanion
        | Failure::BackendRequestUnexpectedAssemblySyntax
        | Failure::InconsistentPlan => Vec::new(),
    }
}

pub(super) fn package_interface_failure_context(
    failure: &bray_diagnostics::DiagnosticPackageInterfaceFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticPackageInterfaceFailure as Failure;

    match failure {
        Failure::RecoveredPublicSymbol(kind)
        | Failure::IncompletePublicDeclaration(kind)
        | Failure::DuplicateSymbol(kind)
        | Failure::MissingSymbol(kind)
        | Failure::MissingSemanticContent(kind) => vec![text_field("symbol_kind", kind)],
        Failure::DuplicateDependencyPackage(package) => {
            vec![text_field("package", package)]
        }
        Failure::NonCanonicalSymbolOrder { previous, current } => vec![
            count_field("previous_record", *previous),
            count_field("current_record", *current),
        ],
        Failure::IdentityNonCanonicalSymbolId { expected, actual } => vec![
            count_field("expected_record", *expected),
            count_field("actual_record", *actual),
        ],
        Failure::IdentityMissingPackageRoot { actual } => {
            vec![text_field("actual_symbol_kind", actual)]
        }
        Failure::IdentityPackageRootHasContainer(container) => {
            vec![count_field("container_record", *container)]
        }
        Failure::IdentityPackageMismatch(record)
        | Failure::IdentityMissingContainer(record)
        | Failure::ExportOwnerOutOfBounds(record)
        | Failure::InvalidExportOwner(record)
        | Failure::ExportTargetOutOfBounds(record)
        | Failure::InvalidDirectExportTarget(record)
        | Failure::DuplicateExecutableTemplate(record)
        | Failure::DuplicateNativeBoundary(record)
        | Failure::ImplementationDuplicateCallableBody(record)
        | Failure::ImplementationDuplicateExecutableTemplate(record)
        | Failure::ImplementationDuplicateNativeBoundary(record)
        | Failure::ImplementationInvalidExecutableOwner(record)
        | Failure::ImplementationInvalidNativeBoundaryOwner(record)
        | Failure::ImplementationInvalidCallableOwner(record) => {
            vec![count_field("record", *record)]
        }
        Failure::IdentitySymbolKindMismatch {
            record,
            declared,
            keyed,
        } => vec![
            count_field("record", *record),
            text_field("declared_symbol_kind", declared),
            text_field("keyed_symbol_kind", keyed),
        ],
        Failure::IdentityDuplicateExternalKey { first, duplicate } => vec![
            count_field("first_record", *first),
            count_field("duplicate_record", *duplicate),
        ],
        Failure::IdentityInvalidContainer { record, container }
        | Failure::IdentityContainerKeyMismatch { record, container } => vec![
            count_field("record", *record),
            count_field("container_record", *container),
        ],
        Failure::IdentityUnexpectedRoot { record, kind } => vec![
            count_field("record", *record),
            text_field("symbol_kind", kind),
        ],
        Failure::RelationshipSymbolOutOfBounds {
            owner,
            member,
            ordinal,
        }
        | Failure::InvalidRelationship {
            owner,
            member,
            ordinal,
        }
        | Failure::DuplicateRelationshipPosition {
            owner,
            member,
            ordinal,
        } => vec![
            count_field("owner_record", *owner),
            count_field("member_record", *member),
            count_field("ordinal", *ordinal),
        ],
        Failure::DependencyOutOfBounds(index) | Failure::DependencyKeyPackageMismatch(index) => {
            vec![count_field("dependency_index", *index)]
        }
        Failure::DuplicateExportName { owner, name } => vec![
            count_field("owner_record", *owner),
            text_field("name", name),
        ],
        Failure::Unavailable
        | Failure::InvalidCompilation
        | Failure::SymbolCountOverflow
        | Failure::NonLibraryProduct
        | Failure::DependencyCountOverflow
        | Failure::IdentityEmpty
        | Failure::IdentitySymbolCountOverflow
        | Failure::ImplementationContentTooLarge => Vec::new(),
    }
}

pub(super) fn codegen_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionCodegenFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionCodegenFailure as Failure;

    match failure {
        Failure::DuplicateUnit(unit)
        | Failure::DuplicateMappings(unit)
        | Failure::MissingUnit(unit)
        | Failure::MissingMappings(unit)
        | Failure::Request(unit)
        | Failure::Generation(unit)
        | Failure::MergeDuplicateUnit(unit)
        | Failure::MergeMissingUnit(unit)
        | Failure::MergeUnrequestedUnit(unit)
        | Failure::MergeBackendMismatch(unit)
        | Failure::MergeCapabilityMismatch(unit)
        | Failure::MergeTargetMismatch(unit) => vec![digest_field("codegen_unit", unit)],
        Failure::MergeMissingArtifact(artifact)
        | Failure::MergeUnrequestedArtifact(artifact)
        | Failure::MergeArtifactKindMismatch(artifact)
        | Failure::MergeInvalidContent(artifact) => vec![artifact_field("artifact", *artifact)],
        Failure::MergeReadFailed { artifact, error } => vec![
            artifact_field("artifact", *artifact),
            field(
                "io_error",
                DiagnosticEmissionFieldValueJson::IoErrorKind(error.as_str()),
            ),
        ],
        Failure::MergeLengthMismatch {
            artifact,
            expected,
            actual,
        } => vec![
            artifact_field("artifact", *artifact),
            field(
                "expected_bytes",
                DiagnosticEmissionFieldValueJson::Count(*expected),
            ),
            field(
                "actual_bytes",
                DiagnosticEmissionFieldValueJson::Count(*actual),
            ),
        ],
        Failure::MergeDigestMismatch {
            artifact,
            expected,
            actual,
        } => vec![
            artifact_field("artifact", *artifact),
            digest_field("expected_digest", expected),
            digest_field("actual_digest", actual),
        ],
        Failure::Incomplete => Vec::new(),
    }
}

pub(super) fn staging_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionStagingFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionStagingFailure as Failure;

    match failure {
        Failure::DuplicateContribution(artifact)
        | Failure::MissingContribution(artifact)
        | Failure::InvalidContribution(artifact)
        | Failure::UnexpectedContribution(artifact)
        | Failure::UnsupportedOutput(artifact)
        | Failure::InvalidPath(artifact)
        | Failure::InvalidContent(artifact) => vec![artifact_field("artifact", *artifact)],
        Failure::Incomplete => Vec::new(),
    }
}

pub(super) fn link_plan_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionLinkPlanFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionLinkPlanFailure as Failure;

    match failure {
        Failure::DuplicateStagedArtifact(artifact)
        | Failure::MissingStagedArtifact(artifact)
        | Failure::UnexpectedStagedArtifact(artifact)
        | Failure::UnsupportedStagedArtifact(artifact)
        | Failure::DuplicateOutputStaging(artifact)
        | Failure::MissingOutputStaging(artifact)
        | Failure::UnexpectedOutputStaging(artifact) => {
            vec![artifact_field("artifact", *artifact)]
        }
        Failure::OutputKindMismatch { artifact, actual } => vec![
            artifact_field("artifact", *artifact),
            field(
                "actual_output_kind",
                DiagnosticEmissionFieldValueJson::LinkedArtifactKind(actual.as_str()),
            ),
        ],
        Failure::InvalidStartupInputKind(kind)
        | Failure::InvalidNativeInputKind(kind)
        | Failure::InvalidTerminationInputKind(kind) => vec![field(
            "input_kind",
            DiagnosticEmissionFieldValueJson::LinkInputKind(kind.as_str()),
        )],
        Failure::DuplicateInput(ordinal) | Failure::UnexpectedStartupInput(ordinal) => {
            vec![count_field("input_ordinal", *ordinal)]
        }
        Failure::DuplicateOutput(ordinal) => vec![count_field("output_ordinal", *ordinal)],
        Failure::OutputPathCollision { first, second } => vec![
            count_field("first_output", *first),
            count_field("second_output", *second),
        ],
        Failure::IncompatibleOutputKind { product, artifact } => vec![
            field(
                "product_kind",
                DiagnosticEmissionFieldValueJson::LinkedProductKind(product.as_str()),
            ),
            field(
                "artifact_kind",
                DiagnosticEmissionFieldValueJson::LinkedArtifactKind(artifact.as_str()),
            ),
        ],
        Failure::InvalidOutputPath(path) => vec![field(
            "path",
            DiagnosticEmissionFieldValueJson::Path(path_to_output_string(path)),
        )],
        Failure::Incomplete
        | Failure::MissingLinkedProduct
        | Failure::MissingLinker
        | Failure::UnexpectedLinker
        | Failure::RuntimeContractMismatch
        | Failure::InputOrdinalOverflow
        | Failure::OutputOrdinalOverflow
        | Failure::InputEmptyPath
        | Failure::InputSourceKindMismatch
        | Failure::WholeArchiveRequiresArchive
        | Failure::OutputEmptyPath
        | Failure::MissingInputs
        | Failure::DuplicateSearchPath
        | Failure::MissingRuntimeComponent
        | Failure::UnexpectedRuntimeComponent
        | Failure::RuntimeArtifactMismatch
        | Failure::MissingPrimaryOutput
        | Failure::MultiplePrimaryOutputs
        | Failure::OptionalPrimaryOutput
        | Failure::MissingExecutableHost
        | Failure::UnexpectedExecutableHost
        | Failure::ExecutableHostProductMismatch
        | Failure::ExecutableHostTargetMismatch
        | Failure::UnexpectedEntryPoint
        | Failure::MissingStartupMode
        | Failure::UnexpectedStartupMode
        | Failure::MissingStartupInput
        | Failure::MissingDebugCompanion
        | Failure::UnexpectedDebugCompanion => Vec::new(),
    }
}
