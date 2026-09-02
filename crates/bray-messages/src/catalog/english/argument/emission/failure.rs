use super::super::super::format_internal_compiler_error;
use super::super::interface::format_english_interface_symbol_graph_problem;
use super::super::native::{
    format_artifact_failure, format_english_emission_evaluation_failure, format_unit_failure,
};
use super::super::source::{
    format_english_artifact_digest, format_english_artifact_kind, format_english_io_error_kind,
    format_english_output_sink,
};
use super::kind::{
    format_english_artifact_requirement, format_english_assembly_syntax,
    format_english_debug_information_mode, format_english_debug_output_mode,
    format_english_link_input_kind, format_english_linked_artifact_kind,
    format_english_linked_product_kind, format_english_product_kind,
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
        Failure::LinkPlan(failure) => format_english_emission_link_plan_failure(failure),
        Failure::Evaluation(failure) => format_english_emission_evaluation_failure(failure),
        Failure::TestCatalog(_) => {
            format_internal_compiler_error("the test catalog could not be encoded")
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
        Failure::MissingExecutableHost => {
            "the executable output does not include its generated startup code".to_owned()
        }
        Failure::MissingRootFrame(identity) => format!(
            "the asynchronous executable plan does not request protected root frame {}",
            format_english_artifact_digest(identity),
        ),
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
        Failure::BackendRequestEmpty => {
            "a native-code work item has no requested outputs".to_owned()
        }
        Failure::BackendRequestForeignUnit(artifact) => format_artifact_failure(
            "a generated output belongs to another native-code work item",
            *artifact,
        ),
        Failure::BackendRequestDuplicateIdentity(artifact) => format_artifact_failure(
            "a generated output identity appears more than once",
            *artifact,
        ),
        Failure::BackendRequestMissingLinkableArtifact { kind, requirement } => format!(
            "the native-code request lacks its {} {} artifact",
            format_english_artifact_requirement(*requirement),
            format_english_artifact_kind(*kind),
        ),
        Failure::BackendRequestMissingRequiredDebugCompanion => {
            "the native-code request lacks its required debug companion".to_owned()
        }
        Failure::BackendRequestUnexpectedDebugCompanion => {
            "the native-code request contains an inapplicable debug companion".to_owned()
        }
        Failure::BackendRequestUnexpectedAssemblySyntax => {
            "the native-code request selects assembly syntax without assembly output".to_owned()
        }
        Failure::BackendRequestUnexpectedBitcodeSemantics => {
            "link-time optimization requires a linked executable or library, but this build requests only standalone artifacts"
                .to_owned()
        }
        Failure::InconsistentPlan => {
            "the derived artifacts do not match the complete output request".to_owned()
        }
    }
}

fn format_english_package_interface_failure(
    failure: &bray_diagnostics::DiagnosticPackageInterfaceFailure,
) -> String {
    use bray_diagnostics::DiagnosticPackageInterfaceFailure as Failure;

    use crate::catalog::english::argument::interface::{
        format_english_interface_symbol_identity, format_english_interface_symbol_reference,
    };

    match failure {
        Failure::Unavailable => {
            "package-interface output was requested without export configuration".to_owned()
        }
        Failure::InvalidCompilation => {
            "source or semantic errors prevent package-interface export".to_owned()
        }
        Failure::SemanticValueStoreCreate => format_english_package_interface_store_create(),
        Failure::SemanticValue(failure) => format_english_package_interface_semantic_value(*failure),
        Failure::RecoveredPublicSymbol(kind) => {
            format!("a recovered public {kind} declaration has no stable external identity")
        }
        Failure::ConstantCallableEvaluation { declaration, cause } => format!(
            "constant body preparation failed for {} because {}",
            format_english_interface_symbol_identity(declaration),
            format_english_emission_evaluation_failure(cause)
        ),
        Failure::ExecutableTemplateEvaluation { declaration, cause } => format!(
            "executable-template preparation failed for {} because {}",
            format_english_interface_symbol_identity(declaration),
            format_english_emission_evaluation_failure(cause)
        ),
        Failure::IncompletePublicDeclaration(kind) => format_internal_compiler_error(format!(
            "complete data was unavailable for a reachable public {kind} declaration"
        )),
        Failure::DuplicateSymbol(kind) => {
            format!("the export surface contains a duplicate stable {kind} identity")
        }
        Failure::MissingSymbol(kind) => {
            format!("the export surface references an unselected {kind} symbol")
        }
        Failure::SymbolCountOverflow => {
            "the export surface exceeds the compact symbol identity range".to_owned()
        }
        Failure::SymbolGraph(problem) => {
            format_english_interface_symbol_graph_problem(problem)
        }
        Failure::MissingDeclarationData(declaration) => format_internal_compiler_error(format!(
            "complete declaration data was unavailable for exported {}",
            format_english_interface_symbol_identity(declaration)
        )),
        Failure::LostDeclarationReference {
            declaration,
            table,
            reference,
        } => format_english_package_interface_declaration_reference_failure(
            PackageInterfaceDeclarationReferenceFailure::Missing,
            declaration.as_ref(),
            table,
            *reference,
        ),
        Failure::DuplicateDeclarationIdentity {
            first,
            second,
            identity,
        } => format!(
            "exported declarations {} and {} both resolve to stable identity {}",
            format_english_interface_symbol_identity(first),
            format_english_interface_symbol_identity(second),
            format_english_interface_symbol_identity(identity)
        ),
        Failure::RecursiveDeclarationData {
            declaration,
            table,
            reference,
        } => format_english_package_interface_declaration_reference_failure(
            PackageInterfaceDeclarationReferenceFailure::Recursive,
            declaration.as_ref(),
            table,
            *reference,
        ),
        Failure::DeclarationDiscoveryFailure { cause, cycle } => {
            if cycle.is_empty() {
                format_internal_compiler_error(format!(
                    "package-interface export failed because {}",
                    format_english_emission_evaluation_failure(cause)
                ))
            } else {
                format_internal_compiler_error(format!(
                    "package-interface export has mutually dependent internal requests: {}",
                    cycle.join(" -> ")
                ))
            }
        }
        Failure::MissingPackageReference { table, reference } => {
            format_internal_compiler_error(format!(
                "{table} entry {reference} has no associated package interface"
            ))
        }
        Failure::MisassignedPackageRecord(table) => format_internal_compiler_error(format!(
            "package-level {table} data belongs to one exported declaration"
        )),
        Failure::ConflictingDeclarationRecord { kind, owner } => format_internal_compiler_error(format!(
            "conflicting {kind} data exists for {}",
            format_english_interface_symbol_reference(owner)
        )),
        Failure::RecursiveDeclarationReference(table) => {
            format_internal_compiler_error(format!(
                "the {table} data table contains a recursive reference"
            ))
        }
        Failure::SemanticTableOverflow { table, maximum } => format!(
            "package-interface export requires more than {maximum} {table} records, which the file format cannot represent"
        ),
        Failure::DuplicateConstantCallableBody(owner) => {
            format!("two constant callable bodies claim callable record {owner}")
        }
        Failure::DuplicateExecutableTemplate(owner) => {
            format!("two executable templates claim callable record {owner}")
        }
        Failure::InvalidExecutableTemplateFamily(owner) => format!(
            "the executable templates for callable record {owner} do not form one contiguous root-first family"
        ),
        Failure::DuplicateNativeBoundary(owner) => {
            format!("two native boundaries claim function record {owner}")
        }
        Failure::ImplementationDuplicateCallableBody(owner) => {
            format!("two implementation payloads claim callable record {owner}")
        }
        Failure::ImplementationDuplicateExecutableTemplate(owner) => {
            format!("two implementation templates claim declaration record {owner}")
        }
        Failure::ImplementationInvalidExecutableTemplateFamily(owner) => format!(
            "the implementation templates for declaration record {owner} do not form one contiguous root-first family"
        ),
        Failure::ImplementationDuplicateNativeBoundary(owner) => {
            format!("two implementation native boundaries claim declaration record {owner}")
        }
        Failure::ImplementationDuplicateSpecialization => {
            "two implementation payloads claim the same specialization".to_owned()
        }
        Failure::ImplementationSpecializationIdentityMismatch => {
            "an implementation specialization does not belong to this package configuration and dependency set".to_owned()
        }
        Failure::ImplementationInvalidExecutableOwner(owner) => format!(
            "executable template owner record {owner} is missing or cannot own executable code"
        ),
        Failure::ImplementationInvalidNativeBoundaryOwner(owner) => {
            format!("native boundary owner record {owner} is missing or is not a function")
        }
        Failure::ImplementationInvalidCallableOwner(owner) => format!(
            "implementation owner record {owner} is missing, non-callable, or has the wrong body category"
        ),
        Failure::ImplementationContentTooLarge => {
            "the implementation payload exceeds the representable artifact length".to_owned()
        }
    }
}

enum PackageInterfaceDeclarationReferenceFailure {
    Missing,
    Recursive,
}

fn format_english_package_interface_declaration_reference_failure(
    failure: PackageInterfaceDeclarationReferenceFailure,
    declaration: Option<&bray_diagnostics::DiagnosticInterfaceSymbolIdentity>,
    table: &str,
    reference: u32,
) -> String {
    use crate::catalog::english::argument::interface::format_english_interface_symbol_identity;

    let detail = match (failure, declaration) {
        (PackageInterfaceDeclarationReferenceFailure::Missing, Some(declaration)) => format!(
            "{table} entry {reference} was unavailable while preparing exported {}",
            format_english_interface_symbol_identity(declaration)
        ),
        (PackageInterfaceDeclarationReferenceFailure::Missing, None) => format!(
            "{table} entry {reference} was unavailable while preparing an exported declaration"
        ),
        (PackageInterfaceDeclarationReferenceFailure::Recursive, Some(declaration)) => format!(
            "recursive {table} entry {reference} exists while preparing exported {}",
            format_english_interface_symbol_identity(declaration)
        ),
        (PackageInterfaceDeclarationReferenceFailure::Recursive, None) => format!(
            "recursive {table} entry {reference} exists while preparing the package interface"
        ),
    };

    format_internal_compiler_error(detail)
}

fn format_english_package_interface_store_create() -> String {
    format_internal_compiler_error(
        "semantic-value store identity capacity was exhausted while preparing the package interface",
    )
}

fn format_english_package_interface_semantic_value(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> String {
    format_internal_compiler_error(format!(
        "package-interface preparation failed because {}",
        super::super::native::format_english_semantic_value_failure_detail(failure),
    ))
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticEmissionEvaluationFailure, DiagnosticInterfaceSymbolIdentity,
        DiagnosticPackageInterfaceFailure,
    };

    use super::format_english_package_interface_failure;

    fn package(name: &str) -> DiagnosticInterfaceSymbolIdentity {
        DiagnosticInterfaceSymbolIdentity::Package(name.to_owned())
    }

    #[test]
    fn package_interface_failures_render_actionable_context() {
        let conflict = format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::DuplicateDeclarationIdentity {
                first: package("example.first"),
                second: package("example.second"),
                identity: package("example.shared"),
            },
        );

        assert_eq!(
            conflict,
            "exported declarations 'example.first' and 'example.second' both resolve to stable identity 'example.shared'"
        );

        let recursive = format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::RecursiveDeclarationData {
                declaration: Some(package("example.value")),
                table: "type".to_owned(),
                reference: 19,
            },
        );

        assert!(recursive.starts_with(crate::catalog::english::INTERNAL_COMPILER_ERROR));
        assert!(recursive.contains("type entry 19"));
        assert!(recursive.contains("example.value"));

        let cycle = format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::DeclarationDiscoveryFailure {
                cause: DiagnosticEmissionEvaluationFailure::Cycle(
                    bray_diagnostics::DiagnosticEvaluationFailureDetail::new("cycle", []),
                ),
                cycle: ["declaration_table".to_owned(), "symbol_graph".to_owned()].into(),
            },
        );

        assert!(cycle.starts_with(crate::catalog::english::INTERNAL_COMPILER_ERROR));
        assert!(cycle.contains("declaration_table -> symbol_graph"));

        let infrastructure = format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::DeclarationDiscoveryFailure {
                cause: DiagnosticEmissionEvaluationFailure::Infrastructure,
                cycle: Box::new([]),
            },
        );

        assert!(infrastructure.starts_with(crate::catalog::english::INTERNAL_COMPILER_ERROR));
        assert!(!infrastructure.contains("compiler evaluation"));
        assert!(!infrastructure.contains(';'));

        let executable = format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                declaration: package("example.run"),
                cause: DiagnosticEmissionEvaluationFailure::ConstantCallableBodyUnavailable,
            },
        );

        assert!(executable.contains("example.run"));
        assert!(executable.contains("selected constant callable has no available body"));

        let overflow = format_english_package_interface_failure(
            &DiagnosticPackageInterfaceFailure::SemanticTableOverflow {
                table: "type".to_owned(),
                maximum: u32::MAX,
            },
        );

        assert_eq!(
            overflow,
            "package-interface export requires more than 4294967295 type records, which the file format cannot represent"
        );
    }
}

fn format_english_emission_codegen_failure(
    failure: &bray_diagnostics::DiagnosticEmissionCodegenFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionCodegenFailure as Failure;

    match failure {
        Failure::Incomplete => {
            "native-code generation could not supply all requested artifact content".to_owned()
        }
        Failure::DuplicateUnit(unit) => format_unit_failure("appears more than once", unit),
        Failure::DuplicateMappings(unit) => {
            format_unit_failure("has more than one source-to-native mapping set", unit)
        }
        Failure::MissingUnit(unit) => format_unit_failure("is unavailable", unit),
        Failure::MissingMappings(unit) => {
            format_unit_failure("has no source-to-native mappings", unit)
        }
        Failure::Request(unit) => format_unit_failure("could not be requested", unit),
        Failure::Generation(unit) => {
            format_unit_failure("failed while generating native code", unit)
        }
        Failure::MergeDuplicateUnit(unit) => {
            format_unit_failure("has more than one generated result", unit)
        }
        Failure::MergeMissingUnit(unit) => format_unit_failure("has no generated result", unit),
        Failure::MergeUnrequestedUnit(unit) => {
            format_unit_failure("was produced without an emission-plan request", unit)
        }
        Failure::MergeBackendMismatch(unit) => {
            format_unit_failure("was produced by another native-code generator", unit)
        }
        Failure::MergeCapabilityMismatch(unit) => {
            format_unit_failure("uses another native-code generator revision", unit)
        }
        Failure::MergeTargetMismatch(unit) => {
            format_unit_failure("was produced for another target", unit)
        }
        Failure::MergeMissingArtifact(artifact) => format_artifact_failure(
            "a required native-code artifact was not produced",
            *artifact,
        ),
        Failure::MergeUnrequestedArtifact(artifact) => format_artifact_failure(
            "the native-code generator produced an artifact absent from the output request",
            *artifact,
        ),
        Failure::MergeArtifactKindMismatch(artifact) => format_artifact_failure(
            "the generated artifact category disagrees with the output request",
            *artifact,
        ),
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
        Failure::InvalidContent(artifact) => (
            "the staged bytes do not match the generated-content contract",
            artifact,
        ),
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
        Failure::RuntimeContractMismatch => {
            "the selected runtime archive does not match the executable host".to_owned()
        }
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
        Failure::MissingExecutableHost => {
            "the executable link plan has no generated host contract".to_owned()
        }
        Failure::UnexpectedExecutableHost => {
            "a non-executable link plan contains a host contract".to_owned()
        }
        Failure::ExecutableHostProductMismatch => {
            "the executable host belongs to another product".to_owned()
        }
        Failure::ExecutableHostTargetMismatch => {
            "the executable host was generated for another target".to_owned()
        }
        Failure::UnexpectedEntryPoint => {
            "this product category cannot define an explicit native entry point".to_owned()
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
