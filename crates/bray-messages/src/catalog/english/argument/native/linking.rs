use super::super::source::format_english_quoted_text;
use bray_diagnostics::{DiagnosticLinkOptimizationReportProblem, DiagnosticLinkRequirement};

pub(super) fn format_english_native_link_input_failure(
    failure: &bray_diagnostics::DiagnosticNativeLinkInputFailure,
) -> String {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure as Failure;

    match failure {
        Failure::UnsupportedStandardLibraryArtifact {
            path,
            artifact_kind,
        } => format!(
            "standard-library artifact '{path}' has unsupported native link category '{}'",
            english_standard_library_artifact_kind(artifact_kind),
        ),
        Failure::InvalidStandardLibraryArtifact {
            path,
            input_kind,
            cause,
        } => format!(
            "standard-library artifact '{path}' could not form {} link input because {}",
            english_link_input_kind(input_kind),
            english_link_input_cause(cause),
        ),
        Failure::InvalidRequirement {
            name,
            link_kind,
            provenance_kind,
            provenance_identity,
        } => {
            let provider = provenance_identity.as_ref().map_or_else(
                || english_link_provenance(provenance_kind).to_owned(),
                |identity| format!("{} '{identity}'", english_link_provenance(provenance_kind)),
            );

            format!(
                "native link requirement '{name}' for {} from {provider} is invalid",
                english_link_kind(link_kind),
            )
        }
    }
}

fn english_standard_library_artifact_kind(kind: &str) -> &'static str {
    match kind {
        "package_interface" => "package interface",
        "package_implementation" => "package implementation",
        "dependency_metadata" => "dependency metadata",
        "relocatable_object" => "relocatable object",
        "static_library" => "static library",
        "platform_service_library" => "platform service library",
        "optimization_archive" => "optimization archive",
        "shared_library" => "shared library",
        "runtime_artifact" => "runtime artifact",
        _ => "unknown artifact",
    }
}

fn english_link_input_kind(kind: &str) -> &'static str {
    match kind {
        "relocatable_object" => "a relocatable-object",
        "bitcode" => "an LLVM bitcode",
        "archive" => "an archive",
        "startup_object" => "a startup-object",
        "termination_object" => "a termination-object",
        "runtime_component" => "a runtime-component",
        "native_library" => "a native-library",
        "framework" => "a platform-framework",
        _ => "an unknown",
    }
}

fn english_link_input_cause(cause: &str) -> &'static str {
    match cause {
        "empty_file_path" => "its file path is empty",
        "source_kind_mismatch" => "its source does not match the required input category",
        "whole_archive_requires_archive" => "whole-archive treatment requires an archive",
        _ => "its input contract is invalid",
    }
}

fn english_link_provenance(provenance: &str) -> &'static str {
    match provenance {
        "product" => "the product",
        "package" => "package",
        "platform_provider" => "platform provider",
        "target_profile" => "the target profile",
        "runtime" => "runtime",
        "runtime_dependency" => "runtime dependency",
        "host_configuration" => "the host configuration",
        _ => "an unknown provider",
    }
}

fn english_link_kind(kind: &str) -> &'static str {
    match kind {
        "dynamic" => "dynamic linking",
        "static" => "static linking",
        "system" => "system-library linking",
        "framework" => "platform-framework linking",
        _ => "an unknown link category",
    }
}

pub(crate) const fn format_english_link_optimization_report_problem(
    problem: DiagnosticLinkOptimizationReportProblem,
) -> &'static str {
    match problem {
        DiagnosticLinkOptimizationReportProblem::Missing => "the report was not produced",
        DiagnosticLinkOptimizationReportProblem::Inaccessible => {
            "the report could not be read or removed"
        }
        DiagnosticLinkOptimizationReportProblem::Malformed => {
            "its JSON or required fields are malformed"
        }
        DiagnosticLinkOptimizationReportProblem::UnsupportedFormat => {
            "its format number is unsupported"
        }
        DiagnosticLinkOptimizationReportProblem::ToolchainMismatch => {
            "its LLVM toolchain revision differs from the selected linker"
        }
        DiagnosticLinkOptimizationReportProblem::DriverMismatch => {
            "its LLD driver flavor differs from the selected linker"
        }
        DiagnosticLinkOptimizationReportProblem::InconsistentCacheOutcomes => {
            "its cache outcome counters contradict each other"
        }
        DiagnosticLinkOptimizationReportProblem::MissingResourceMeasurement => {
            "its peak resident memory measurement is absent"
        }
    }
}

pub(crate) fn format_english_link_requirement(requirement: &DiagnosticLinkRequirement) -> String {
    use DiagnosticLinkRequirement as Requirement;

    match requirement {
        Requirement::Target(triple) => format_english_quoted_text(triple),
        Requirement::ProductExecutable => "executable product".to_owned(),
        Requirement::ProductSharedLibrary => "shared-library product".to_owned(),
        Requirement::ProductStaticLibrary => "static-library product".to_owned(),
        Requirement::InputRelocatableObject => "relocatable object".to_owned(),
        Requirement::InputBitcode => "LLVM bitcode".to_owned(),
        Requirement::InputArchive => "native archive".to_owned(),
        Requirement::InputStartupObject => "startup object".to_owned(),
        Requirement::InputTerminationObject => "termination object".to_owned(),
        Requirement::InputRuntimeComponent => "Bray runtime component".to_owned(),
        Requirement::InputNativeLibrary => "native library".to_owned(),
        Requirement::InputFramework => "platform framework".to_owned(),
        Requirement::InputModeOrdinary => "ordinary archive treatment".to_owned(),
        Requirement::InputModeWholeArchive => "whole-archive treatment".to_owned(),
        Requirement::OutputExecutable => "executable output".to_owned(),
        Requirement::OutputSharedLibrary => "shared-library output".to_owned(),
        Requirement::OutputStaticLibrary => "static-library output".to_owned(),
        Requirement::OutputImportLibrary => "import-library output".to_owned(),
        Requirement::OutputDebugCompanion => "separate debug-information output".to_owned(),
        Requirement::OutputPlatformCompanion => "platform companion output".to_owned(),
        Requirement::SearchPathLibrary => "native-library search path".to_owned(),
        Requirement::SearchPathFramework => "platform-framework search path".to_owned(),
        Requirement::LinkModelDefault => "target-default linkage".to_owned(),
        Requirement::LinkModelStatic => "static linkage".to_owned(),
        Requirement::LinkModelDynamic => "dynamic linkage".to_owned(),
        Requirement::DeadStripPreserve => "preserve unreachable code".to_owned(),
        Requirement::DeadStripRemoveUnreachable => "remove unreachable code".to_owned(),
        Requirement::SectionGarbageCollectionPreserve => {
            "preserve unreferenced sections".to_owned()
        }
        Requirement::SectionGarbageCollectionRemoveUnreferenced => {
            "remove unreferenced sections".to_owned()
        }
        Requirement::DebugNone => "omit debug information".to_owned(),
        Requirement::DebugEmbedded => "embed debug information".to_owned(),
        Requirement::DebugCompanion => "emit separate debug information".to_owned(),
        Requirement::SubsystemConsole => "console subsystem".to_owned(),
        Requirement::SubsystemWindowed => "windowed subsystem".to_owned(),
        Requirement::SubsystemNative => "native subsystem".to_owned(),
        Requirement::SubsystemWasiCommand => "WASI command".to_owned(),
        Requirement::SubsystemWasiReactor => "WASI reactor".to_owned(),
        Requirement::SymbolEntryPoint => "explicit entry point".to_owned(),
        Requirement::SymbolExportedSymbols => "explicit exported-symbol set".to_owned(),
        Requirement::SymbolRetainedSymbols => "explicit retained-symbol set".to_owned(),
        Requirement::StartupNotApplicable => "no startup ownership".to_owned(),
        Requirement::StartupExplicitInputs => "explicit startup inputs".to_owned(),
        Requirement::StartupPlatformCompilerDriver => "platform compiler-driver startup".to_owned(),
        Requirement::RuntimeExplicitInput => "explicit runtime input".to_owned(),
        Requirement::OptimizationThinLto => "LLVM ThinLTO".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticNativeLinkInputFailure;

    use super::format_english_native_link_input_failure;

    #[test]
    fn native_link_input_failures_translate_machine_keys() {
        let artifact = format_english_native_link_input_failure(
            &DiagnosticNativeLinkInputFailure::InvalidStandardLibraryArtifact {
                path: "lib.a".to_owned(),
                input_kind: "relocatable_object",
                cause: "empty_file_path",
            },
        );

        let requirement = format_english_native_link_input_failure(
            &DiagnosticNativeLinkInputFailure::InvalidRequirement {
                name: "ssl".to_owned(),
                link_kind: "dynamic".to_owned(),
                provenance_kind: "platform_provider",
                provenance_identity: Some("system".to_owned()),
            },
        );

        assert!(artifact.contains("lib.a"));
        assert!(!artifact.contains("relocatable_object"));
        assert!(!artifact.contains("empty_file_path"));
        assert!(requirement.contains("ssl"));
        assert!(requirement.contains("system"));
        assert!(!requirement.contains("platform_provider"));
    }
}
