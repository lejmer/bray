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
        DiagnosticStandardLibraryManifestProblem::BundleDigestMismatch => "bundle digest mismatch",
        DiagnosticStandardLibraryManifestProblem::LengthExceeded => {
            "value exceeds the manifest size limit"
        }
    }
}
