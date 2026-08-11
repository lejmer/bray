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
const PACKAGE_IDENTITY_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package interface encodes package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text(" but dependency selection requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
];
const PRODUCT_IDENTITY_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package interface encodes product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" but dependency selection requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedProductIdentity),
];
const DUPLICATE_PACKAGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("more than one selected package interface claims package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
];
const MISSING_DEPENDENCY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package interface requires unselected dependency "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
];
const DEPENDENCY_PRODUCT_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected dependency provides product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" but its consumer requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedProductIdentity),
];
const DEPENDENCY_CONTENT_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected dependency content digest is "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualArtifactDigest),
    MessageTemplatePart::Text(" but its consumer requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedArtifactDigest),
];
const SYMBOL_REFERENCE_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package interface references missing symbol record "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceRecordIndex),
];
const DEPENDENCY_REFERENCE_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package interface references missing dependency record "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceRecordIndex),
];
const DEPENDENCY_SYMBOL_MISSING: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
    MessageTemplatePart::Text(" does not export required symbol "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSymbolIdentity),
];
const COMPILER_DECLARATION_EXPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "package interface attempts to export compiler-provided declaration ",
    ),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSymbolIdentity),
];
const SYMBOL_GRAPH_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package-interface symbol graph is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSymbolGraphProblem),
];
const SYMBOL_CAPACITY_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("compilation for package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text(" requires a "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceLimit),
    MessageTemplatePart::Text(" of "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(", but the compiler can represent at most "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];
const SEMANTIC_CONTENT_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package-interface semantic content is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSemanticProblem),
];
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
        DiagnosticKind::InterfaceDuplicatePackage => DUPLICATE_PACKAGE,
        DiagnosticKind::InterfaceMissingDependency => MISSING_DEPENDENCY,
        DiagnosticKind::InterfaceDependencyProductMismatch => DEPENDENCY_PRODUCT_MISMATCH,
        DiagnosticKind::InterfaceDependencyContentMismatch => DEPENDENCY_CONTENT_MISMATCH,
        DiagnosticKind::InterfaceSymbolReferenceInvalid => SYMBOL_REFERENCE_INVALID,
        DiagnosticKind::InterfaceDependencyReferenceInvalid => DEPENDENCY_REFERENCE_INVALID,
        DiagnosticKind::InterfaceDependencySymbolMissing => DEPENDENCY_SYMBOL_MISSING,
        DiagnosticKind::InterfaceCompilerDeclarationExported => COMPILER_DECLARATION_EXPORTED,
        DiagnosticKind::InterfaceSymbolGraphInvalid => SYMBOL_GRAPH_INVALID,
        DiagnosticKind::InterfaceSymbolCapacityExceeded => SYMBOL_CAPACITY_EXCEEDED,
        DiagnosticKind::InterfaceSemanticSymbolUnresolved
        | DiagnosticKind::InterfaceSemanticSymbolKindInvalid
        | DiagnosticKind::InterfaceSemanticValueGraphInvalid
        | DiagnosticKind::InterfaceSemanticValueInvalid
        | DiagnosticKind::InterfaceExecutableTemplateInvalid
        | DiagnosticKind::InterfaceSupportEntityInvalid => SEMANTIC_CONTENT_INVALID,
        DiagnosticKind::InterfaceConstantCallableBodyUnavailable => {
            CONSTANT_CALLABLE_BODY_UNAVAILABLE
        }
        DiagnosticKind::InterfaceExecutableTemplateUnavailable => EXECUTABLE_TEMPLATE_UNAVAILABLE,
        _ => unreachable!(),
    };

    MessageTemplate::new(parts)
}
