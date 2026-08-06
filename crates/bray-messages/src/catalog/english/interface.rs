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
const CONSTANT_CALLABLE_BODY_UNAVAILABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "selected imported const callable has no compatible implementation body",
)];
const EXECUTABLE_TEMPLATE_UNAVAILABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "selected imported generic callable has no compatible executable template",
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
        DiagnosticKind::InterfaceConstantCallableBodyUnavailable => {
            CONSTANT_CALLABLE_BODY_UNAVAILABLE
        }
        DiagnosticKind::InterfaceExecutableTemplateUnavailable => EXECUTABLE_TEMPLATE_UNAVAILABLE,
        _ => unreachable!(),
    };

    MessageTemplate::new(parts)
}
