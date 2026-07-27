use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticBag, DiagnosticId,
    DiagnosticInterfaceLimit, DiagnosticInterfaceSection, DiagnosticKind, SeverityKind,
};

use crate::section::InterfaceSectionTag;
use crate::{InterfaceFormatRevision, InterfaceLanguageRevision, InterfaceLimit};

/// Structural or resource failure found while validating an untrusted package interface.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceValidationError {
    /// The artifact does not start with the required file magic.
    InvalidMagic,
    /// The artifact uses a format revision this compiler does not implement.
    UnsupportedFormatRevision {
        /// Format revision found in the artifact.
        actual: InterfaceFormatRevision,
    },
    /// The artifact requires a different language semantic revision.
    UnsupportedLanguageRevision {
        /// Language revision required by the validation policy.
        expected: InterfaceLanguageRevision,
        /// Language revision found in the artifact.
        actual: InterfaceLanguageRevision,
    },
    /// The artifact requires an unsupported byte order or compatibility flag.
    UnsupportedEncoding,
    /// The byte sequence ends before a declared structural value is complete.
    Truncated,
    /// The fixed header or section directory violates the current format contract.
    Malformed,
    /// The semantic content or complete artifact hash does not match.
    HashMismatch,
    /// One section payload does not match its declared checksum.
    SectionChecksumMismatch {
        /// Section whose payload checksum failed validation.
        section: InterfaceSectionTag,
    },
    /// An externally controlled size exceeds its configured ceiling.
    ResourceLimitExceeded {
        /// Resource category that exceeded its ceiling.
        limit: InterfaceLimit,
        /// Externally supplied size or count.
        actual: u64,
        /// Configured maximum size or count.
        maximum: u64,
    },
}

impl InterfaceValidationError {
    /// Converts this validation failure into a locale-neutral diagnostic.
    pub fn into_diagnostic(self, id: DiagnosticId) -> Diagnostic {
        match self {
            Self::InvalidMagic => diagnostic(id, DiagnosticKind::InterfaceInvalidMagic),
            Self::UnsupportedFormatRevision { actual } => {
                diagnostic(id, DiagnosticKind::InterfaceUnsupportedFormatRevision)
                    .with_arg(revision_arg(
                        DiagnosticArgName::ExpectedRevision,
                        u64::from(crate::CURRENT_FORMAT_REVISION.raw()),
                    ))
                    .with_arg(revision_arg(
                        DiagnosticArgName::ActualRevision,
                        u64::from(actual.raw()),
                    ))
            }
            Self::UnsupportedLanguageRevision { expected, actual } => {
                diagnostic(id, DiagnosticKind::InterfaceUnsupportedLanguageRevision)
                    .with_arg(revision_arg(
                        DiagnosticArgName::ExpectedRevision,
                        u64::from(expected.raw()),
                    ))
                    .with_arg(revision_arg(
                        DiagnosticArgName::ActualRevision,
                        u64::from(actual.raw()),
                    ))
            }
            Self::UnsupportedEncoding => {
                diagnostic(id, DiagnosticKind::InterfaceUnsupportedEncoding)
            }
            Self::Truncated => diagnostic(id, DiagnosticKind::InterfaceTruncated),
            Self::Malformed => diagnostic(id, DiagnosticKind::InterfaceMalformed),
            Self::HashMismatch => diagnostic(id, DiagnosticKind::InterfaceHashMismatch),
            Self::SectionChecksumMismatch { section } => {
                diagnostic(id, DiagnosticKind::InterfaceSectionChecksumMismatch).with_arg(
                    DiagnosticArg::new(
                        DiagnosticArgName::InterfaceSection,
                        DiagnosticArgValue::InterfaceSection(diagnostic_section(section)),
                    ),
                )
            }
            Self::ResourceLimitExceeded {
                limit,
                actual,
                maximum,
            } => diagnostic(id, DiagnosticKind::InterfaceResourceLimitExceeded)
                .with_arg(DiagnosticArg::new(
                    DiagnosticArgName::InterfaceLimit,
                    DiagnosticArgValue::InterfaceLimit(diagnostic_limit(limit)),
                ))
                .with_arg(DiagnosticArg::new(
                    DiagnosticArgName::ActualCount,
                    DiagnosticArgValue::Count(actual),
                ))
                .with_arg(DiagnosticArg::maximum_count(maximum)),
        }
    }

    /// Converts this validation failure into a single-entry diagnostic bag.
    pub fn into_diagnostic_bag(self) -> DiagnosticBag {
        DiagnosticBag::single(self.into_diagnostic(DiagnosticId::new(0)))
    }
}

fn diagnostic(id: DiagnosticId, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(id, kind, SeverityKind::Error)
}

fn revision_arg(name: DiagnosticArgName, revision: u64) -> DiagnosticArg {
    DiagnosticArg::new(name, DiagnosticArgValue::Revision(revision))
}

const fn diagnostic_limit(limit: InterfaceLimit) -> DiagnosticInterfaceLimit {
    match limit {
        InterfaceLimit::FileSize => DiagnosticInterfaceLimit::FileSize,
        InterfaceLimit::SectionCount => DiagnosticInterfaceLimit::SectionCount,
        InterfaceLimit::RecordCount => DiagnosticInterfaceLimit::RecordCount,
        InterfaceLimit::StringLength => DiagnosticInterfaceLimit::StringLength,
        InterfaceLimit::BlobLength => DiagnosticInterfaceLimit::BlobLength,
        InterfaceLimit::DecodedAllocation => DiagnosticInterfaceLimit::DecodedAllocation,
        InterfaceLimit::SemanticTypeDepth => DiagnosticInterfaceLimit::SemanticTypeDepth,
        InterfaceLimit::TemplateGraphSize => DiagnosticInterfaceLimit::TemplateGraphSize,
        InterfaceLimit::ExternalReferenceCount => DiagnosticInterfaceLimit::ExternalReferenceCount,
    }
}

const fn diagnostic_section(section: InterfaceSectionTag) -> DiagnosticInterfaceSection {
    match section {
        InterfaceSectionTag::Strings => DiagnosticInterfaceSection::Strings,
        InterfaceSectionTag::PackageMetadata => DiagnosticInterfaceSection::PackageMetadata,
        InterfaceSectionTag::Dependencies => DiagnosticInterfaceSection::Dependencies,
        InterfaceSectionTag::SymbolIdentities => DiagnosticInterfaceSection::SymbolIdentities,
        InterfaceSectionTag::Relationships => DiagnosticInterfaceSection::Relationships,
        InterfaceSectionTag::ExportedLookup => DiagnosticInterfaceSection::ExportedLookup,
        InterfaceSectionTag::SymbolFactDirectory => DiagnosticInterfaceSection::SymbolFactDirectory,
        InterfaceSectionTag::SemanticTypes => DiagnosticInterfaceSection::SemanticTypes,
        InterfaceSectionTag::Constants => DiagnosticInterfaceSection::Constants,
        InterfaceSectionTag::Contracts => DiagnosticInterfaceSection::Contracts,
        InterfaceSectionTag::DeclarationFacts => DiagnosticInterfaceSection::DeclarationFacts,
        InterfaceSectionTag::DeclarationTemplates => {
            DiagnosticInterfaceSection::DeclarationTemplates
        }
        InterfaceSectionTag::Implementations => DiagnosticInterfaceSection::Implementations,
        InterfaceSectionTag::TargetDependencies => DiagnosticInterfaceSection::TargetDependencies,
        InterfaceSectionTag::SourceProvenance => DiagnosticInterfaceSection::SourceProvenance,
        InterfaceSectionTag::SupportGraph => DiagnosticInterfaceSection::SupportGraph,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticId, DiagnosticKind, SeverityKind};

    use super::InterfaceValidationError;
    use crate::{
        InterfaceFormatRevision, InterfaceLanguageRevision, InterfaceLimit, InterfaceSectionTag,
    };

    #[test]
    fn validation_failures_publish_exact_structured_diagnostic_kinds() {
        let cases = [
            (
                InterfaceValidationError::InvalidMagic,
                DiagnosticKind::InterfaceInvalidMagic,
                0,
            ),
            (
                InterfaceValidationError::UnsupportedFormatRevision {
                    actual: InterfaceFormatRevision::new(2),
                },
                DiagnosticKind::InterfaceUnsupportedFormatRevision,
                2,
            ),
            (
                InterfaceValidationError::UnsupportedLanguageRevision {
                    expected: InterfaceLanguageRevision::new(1),
                    actual: InterfaceLanguageRevision::new(2),
                },
                DiagnosticKind::InterfaceUnsupportedLanguageRevision,
                2,
            ),
            (
                InterfaceValidationError::UnsupportedEncoding,
                DiagnosticKind::InterfaceUnsupportedEncoding,
                0,
            ),
            (
                InterfaceValidationError::Truncated,
                DiagnosticKind::InterfaceTruncated,
                0,
            ),
            (
                InterfaceValidationError::Malformed,
                DiagnosticKind::InterfaceMalformed,
                0,
            ),
            (
                InterfaceValidationError::HashMismatch,
                DiagnosticKind::InterfaceHashMismatch,
                0,
            ),
            (
                InterfaceValidationError::SectionChecksumMismatch {
                    section: InterfaceSectionTag::Strings,
                },
                DiagnosticKind::InterfaceSectionChecksumMismatch,
                1,
            ),
            (
                InterfaceValidationError::ResourceLimitExceeded {
                    limit: InterfaceLimit::FileSize,
                    actual: 2,
                    maximum: 1,
                },
                DiagnosticKind::InterfaceResourceLimitExceeded,
                3,
            ),
        ];

        for (error, expected_kind, expected_arg_count) in cases {
            let diagnostic = error.into_diagnostic(DiagnosticId::new(0));

            assert_eq!(diagnostic.kind(), expected_kind);
            assert_eq!(diagnostic.severity(), SeverityKind::Error);
            assert_eq!(diagnostic.args().len(), expected_arg_count);
        }
    }
}
