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
        DiagnosticStandardLibraryManifestProblem::InvalidNativeArtifact(cause) => {
            format_english_native_artifact_problem(cause)
        }
        DiagnosticStandardLibraryManifestProblem::NativeDemandUnresolved => {
            "native unit demand has no provider"
        }
        DiagnosticStandardLibraryManifestProblem::NativeProviderConflict => {
            "native units contain conflicting strong providers"
        }
        DiagnosticStandardLibraryManifestProblem::BundleDigestMismatch => "bundle digest mismatch",
        DiagnosticStandardLibraryManifestProblem::LengthExceeded => {
            "value exceeds the manifest size limit"
        }
    }
}

pub(super) const fn format_english_native_artifact_problem(
    cause: bray_diagnostics::DiagnosticNativeArtifactCause,
) -> &'static str {
    use bray_diagnostics::DiagnosticNativeArtifactCause as Cause;

    match cause {
        Cause::IndexSizeLimitExceeded => "native index exceeds its size limit",
        Cause::IndexMalformed => "native index is malformed",
        Cause::IndexUnsupportedSchema => "native index schema is unsupported",
        Cause::IndexInvalidTarget => "native index target is invalid",
        Cause::IndexInvalidDigest => "native index digest is invalid",
        Cause::IndexInvalidSymbol => "native index symbol is invalid",
        Cause::IndexInvalidLink => "native index link requirement is invalid",
        Cause::IndexDigestMismatch => "native index digest differs from published bytes",
        Cause::PayloadDigestMismatch => "native unit digest differs from published bytes",
        Cause::WrongTarget => "native index targets another platform",
        Cause::WrongProducer => "native index uses another code generation policy",
        Cause::ReadFailure => "native unit could not be read",
        Cause::DuplicateUnit => "native index contains a duplicate unit",
        Cause::InvalidSummary => "native unit summary is invalid",
        Cause::DuplicateDefinition => "native unit defines a symbol twice",
        Cause::InvalidAssociation => "native unit association is invalid",
        Cause::InvalidLinkOption => "native unit link option is invalid",
        Cause::NoncanonicalSummary => "native unit summary is not canonical",
        Cause::MissingCoRetentionMember => "native unit retention member is missing",
        Cause::DuplicateCoRetentionGroup => "native unit retention group is duplicated",
        Cause::InvalidCoRetentionGroup => "native unit retention group is invalid",
        Cause::UnsupportedTarget => "native unit target is unsupported",
        Cause::MissingIndex => "native unit index is missing",
        Cause::MissingUnit => "native unit is missing",
        Cause::UnindexedUnit => "native unit is not indexed",
        Cause::InvalidBinding => "native unit binding is invalid",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticNativeArtifactCause, DiagnosticStandardLibraryManifestProblem};

    use super::format_english_standard_library_manifest_problem;

    #[test]
    fn native_index_policy_mismatch_has_a_specific_message() {
        let problem = DiagnosticStandardLibraryManifestProblem::InvalidNativeArtifact(
            DiagnosticNativeArtifactCause::WrongProducer,
        );

        assert_eq!(
            format_english_standard_library_manifest_problem(problem),
            "native index uses another code generation policy"
        );
    }
}
