use bray_diagnostics::{DiagnosticArgName, DiagnosticKind};

use crate::catalog::{MessageTemplate, MessageTemplatePart};

const INVALID_MAGIC: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "invalid compiled package-interface file identity",
)];
const UNSUPPORTED_FORMAT_REVISION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unsupported package-interface format revision "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualRevision),
    MessageTemplatePart::Text("; expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedRevision),
];
const UNSUPPORTED_LANGUAGE_REVISION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unsupported package-interface language revision "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualRevision),
    MessageTemplatePart::Text("; expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedRevision),
];
const UNSUPPORTED_ENCODING: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package interface requires an unsupported wire encoding",
)];
const TRUNCATED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("package interface is truncated")];
const MALFORMED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package-interface header or section directory is malformed",
)];
const HASH_MISMATCH: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package-interface content or artifact hash does not match",
)];
const SECTION_CHECKSUM_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package-interface section checksum does not match for "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSection),
];
const RESOURCE_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package interface exceeds the configured "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceLimit),
    MessageTemplatePart::Text(" limit: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text("; maximum "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];
const PACKAGE_IDENTITY_MISMATCH: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package interface does not match the selected package",
)];
const PRODUCT_IDENTITY_MISMATCH: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package interface does not match the selected product",
)];
const DEPENDENCY_GRAPH_INVALID: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package-interface dependency graph does not match the selected dependencies",
)];
const SEMANTIC_FACTS_INVALID: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "package-interface semantic facts cannot be loaded",
)];
const COMMAND_USAGE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "usage: cargo xtask package-interface <inspect <path> [--section <name>]... | validate <path>>",
)];
const COMMAND_UNEXPECTED_ACTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unexpected package-interface command: "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
];
const COMMAND_UNEXPECTED_ARGUMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unexpected package-interface argument: "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
];
const COMMAND_MISSING_SECTION_NAME: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "--section requires a package-interface section name",
)];
const COMMAND_UNKNOWN_SECTION_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unknown package-interface section: "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
];
const ARTIFACT_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read package interface "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];
const COMMAND_OUTPUT_FAILED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "could not render structured package-interface output",
)];

pub(super) const fn diagnostic_template(kind: DiagnosticKind) -> MessageTemplate {
    let parts = match kind {
        DiagnosticKind::InterfaceInvalidMagic => INVALID_MAGIC,
        DiagnosticKind::InterfaceUnsupportedFormatRevision => UNSUPPORTED_FORMAT_REVISION,
        DiagnosticKind::InterfaceUnsupportedLanguageRevision => UNSUPPORTED_LANGUAGE_REVISION,
        DiagnosticKind::InterfaceUnsupportedEncoding => UNSUPPORTED_ENCODING,
        DiagnosticKind::InterfaceTruncated => TRUNCATED,
        DiagnosticKind::InterfaceMalformed => MALFORMED,
        DiagnosticKind::InterfaceHashMismatch => HASH_MISMATCH,
        DiagnosticKind::InterfaceSectionChecksumMismatch => SECTION_CHECKSUM_MISMATCH,
        DiagnosticKind::InterfaceResourceLimitExceeded => RESOURCE_LIMIT_EXCEEDED,
        DiagnosticKind::InterfacePackageIdentityMismatch => PACKAGE_IDENTITY_MISMATCH,
        DiagnosticKind::InterfaceProductIdentityMismatch => PRODUCT_IDENTITY_MISMATCH,
        DiagnosticKind::InterfaceDependencyGraphInvalid => DEPENDENCY_GRAPH_INVALID,
        DiagnosticKind::InterfaceSemanticFactsInvalid => SEMANTIC_FACTS_INVALID,
        DiagnosticKind::InterfaceCommandUsage => COMMAND_USAGE,
        DiagnosticKind::InterfaceCommandUnexpectedAction => COMMAND_UNEXPECTED_ACTION,
        DiagnosticKind::InterfaceCommandUnexpectedArgument => COMMAND_UNEXPECTED_ARGUMENT,
        DiagnosticKind::InterfaceCommandMissingSectionName => COMMAND_MISSING_SECTION_NAME,
        DiagnosticKind::InterfaceCommandUnknownSectionName => COMMAND_UNKNOWN_SECTION_NAME,
        DiagnosticKind::InterfaceArtifactReadFailed => ARTIFACT_READ_FAILED,
        DiagnosticKind::InterfaceCommandOutputFailed => COMMAND_OUTPUT_FAILED,
        _ => unreachable!(),
    };

    MessageTemplate::new(parts)
}
