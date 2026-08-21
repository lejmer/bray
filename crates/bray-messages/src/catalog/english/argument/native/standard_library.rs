pub(crate) const fn format_english_standard_library_manifest_problem(
    problem: bray_diagnostics::DiagnosticStandardLibraryManifestProblem,
) -> &'static str {
    use bray_diagnostics::DiagnosticStandardLibraryManifestProblem;

    match problem {
        DiagnosticStandardLibraryManifestProblem::Malformed => "malformed JSON or schema",
        DiagnosticStandardLibraryManifestProblem::NonCanonicalEncoding => "non-canonical encoding",
        DiagnosticStandardLibraryManifestProblem::InvalidDigest => "invalid digest",
        DiagnosticStandardLibraryManifestProblem::InvalidInterfaceArtifact => {
            "invalid interface artifact"
        }
        DiagnosticStandardLibraryManifestProblem::InvalidImplementationArtifact => {
            "invalid implementation artifact"
        }
        DiagnosticStandardLibraryManifestProblem::InvalidTargetArtifact => {
            "artifact outside its target directory"
        }
        DiagnosticStandardLibraryManifestProblem::InvalidArtifactPath => {
            "non-canonical artifact path"
        }
        DiagnosticStandardLibraryManifestProblem::MissingArtifact => "missing required artifact",
        DiagnosticStandardLibraryManifestProblem::DuplicateArtifact => "duplicate artifact record",
        DiagnosticStandardLibraryManifestProblem::DuplicateArtifactPath => {
            "duplicate artifact path"
        }
        DiagnosticStandardLibraryManifestProblem::MissingTarget => "missing target inventory",
        DiagnosticStandardLibraryManifestProblem::DuplicateTarget => "duplicate target inventory",
        DiagnosticStandardLibraryManifestProblem::InvalidIdentity => {
            "invalid standard library identity"
        }
        DiagnosticStandardLibraryManifestProblem::InvalidNativeLink => {
            "invalid native link requirement"
        }
        DiagnosticStandardLibraryManifestProblem::InvalidPlatformServices => {
            "native support library does not identify its operations"
        }
        DiagnosticStandardLibraryManifestProblem::DuplicatePlatformService => {
            "native operation appears in multiple support libraries"
        }
        DiagnosticStandardLibraryManifestProblem::InvalidOptimizationMetadata(problem) => {
            format_english_optimization_metadata_problem(problem)
        }
        DiagnosticStandardLibraryManifestProblem::InvalidOptimizationFallback => {
            "native optimization fallback does not match the packaged artifact"
        }
        DiagnosticStandardLibraryManifestProblem::BundleDigestMismatch => "bundle digest mismatch",
        DiagnosticStandardLibraryManifestProblem::LengthExceeded => {
            "value exceeds the manifest size limit"
        }
    }
}

const fn format_english_optimization_metadata_problem(
    problem: bray_diagnostics::DiagnosticStandardLibraryOptimizationMetadataProblem,
) -> &'static str {
    use bray_diagnostics::DiagnosticStandardLibraryOptimizationMetadataProblem;

    match problem {
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingForArchive => {
            "optimization archive is missing optimization metadata"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::AttachedToUnsupportedArtifact => {
            "optimization metadata is attached to an artifact that is not an optimization archive"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedSemantics => {
            "optimization semantics are not supported"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedProducerKind => {
            "optimization producer kind is not supported"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingProducerImplementation => {
            "optimization producer implementation is empty"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingProducerImplementationRevision => {
            "optimization producer implementation revision is empty"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingToolchain => {
            "optimization toolchain is empty"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingToolchainRevision => {
            "optimization toolchain revision is empty"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingTargetTriple => {
            "optimization target triple is empty"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingDataLayout => {
            "optimization data layout is empty"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedRelocationModel => {
            "optimization relocation model is not supported"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedCodeModel => {
            "optimization code model is not supported"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::NonCanonicalFallbackPath => {
            "optimization fallback path is not canonical and relative"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::ZeroModuleCount => {
            "optimization module count is zero"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::InvalidPreservationRoot => {
            "optimization preservation root is not a valid native symbol"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::UnsupportedLifecycleRoot => {
            "optimization lifecycle root is not supported"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::UnknownPlatformService => {
            "optimization platform service is unknown"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::NonCanonicalDependencyPath => {
            "optimization dependency path is not canonical and relative"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::InvalidPartition => {
            "optimization partition identity is invalid"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::DuplicatePartition => {
            "optimization partition is duplicated"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::RuntimeAbiMismatch => {
            "optimization runtime ABI does not match its target inventory"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::TargetMismatch => {
            "optimization target does not match its target inventory"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::CompatibilityMismatch => {
            "optimization archives have incompatible target settings"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::ToolchainMismatch => {
            "optimization archives were produced by different toolchains"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingDependencyArtifact => {
            "optimization dependency metadata does not match a packaged artifact"
        }
        DiagnosticStandardLibraryOptimizationMetadataProblem::MissingBrayPartition => {
            "target inventory is missing the Bray standard library optimization partition"
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticStandardLibraryManifestProblem,
        DiagnosticStandardLibraryOptimizationMetadataProblem,
    };

    use super::format_english_standard_library_manifest_problem;

    #[test]
    fn optimization_metadata_problem_identifies_the_failed_contract() {
        let problem = DiagnosticStandardLibraryManifestProblem::InvalidOptimizationMetadata(
            DiagnosticStandardLibraryOptimizationMetadataProblem::MissingToolchainRevision,
        );

        assert_eq!(
            format_english_standard_library_manifest_problem(problem),
            "optimization toolchain revision is empty"
        );
    }
}
