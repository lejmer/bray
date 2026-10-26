use super::super::native::{
    format_artifact_failure, format_english_emission_evaluation_failure, format_unit_failure,
};
use super::super::source::{
    format_english_artifact_digest, format_english_artifact_kind, format_english_io_error_kind,
    format_english_output_sink, format_english_path,
};
use super::kind::{
    format_english_assembly_syntax, format_english_debug_information_mode,
    format_english_debug_output_mode, format_english_link_input_kind,
    format_english_linked_artifact_kind, format_english_linked_product_kind,
    format_english_product_kind,
};
pub(crate) fn format_english_emission_failure(
    failure: &bray_diagnostics::DiagnosticEmissionFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionFailure as Failure;

    match failure {
        Failure::InvalidRequest => "the emission request is invalid".to_owned(),
        Failure::Planning(failure) => format_english_emission_planning_failure(failure),
        Failure::PackageInterface(failure) => format_english_package_interface_failure(failure),
        Failure::Codegen(failure) => format_english_emission_codegen_failure(failure),
        Failure::Staging(failure) => format_english_emission_staging_failure(failure),
        Failure::NativeIndexSizeLimitExceeded => {
            "native package index exceeds its supported size".to_owned()
        }
        Failure::NativeRead { path, kind } => format!(
            "could not read temporary compiler output {}: {}",
            format_english_path(path),
            format_english_io_error_kind(*kind),
        ),
        Failure::LinkPlan(failure) => format_english_emission_link_plan_failure(failure),
        Failure::Evaluation(failure) => format_english_emission_evaluation_failure(failure),
        Failure::TestCatalogCapacityExceeded => {
            "the test catalog exceeds its supported encoding size".to_owned()
        }
        Failure::MissingContribution(artifact) => format!(
            "the required {} artifact #{} has no generated content",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
        ),
        Failure::InvalidContribution(artifact) => format!(
            "the generated content for {} artifact #{} does not match the output request",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
        ),
        Failure::Publication(artifact) => format!(
            "the completed {} artifact #{} could not be published",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
        ),
        Failure::Linking => "native linking could not produce the requested artifacts".to_owned(),
        Failure::IncompleteProduct => {
            "completed work does not satisfy the product contract".to_owned()
        }
    }
}

fn format_english_emission_planning_failure(
    failure: &bray_diagnostics::DiagnosticEmissionPlanningFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionPlanningFailure as Failure;

    match failure {
        Failure::Incomplete => "a complete output request could not be formed".to_owned(),
        Failure::ProductArtifactMismatch { product, artifact } => format!(
            "the {} product cannot emit {} output",
            format_english_product_kind(*product),
            format_english_artifact_kind(*artifact),
        ),
        Failure::MissingPackageInterfaceArtifact => {
            "the requested package interface has no completed artifact".to_owned()
        }
        Failure::MissingTestCatalogArtifact => {
            "the requested test catalog has no completed artifact".to_owned()
        }
        Failure::UnexpectedPackageInterfaceArtifact => {
            "a package interface was completed without a matching request".to_owned()
        }
        Failure::PackageInterfaceProductMismatch { expected, actual } => format!(
            "the package interface belongs to {actual}, but this request requires {expected}",
        ),
        Failure::MissingLinkedProduct => {
            "a linked companion was requested without a linked product".to_owned()
        }
        Failure::MultipleLinkedProducts => {
            "the emission request contains more than one linked product".to_owned()
        }
        Failure::LinkedCompanionRequirementMismatch => {
            "a linked companion is required more strongly than its linked product".to_owned()
        }
        Failure::MissingBackend => {
            "the requested outputs require native-code generation".to_owned()
        }
        Failure::MissingCodegenUnits => {
            "native code for the requested program was not prepared".to_owned()
        }
        Failure::UnsupportedBackendTarget(target) => {
            format!("the native-code generator does not support target {target}")
        }
        Failure::UnsupportedBackendArtifact(artifact) => format!(
            "the native-code generator cannot produce {} output",
            format_english_artifact_kind(*artifact),
        ),
        Failure::UnsupportedDebugInformation(mode) => format!(
            "the native-code generator does not support {}",
            format_english_debug_information_mode(*mode),
        ),
        Failure::UnsupportedDebugOutput(mode) => format!(
            "the native-code generator does not support {}",
            format_english_debug_output_mode(*mode),
        ),
        Failure::UnsupportedAssemblySyntax(syntax) => format!(
            "the native-code generator does not support {}",
            format_english_assembly_syntax(*syntax),
        ),
        Failure::InvalidDebugOutput {
            information,
            output,
        } => format!(
            "{} is incompatible with {}",
            format_english_debug_information_mode(*information),
            format_english_debug_output_mode(*output),
        ),
        Failure::MissingRequiredDebugCompanion => {
            "separate debug output requires a debug companion artifact".to_owned()
        }
        Failure::UnexpectedDebugCompanion => {
            "a debug companion was requested without separate debug output".to_owned()
        }
        Failure::MissingLinkableArtifact => {
            "the linked product has no generated native-code input".to_owned()
        }
        Failure::MissingSerializationArtifact(artifact) => format!(
            "the serialization policy requires an unrequested {} artifact",
            format_english_artifact_kind(*artifact),
        ),
        Failure::MissingOutputName(artifact) => format!(
            "the target has no output name for {} artifacts",
            format_english_artifact_kind(*artifact),
        ),
        Failure::InvalidProductName => {
            "the product name is not a valid host filename component".to_owned()
        }
        Failure::InvalidGeneratedFileName(artifact) => format!(
            "the generated name for {} output is not a valid host filename",
            format_english_artifact_kind(*artifact),
        ),
        Failure::MultipleArtifactsForSingleSink => {
            "one unkeyed output destination would receive multiple artifacts".to_owned()
        }
        Failure::MissingExplicitFileName => {
            "the explicit filesystem output has no filename".to_owned()
        }
        Failure::InvalidExplicitFileName => {
            "the explicit filesystem output has an invalid host filename".to_owned()
        }
        Failure::ExplicitOutputSuffixMismatch(artifact) => format!(
            "the explicit filename suffix does not match {} output",
            format_english_artifact_kind(*artifact),
        ),
        Failure::ManagedProductDestinationRequired => {
            "product and companion artifacts require managed product publication".to_owned()
        }
        Failure::ArtifactOrdinalOverflow(artifact) => format!(
            "{} output exceeds its stable ordinal range",
            format_english_artifact_kind(*artifact),
        ),
        Failure::OutputCollision(sink) => format!(
            "multiple artifacts resolve to output destination {}",
            format_english_output_sink(sink),
        ),
    }
}

fn format_english_package_interface_failure(
    failure: &bray_diagnostics::DiagnosticPackageInterfaceFailure,
) -> String {
    use bray_diagnostics::DiagnosticPackageInterfaceFailure as Failure;

    use crate::catalog::english::argument::interface::format_english_interface_symbol_identity;

    match failure {
        Failure::Unavailable => {
            "package-interface output was requested without export configuration".to_owned()
        }
        Failure::InvalidCompilation => {
            "source or semantic errors prevent package-interface export".to_owned()
        }
        Failure::InvalidCompilationCause { reason, context } => {
            let message = match *reason {
                "invalid_compilation_capacity" => {
                    "the library interface exceeds a supported declaration or expression limit"
                }
                "invalid_compilation_runtime_identity_mismatch" => {
                    "the exported declarations require different runtimes"
                }
                "invalid_compilation_runtime_abi_mismatch" => {
                    "the exported declarations require incompatible runtime interfaces"
                }
                "invalid_compilation_runtime_frame_abi_mismatch" => {
                    "the exported declarations require incompatible execution interfaces"
                }
                "invalid_compilation_runtime_target_mismatch" => {
                    "the exported declarations require different target platforms"
                }
                "invalid_compilation_runtime_panic_abi_mismatch" => {
                    "the exported declarations require incompatible panic interfaces"
                }
                _ => {
                    "the requested library identity or product does not match the selected compilation"
                }
            };

            super::super::native::format_failure_detail(message, context)
        }
        Failure::SemanticValueStoreCreate => format_english_package_interface_store_create(),
        Failure::SemanticValue(failure) => {
            format_english_package_interface_semantic_value(*failure)
        }
        Failure::RecoveredPublicSymbol(kind) => {
            format!("a recovered public {kind} declaration has no stable external identity")
        }
        Failure::ConstantCallableEvaluation { declaration, cause } => format!(
            "constant evaluation for {} could not complete because {}",
            format_english_interface_symbol_identity(declaration),
            format_english_emission_evaluation_failure(cause),
        ),
        Failure::ExecutableTemplateEvaluation { declaration, cause } => format!(
            "native preparation for {} could not complete because {}",
            format_english_interface_symbol_identity(declaration),
            format_english_emission_evaluation_failure(cause),
        ),
        Failure::DeclarationDiscoveryFailure { .. } => {
            String::from("package-interface export could not complete declaration discovery")
        }
        Failure::ImplementationContentTooLarge => {
            "the implementation payload exceeds the representable artifact length".to_owned()
        }
    }
}

fn format_english_package_interface_store_create() -> String {
    String::from(
        "semantic-value store identity capacity was exhausted while preparing the package interface",
    )
}

fn format_english_package_interface_semantic_value(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> String {
    format!(
        "package-interface preparation failed because {}",
        super::super::native::format_english_semantic_value_failure_detail(failure),
    )
}

fn format_english_emission_codegen_failure(
    failure: &bray_diagnostics::DiagnosticEmissionCodegenFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionCodegenFailure as Failure;

    match failure {
        Failure::Incomplete => {
            "native-code generation could not supply all requested artifact content".to_owned()
        }
        Failure::Request(unit) => format_unit_failure("could not be requested", unit),
        Failure::Generation(unit) => {
            format_unit_failure("failed while generating native code", unit)
        }
        Failure::MergeReadFailed { artifact, error } => format!(
            "{} artifact #{} could not be read because {}",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
            format_english_io_error_kind(*error),
        ),
        Failure::MergeLengthMismatch {
            artifact,
            expected,
            actual,
        } => format!(
            "{} artifact #{} contains {actual} bytes, but its producer declared {expected} bytes",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
        ),
        Failure::MergeDigestMismatch {
            artifact,
            expected,
            actual,
        } => format!(
            "{} artifact #{} has digest {}, but its producer declared {}",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
            format_english_artifact_digest(actual),
            format_english_artifact_digest(expected),
        ),
        Failure::MergeInvalidContent(artifact) => format_artifact_failure(
            "validated native-code content cannot form the requested artifact",
            *artifact,
        ),
    }
}

fn format_english_emission_staging_failure(
    failure: &bray_diagnostics::DiagnosticEmissionStagingFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionStagingFailure as Failure;

    let (message, artifact) = match failure {
        Failure::Incomplete => {
            return "native link inputs or outputs could not be staged".to_owned();
        }
        Failure::DuplicateContribution(artifact) => (
            "more than one generated result claims the staged artifact",
            artifact,
        ),
        Failure::MissingContribution(artifact) => (
            "the staged artifact has no complete generated content",
            artifact,
        ),
        Failure::InvalidContribution(artifact) => (
            "the staged content does not match its requested producer",
            artifact,
        ),
        Failure::UnexpectedContribution(artifact) => (
            "the generated content does not belong to a staged artifact",
            artifact,
        ),
        Failure::UnsupportedOutput(artifact) => (
            "the requested linker output has no native artifact category",
            artifact,
        ),
        Failure::InvalidPath(artifact) => {
            ("the private staging path is not representable", artifact)
        }
    };

    format_artifact_failure(message, *artifact)
}

fn format_english_emission_link_plan_failure(
    failure: &bray_diagnostics::DiagnosticEmissionLinkPlanFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionLinkPlanFailure as Failure;

    match failure {
        Failure::Incomplete => "a complete native link plan could not be formed".to_owned(),
        Failure::MissingLinkedProduct => "the output request has no linked product".to_owned(),
        Failure::MissingLinker => "the linked product has no selected linker".to_owned(),
        Failure::UnexpectedLinker => {
            "link inputs were supplied for a product without linked output".to_owned()
        }
        Failure::DuplicateStagedArtifact(artifact) => format_artifact_failure(
            "the native input artifact was staged more than once",
            *artifact,
        ),
        Failure::MissingStagedArtifact(artifact) => format_artifact_failure(
            "the requested native input artifact was not staged",
            *artifact,
        ),
        Failure::UnexpectedStagedArtifact(artifact) => format_artifact_failure(
            "the artifact was staged without a matching native input",
            *artifact,
        ),
        Failure::UnsupportedStagedArtifact(artifact) => format_artifact_failure(
            "the staged artifact has no native linker representation",
            *artifact,
        ),
        Failure::DuplicateOutputStaging(artifact) => format_artifact_failure(
            "the linked output has more than one staging destination",
            *artifact,
        ),
        Failure::MissingOutputStaging(artifact) => {
            format_artifact_failure("the linked output has no staging destination", *artifact)
        }
        Failure::UnexpectedOutputStaging(artifact) => {
            format_artifact_failure("the staging destination has no linked artifact", *artifact)
        }
        Failure::OutputKindMismatch { artifact, actual } => format!(
            "{} artifact #{} was requested as {} linked output",
            format_english_artifact_kind(artifact.kind()),
            artifact.ordinal(),
            format_english_linked_artifact_kind(*actual),
        ),
        Failure::InvalidStartupInputKind(kind) => format!(
            "startup input has incompatible category {}",
            format_english_link_input_kind(*kind),
        ),
        Failure::InvalidNativeInputKind(kind) => format!(
            "resolved native input has incompatible category {}",
            format_english_link_input_kind(*kind),
        ),
        Failure::InvalidTerminationInputKind(kind) => format!(
            "termination input has incompatible category {}",
            format_english_link_input_kind(*kind),
        ),
        Failure::InputOrdinalOverflow => {
            "the native input count exceeds the identity range".to_owned()
        }
        Failure::OutputOrdinalOverflow => {
            "the linked output count exceeds the staging identity range".to_owned()
        }
        Failure::InputEmptyPath => "a file-backed native input has an empty path".to_owned(),
        Failure::InputSourceKindMismatch => {
            "a native input source does not match its category".to_owned()
        }
        Failure::WholeArchiveRequiresArchive => {
            "whole-archive treatment was requested for a non-archive input".to_owned()
        }
        Failure::OutputEmptyPath => "a linked output staging path is empty".to_owned(),
        Failure::MissingInputs => "the native link plan contains no inputs".to_owned(),
        Failure::DuplicateInput(ordinal) => {
            format!("native link input #{ordinal} appears more than once")
        }
        Failure::DuplicateSearchPath => {
            "the native link plan contains a duplicate search path".to_owned()
        }
        Failure::MissingRuntimeComponent => {
            "the selected runtime has no corresponding native input".to_owned()
        }
        Failure::UnexpectedRuntimeComponent => {
            "a runtime component is present without a selected runtime".to_owned()
        }
        Failure::RuntimeArtifactMismatch => {
            "a runtime component belongs to another runtime artifact".to_owned()
        }
        Failure::MissingPrimaryOutput => {
            "the native link plan has no required primary output".to_owned()
        }
        Failure::MultiplePrimaryOutputs => {
            "the native link plan has more than one primary output".to_owned()
        }
        Failure::OptionalPrimaryOutput => {
            "the native link plan marks its primary output as optional".to_owned()
        }
        Failure::DuplicateOutput(ordinal) => {
            format!("linked output #{ordinal} appears more than once")
        }
        Failure::OutputPathCollision { first, second } => {
            format!("linked outputs #{first} and #{second} use the same staging path")
        }
        Failure::MissingStartupMode => "the linked product has no native startup mode".to_owned(),
        Failure::UnexpectedStartupMode => {
            "this product category cannot select a native startup mode".to_owned()
        }
        Failure::MissingStartupInput => {
            "explicit startup ownership requires a startup object".to_owned()
        }
        Failure::UnexpectedStartupInput(ordinal) => {
            format!("startup ownership does not permit explicit input #{ordinal}")
        }
        Failure::MissingDebugCompanion => {
            "debug policy requires a staged debug companion".to_owned()
        }
        Failure::UnexpectedDebugCompanion => {
            "a debug companion was staged without companion debug policy".to_owned()
        }
        Failure::IncompatibleOutputKind { product, artifact } => format!(
            "{} output is incompatible with a {} linked product",
            format_english_linked_artifact_kind(*artifact),
            format_english_linked_product_kind(*product),
        ),
        Failure::InvalidOutputPath(path) => {
            format!("linked output path {} is invalid", path.display())
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticEmissionEvaluationFailure, DiagnosticInterfaceSymbolIdentity,
        DiagnosticPackageInterfaceFailure,
    };

    #[test]
    fn package_evaluation_reports_the_declaration_and_capacity_cause() {
        let message = super::format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                declaration: DiagnosticInterfaceSymbolIdentity::Package("example.run".to_owned()),
                cause: DiagnosticEmissionEvaluationFailure::MirCapacity,
            },
        );

        assert!(message.contains("example.run"));
        assert!(message.contains("identity limit"));
        assert!(!message.contains("internal compiler error"));
    }
}
