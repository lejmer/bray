// rust-style: allow(module-too-large, reason = "the English diagnostic catalog is intentionally one exhaustive flat message mapping")

use bray_diagnostics::{DiagnosticArgName, DiagnosticKind, DiagnosticNoteKind, SeverityKind};

use crate::catalog::{MessageTemplate, MessageTemplatePart};
use crate::rendered_diagnostic::RenderedDiagnosticNoteKind;

use super::interface::diagnostic_template as interface_diagnostic_template;

const SOURCE_FILE_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read "),
    MessageTemplatePart::Arg(DiagnosticArgName::SourceInput),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const SOURCE_INVALID_UTF8: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SourceInput),
    MessageTemplatePart::Text(" contains invalid UTF-8 starting at byte offset "),
    MessageTemplatePart::Arg(DiagnosticArgName::TextOffset),
];

const CODEGEN_UNSUPPORTED_TARGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" does not support target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const CODEGEN_UNSUPPORTED_ARTIFACT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" cannot produce required "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" output for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const CODEGEN_INVALID_CONFIGURATION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" has an incompatible configuration for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const CODEGEN_RESOURCE_EXHAUSTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" exhausted an available resource while compiling target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const CODEGEN_BACKEND_LIBRARY_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" failed while compiling target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const CODEGEN_GENERATED_MODULE_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" rejected the generated module for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const CODEGEN_ARTIFACT_CONSTRUCTION_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native-code generator "),
    MessageTemplatePart::Arg(DiagnosticArgName::CodegenBackendIdentity),
    MessageTemplatePart::Text(" could not construct "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" output for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];
const NATIVE_PRODUCT_PREPARATION_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot prepare native product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::NativeProductFailureKind),
];
const EMISSION_LINKED_PLAN_MISSING: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot publish linked product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(
        " because its completed native output was not requested for this product",
    ),
];
const EMISSION_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot emit product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::EmissionFailure),
];
const EMISSION_TARGET_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot emit product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" because it requests target "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualTargetTriple),
    MessageTemplatePart::Text(" but the compilation selected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedTargetTriple),
];
const EMISSION_PRODUCT_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot emit requested product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" because this compilation loaded package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
];
const EMISSION_ARTIFACT_IO_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not "),
    MessageTemplatePart::Arg(DiagnosticArgName::EmissionArtifactOperation),
    MessageTemplatePart::Text(" for "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" of product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const LINKER_UNSUPPORTED_TARGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support target "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_PRODUCT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support product category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support input category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_INPUT_MODE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support input treatment "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_OUTPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support output category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_SEARCH_PATH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support search-path category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_LINK_MODEL: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support linkage model "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_DEAD_STRIP: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support dead-code removal policy "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_SECTION_GARBAGE_COLLECTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support section-removal policy "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_DEBUG: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support debug-information policy "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_SUBSYSTEM: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support subsystem "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_SYMBOL: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support symbol requirement "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_STARTUP: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support startup ownership "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_UNSUPPORTED_RUNTIME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected linker does not support runtime ownership "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
];
const LINKER_DRIVER_UNAVAILABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "no configured native linker can satisfy this target and product",
)];
const LINKER_DRIVER_INCOMPATIBLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "the selected native linker is incompatible with the validated link plan",
)];
const LINKER_INPUT_MISSING: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native link input "),
    MessageTemplatePart::Arg(DiagnosticArgName::InputIndex),
    MessageTemplatePart::Text(" of category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
    MessageTemplatePart::Text(" is unavailable"),
];
const LINKER_RESPONSE_FILE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExternalToolOperation),
    MessageTemplatePart::Text(" "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];
const LINKER_INVOCATION_FAILED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "the native linker process did not complete successfully",
)];
const LINKER_EXTERNAL_TOOL_IO_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExternalToolOperation),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];
const LINKER_EXTERNAL_TOOL_CONTRACT_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExternalToolOperation),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExternalToolFailureKind),
];
const LINKER_EXTERNAL_TOOL_EXITED_UNSUCCESSFULLY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("the native linker completed unsuccessfully: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExternalToolExit),
];
const LINKER_OUTPUT_MISSING: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native linker output "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" of category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
    MessageTemplatePart::Text(" was not produced at "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];
const LINKER_OUTPUT_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native linker output "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" of category "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkRequirement),
    MessageTemplatePart::Text(" at "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" does not satisfy its staging contract"),
];
const LINKER_RESOURCE_EXHAUSTED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "the native linker could not acquire an external-process slot",
)];

const SOURCE_TOO_MANY_INPUTS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot load "),
    MessageTemplatePart::Arg(DiagnosticArgName::SourceInput),
    MessageTemplatePart::Text(": source input count "),
    MessageTemplatePart::Arg(DiagnosticArgName::SourceCount),
    MessageTemplatePart::Text(" exceeds the compact source identity range"),
];

const SOURCE_TEXT_TOO_LARGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SourceInput),
    MessageTemplatePart::Text(" is too large: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ByteCount),
    MessageTemplatePart::Text(" bytes"),
];

const INSPECTION_REPORT_WRITE_FAILED: &[MessageTemplatePart] = &[MessageTemplatePart::Arg(
    DiagnosticArgName::ProjectCommandFailure,
)];

const COMPILER_PROFILE_WRITE_FAILED: &[MessageTemplatePart] = &[MessageTemplatePart::Arg(
    DiagnosticArgName::ProjectCommandFailure,
)];

const RUNTIME_ARTIFACT_METADATA_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read runtime artifact metadata "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const RUNTIME_ARTIFACT_METADATA_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("runtime artifact metadata is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::RuntimeArtifactProblem),
];

const RUNTIME_ARTIFACT_TARGET_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("runtime artifact metadata "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(" targets "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualTargetIdentity),
    MessageTemplatePart::Text(" but the compilation selected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedTargetIdentity),
];

const RUNTIME_ARTIFACT_ABI_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("runtime artifact metadata "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(" provides runtime ABI "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualRuntimeAbi),
    MessageTemplatePart::Text(" but the compilation requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedRuntimeAbi),
];

const RUNTIME_ARTIFACT_ARCHIVE_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read runtime archive "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const RUNTIME_ARTIFACT_ARCHIVE_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("runtime archive is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
];

const RUNTIME_ARTIFACT_ARCHIVE_DIGEST_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("runtime archive "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(" has digest "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualArtifactDigest),
    MessageTemplatePart::Text(" but expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedArtifactDigest),
];

const PROJECT_MANIFEST_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read Bray project manifest "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const PROJECT_MANIFEST_PARSE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not decode Bray project manifest "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" at line "),
    MessageTemplatePart::Arg(DiagnosticArgName::DocumentLine),
    MessageTemplatePart::Text(", column "),
    MessageTemplatePart::Arg(DiagnosticArgName::DocumentColumn),
    MessageTemplatePart::Text(": found "),
    MessageTemplatePart::Arg(DiagnosticArgName::DocumentParseKind),
];

const PROJECT_MANIFEST_UNSUPPORTED_FORMAT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" uses format revision "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualRevision),
    MessageTemplatePart::Text(", but Bray accepts revision "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedRevision),
];

const PROJECT_MANIFEST_INVALID_PATH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" contains non-portable project path "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectPath),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_MANIFEST_INVALID_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" contains invalid project name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

macro_rules! project_manifest_selection_message {
    ($name:ident, $text:literal) => {
        const $name: &[MessageTemplatePart] = &[
            MessageTemplatePart::Text($text),
            MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
            MessageTemplatePart::Text(" selected by "),
            MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
            MessageTemplatePart::Text(" in "),
            MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
        ];
    };
}

const PROJECT_MANIFEST_MISSING_SELECTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" has no selection in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_MANIFEST_MISSING_ROOT_PACKAGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" has no root package in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];
project_manifest_selection_message!(
    PROJECT_MANIFEST_UNDECLARED_FEATURE,
    "undeclared package feature "
);
project_manifest_selection_message!(
    PROJECT_MANIFEST_UNKNOWN_SOURCE_ROOT,
    "unknown package source root "
);
const PROJECT_MANIFEST_UNKNOWN_TARGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" selected by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" is not declared in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];
const PROJECT_MANIFEST_UNKNOWN_TARGET_PREDICATE_PROPERTY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target predicate property "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is not defined for "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_MANIFEST_TARGET_PREDICATE_VALUE_KIND_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target predicate property "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" of "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" accepts "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedTargetPredicateValueKind),
    MessageTemplatePart::Text(" values, but received a "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualTargetPredicateValueKind),
    MessageTemplatePart::Text(" value"),
];
const PROJECT_MANIFEST_UNEXPECTED_TESTED_LIBRARY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("non-test product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" has a tested library in "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" of "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_PACKAGE_VERSION_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" contains invalid Bray package version "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_PACKAGE_VERSION_MISSING_WORKSPACE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" inherits a package version absent from "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_MANIFEST_DUPLICATE_SELECTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" repeats selection "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_SOURCE_ROOT_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read project-owned source root "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectPath),
    MessageTemplatePart::Text(" from "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_SOURCE_ROOT_CONTAINS_SYMLINK: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("project-owned source tree contains symbolic link "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectPath),
    MessageTemplatePart::Text(" under "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_SOURCE_ROOT_CONTAINS_NON_UTF8_PATH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("project-owned source tree contains non-UTF-8 path "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectPath),
    MessageTemplatePart::Text(" under "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_PACKAGE_UNKNOWN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text(" selected by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" is absent from the workspace inventory in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_PRODUCT_UNKNOWN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text("/"),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" selected by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" is absent from its package in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_PRODUCT_NOT_LIBRARY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text("/"),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" selected by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" is not a library in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_PRODUCT_TARGET_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text("/"),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" does not support target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" selected by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_CYCLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency cycle includes "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectDependencyCycleMember),
    MessageTemplatePart::Text(" from "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_COMMAND_SELECTION_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid project command selection: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectSelectionProblem),
];

const PROJECT_COMMAND_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("project command failed: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectCommandFailure),
];

const PROJECT_INITIALIZATION_IDENTITY_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot use "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" as a Bray package identity"),
];

const PROJECT_INITIALIZATION_PATH_CONFLICT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "cannot initialize a Bray project because this path already exists: ",
    ),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_INITIALIZATION_WRITE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not initialize a Bray project at "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const PROJECT_INITIALIZATION_TARGET_UNSUPPORTED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "cannot initialize a Bray project for the current host target",
    )];

const FORMATTER_SOURCE_NOT_FORMATTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("source needs formatting: "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const FORMATTER_SOURCE_INVALID_UTF8: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("formatter source contains invalid UTF-8: "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const FORMATTER_SOURCE_TOO_LARGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("formatter source is too large: "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" ("),
    MessageTemplatePart::Arg(DiagnosticArgName::ByteCount),
    MessageTemplatePart::Text(" bytes)"),
];

const FORMATTER_SOURCE_WRITE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not write formatted source "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const FORMATTER_CONFIGURATION_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read formatter configuration "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const FORMATTER_CONFIGURATION_MALFORMED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not decode formatter configuration "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" at line "),
    MessageTemplatePart::Arg(DiagnosticArgName::DocumentLine),
    MessageTemplatePart::Text(", column "),
    MessageTemplatePart::Arg(DiagnosticArgName::DocumentColumn),
    MessageTemplatePart::Text(": found "),
    MessageTemplatePart::Arg(DiagnosticArgName::DocumentParseKind),
];

const FORMATTER_CONFIGURATION_UNKNOWN_RULE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unknown formatter rule "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const FORMATTER_CONFIGURATION_INVALID_MAXIMUM_WIDTH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("formatter maximum line width must be between 1 and 65535 in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(". Found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
];

const REQUEST_MISSING_SOURCE_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("at least one source input is required, but the request contains "),
    MessageTemplatePart::Arg(DiagnosticArgName::SourceCount),
];

const REQUEST_INVALID_SOURCE_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SourceInput),
    MessageTemplatePart::Text(" does not provide a stable source identity"),
];

const REQUEST_RESERVED_PACKAGE_IDENTITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package identity "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is reserved for toolchain-owned standard library source"),
];

const PROJECT_PACKAGE_IDENTITY_RESERVED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package identity "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" of "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" is reserved for toolchain-owned standard library source"),
];

const PROJECT_STANDARD_LIBRARY_PACKAGE_IDENTITY_REQUIRED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library workspace package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" of "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" is outside the reserved standard library namespace"),
];

const PROJECT_STANDARD_LIBRARY_ROOT_PACKAGE_REQUIRED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library workspace root package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualPackageIdentity),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::ProjectManifestField),
    MessageTemplatePart::Text(" of "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" is not the required public package std"),
];

const REQUEST_STANDARD_LIBRARY_PACKAGE_IDENTITY_REQUIRED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library source authority cannot compile package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const STANDARD_LIBRARY_ARTIFACT_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read standard library artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const STANDARD_LIBRARY_MANIFEST_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library manifest "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" contains "),
    MessageTemplatePart::Arg(DiagnosticArgName::StandardLibraryManifestProblem),
];
const STANDARD_LIBRARY_INFRASTRUCTURE_FAILURE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
    MessageTemplatePart::Text(
        " could not be resolved because Bray could not publish the selected artifact",
    ),
];

const STANDARD_LIBRARY_ARTIFACT_LENGTH_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" has "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualByteCount),
    MessageTemplatePart::Text(" bytes but expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedByteCount),
];

const STANDARD_LIBRARY_ARTIFACT_DIGEST_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(" has digest "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualArtifactDigest),
    MessageTemplatePart::Text(" does not match expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedArtifactDigest),
];

const STANDARD_LIBRARY_TARGET_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("standard library does not support target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];

const STANDARD_LIBRARY_RUNTIME_ABI_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" requires runtime ABI "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedRuntimeAbi),
    MessageTemplatePart::Text(" but the standard library provides "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualRuntimeAbi),
];

const REQUEST_DUPLICATE_SOURCE_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SourceInput),
    MessageTemplatePart::Text(" selects a source that is already present"),
];

const REQUEST_INVALID_WORKER_BUDGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("worker budget must be greater than zero, but the request contains "),
    MessageTemplatePart::Arg(DiagnosticArgName::WorkerCount),
];

const REQUEST_UNSUPPORTED_PRODUCT_EMISSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot emit a native product for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" because "),
    MessageTemplatePart::Arg(DiagnosticArgName::UnsupportedEmissionReason),
];

const SYNTAX_NESTING_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("syntax nesting exceeds the maximum depth of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_NO_APPLICABLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("no applicable "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate"),
];

const CHECKING_MUTABLE_INDEX_CONTRACT_REQUIRED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("mutable indexing requires an implementation of "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_UNKNOWN_UNION_VARIANT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected union type has no variant named "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_AMBIGUOUS_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" selection is ambiguous between "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionCandidates),
];

const CHECKING_INCOMPATIBLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidates reject the supplied expressions: "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionRejections),
];

const CHECKING_TARGET_REPRESENTATION_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetRepresentation),
    MessageTemplatePart::Text(" representation"),
];

const CHECKING_TARGET_CALLABLE_ABI_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" callable ABI"),
];

const CHECKING_TARGET_ALIGNMENT_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("required "),
    MessageTemplatePart::Arg(DiagnosticArgName::AlignmentKind),
    MessageTemplatePart::Text(" alignment "),
    MessageTemplatePart::Arg(DiagnosticArgName::RequiredAlignment),
    MessageTemplatePart::Text(" exceeds target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" maximum of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumAlignment),
];

const CHECKING_TARGET_ABI_REPRESENTATION_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" callable ABI does not accept "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetRepresentation),
    MessageTemplatePart::Text(" values by value"),
];

const CHECKING_TARGET_MEMORY_OPERATION_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
];

const CHECKING_INVALID_TARGET_CONTROL_CONTRACT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("the "),
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
    MessageTemplatePart::Text(" contract is invalid for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
];

const CHECKING_INVALID_ATOMIC_MEMORY_ORDER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("compile-time memory ordering is invalid for "),
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
];

const CHECKING_INVALID_CALLBACK_STATE_CONTEXT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(
        DiagnosticArgName::CallbackStateProblem,
    )];

const CHECKING_MISSING_TRUSTED_MEMORY_GUARANTEES: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
    MessageTemplatePart::Text(" requires trusted memory guarantees"),
];

const CHECKING_MEMORY_OPERATION_AFTER_DEALLOCATION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
    MessageTemplatePart::Text(" uses an invalidated allocation"),
];

const CHECKING_UNINITIALIZED_RAW_STORAGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
    MessageTemplatePart::Text(" reads raw storage without an initialized value"),
];

const CHECKING_DEALLOCATION_WITH_OUTSTANDING_OBLIGATIONS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::MemoryOperation),
    MessageTemplatePart::Text(" cannot release raw storage while initialized values remain"),
];

const BINDING_INVALID_CALLABLE_ABI: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable ABI directive "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
    MessageTemplatePart::Text(" does not match a supported ABI form"),
];

const BINDING_DUPLICATE_CALLABLE_ABI: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable ABI directive "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
    MessageTemplatePart::Text(" repeats an earlier ABI selection"),
];

const BINDING_CYCLIC_MODULE_EXPORT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module export path through "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" forms a cycle"),
];

const BINDING_CONFLICTING_MODULE_EXPORT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module export name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" conflicts with another declaration"),
];

const BINDING_INVALID_MODULE_EXPORT_TARGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" cannot be re-exported"),
];

const BINDING_MALFORMED_DIRECTIVE_ARGUMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("directive argument is malformed")];

const CHECKING_DUPLICATE_MODULE_CONTRIBUTION_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualSyntaxKind),
];

const CHECKING_INVALID_STORED_TYPE: &[MessageTemplatePart] = &[MessageTemplatePart::Arg(
    DiagnosticArgName::StoredTypeProblem,
)];

const CHECKING_RECURSIVE_TYPE_REPRESENTATION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("declared type has an inline recursive representation through "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" source locations"),
];

const CHECKING_TYPE_REPRESENTATION_RECURSION_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("type representation analysis required "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" nested declarations but the limit is "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_INVALID_LAYOUT_DIRECTIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(DiagnosticArgName::LayoutProblem)];

const CHECKING_INVALID_COPY_CONTRACT: &[MessageTemplatePart] = &[MessageTemplatePart::Arg(
    DiagnosticArgName::CopyContractProblem,
)];

const CHECKING_INVALID_UNION_TAG: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(DiagnosticArgName::UnionTagProblem)];

const CHECKING_REFINEMENT_CAPACITY_EXCEEDED: &[MessageTemplatePart] = &[MessageTemplatePart::Arg(
    DiagnosticArgName::RefinementCapacity,
)];

const CHECKING_USE_OF_MOVED_STORAGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::StorageAccess),
    MessageTemplatePart::Text(" reaches storage whose value was moved"),
];

const CHECKING_CONFLICTING_BORROW: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::StorageAccess),
    MessageTemplatePart::Text(" conflicts with an active borrow"),
];

const CHECKING_MISSING_MUTATION_AUTHORITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::StorageAccess),
    MessageTemplatePart::Text(" has no mutable access to the reached storage"),
];

const CHECKING_MISSING_STORAGE_OWNERSHIP: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::StorageAccess),
    MessageTemplatePart::Text(" does not own the reached storage"),
];

const CHECKING_MISSING_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait implementation does not fulfill "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_EXTRA_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait has no member named "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_INCOMPATIBLE_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait fulfillment of "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
    MessageTemplatePart::Text(" is incompatible: "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitFulfillmentMismatch),
];

const CHECKING_DUPLICATE_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait member is fulfilled more than once: "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_OVERLAPPING_IMPLEMENTATION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("implementation of "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSymbolIdentity),
    MessageTemplatePart::Text(" for "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
    MessageTemplatePart::Text(" overlaps another participating implementation"),
];

const CHECKING_UNGROUPED_IMPLEMENTATION_OVERLOADS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("implementation of "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSymbolIdentity),
    MessageTemplatePart::Text(" for "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
    MessageTemplatePart::Text(" conflicts with an implementation outside its overload family"),
];

const CHECKING_INVALID_IMPLEMENTATION_OVERLOAD_HEADER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("implementation overload header is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ImplementationOverloadProblem),
];

const CHECKING_INVALID_IMPLEMENTATION_OVERLOAD_ARM: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("implementation overload arm is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ImplementationOverloadProblem),
];

const CHECKING_DUPLICATE_IMPLEMENTATION_OVERLOAD_ARM: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("implementation overload arm is duplicated: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ImplementationOverloadProblem),
];

const CHECKING_INVALID_CALLABLE_OVERLOAD_ARM: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable overload arm is invalid: "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableOverloadProblem),
];

const CHECKING_DUPLICATE_CALLABLE_OVERLOAD_ARM: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable overload arm is duplicated: "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableOverloadProblem),
];

const CHECKING_CONFLICTING_CALLABLE_OVERLOAD_FAMILY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable overload family membership conflicts: "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableOverloadProblem),
];

const CHECKING_CONFLICTING_CALLABLE_OVERLOAD_SIGNATURE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable overload signatures conflict: "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableOverloadProblem),
];

const CHECKING_IMPLEMENTATION_COHERENCE_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("implementation coherence comparison "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" exceeds the configured limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CALLABLE_OVERLOAD_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("callable overload comparison "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" exceeds the configured limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_MISSING_FOREIGN_CALLABLE_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("foreign callable requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const CHECKING_FOREIGN_CALLABLE_REQUIRES_TRUSTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" foreign callable is not declared trusted"),
];

const CHECKING_FOREIGN_CALLABLE_REQUIRES_CAPABILITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("foreign callable requires capability "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_FOREIGN_CALLABLE_EXECUTION_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("asynchronous callable cannot cross the "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" ABI boundary"),
];

const CHECKING_FOREIGN_ABI_TYPE_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
    MessageTemplatePart::Text(" cannot cross the "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" ABI boundary"),
];

const CHECKING_PLATFORM_SERVICE_SIGNATURE_MISMATCH: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(
        DiagnosticArgName::PlatformServiceSignatureProblem,
    )];

const CHECKING_INVALID_NATIVE_LINK_DIRECTIVE: &[MessageTemplatePart] = &[MessageTemplatePart::Arg(
    DiagnosticArgName::NativeLinkDirectiveProblem,
)];

const CHECKING_INVALID_NATIVE_SYMBOL_DIRECTIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(
        DiagnosticArgName::NativeSymbolDirectiveProblem,
    )];

const CHECKING_DUPLICATE_NATIVE_SYMBOL: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native symbol is declared more than once: "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
];

const CHECKING_UNAVAILABLE_NATIVE_LINK_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("native link input is unavailable for the selected target: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_UNDECLARED_TRUSTED_CAPABILITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trusted capability is used but not declared: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_UNUSED_TRUSTED_CAPABILITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trusted capability is declared but not used: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_TRUSTED_CAPABILITY_REQUIRES_TRUSTED_CALLABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trusted capability requires a trusted callable: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_AWAIT_OUTSIDE_ASYNC_CALLABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "await is used in a synchronous callable body",
)];

const CHECKING_TASK_START_OUTSIDE_ASYNC_CALLABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "a future is started in a synchronous callable body",
    )];

const CHECKING_UNAVAILABLE_AWAIT_DEPENDENCY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("await requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::DependencySubjectKind),
    MessageTemplatePart::Text(" to "),
    MessageTemplatePart::Arg(DiagnosticArgName::DependencyRequirementKind),
];

const CHECKING_ENTRYPOINT_NOT_ALLOWED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("the entrypoint directive is not allowed on a "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductKind),
];

const CHECKING_MISSING_ENTRYPOINT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "executable product requires an entry point",
)];

const CHECKING_DUPLICATE_ENTRYPOINT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("executable product has "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" entry points"),
];

const CHECKING_ENTRY_CANNOT_BE_GENERIC: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("product entry function declares "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" generic parameters"),
];

const CHECKING_ENTRY_CANNOT_TAKE_PARAMETERS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("product entry function requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" caller-supplied parameters"),
];

const CHECKING_INVALID_ENTRYPOINT_RESULT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("entry point has unsupported result type "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_INVALID_TEST_RESULT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("test entry has unsupported result type "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_INVALID_TEST_ENTRY_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("@test on a function uses unsupported form "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
    MessageTemplatePart::Text(" with argument count "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
];

const CHECKING_INVALID_TEST_MODULE_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("@test on a module uses unsupported form "),
    MessageTemplatePart::Arg(DiagnosticArgName::TokenText),
    MessageTemplatePart::Text(" with argument count "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
];

const CHECKING_DUPLICATE_TEST_IDENTITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("test declaration path is used more than once: "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
];

const CHECKING_ENTRY_CANNOT_BE_CONSTANT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "product entry function cannot be constant",
)];

const CHECKING_ENTRY_CANNOT_REQUIRE_TRUST: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "product entry function requires a trusted caller",
)];

const CHECKING_EXPORT_DEPENDS_ON_INTERNAL_DECLARATION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("public declaration exposes internal declaration "),
    MessageTemplatePart::Arg(DiagnosticArgName::InterfaceSymbolIdentity),
];

const EMISSION_MISSING_CONTRIBUTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("missing required "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" content"),
];

const EMISSION_INVALID_CONTRIBUTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" content does not match the output request"),
];

const EMISSION_ARTIFACT_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" content: "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_OPEN_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not open "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_WRITE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not write "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_FLUSH_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not flush "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_COMMIT_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not commit "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_MANAGED_PUBLICATION_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" targets filesystem output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(" does not support atomic managed product publication"),
];

const EMISSION_GENERATION_COLLISION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" conflicts with different content at "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
];

const EMISSION_GENERATION_MANIFEST_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("managed generation manifest for "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" is invalid at "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
];

const EMISSION_ARTIFACT_DIGEST_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" declared digest "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedArtifactDigest),
    MessageTemplatePart::Text(", but content digest was "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualArtifactDigest),
];

const EMISSION_ARTIFACT_LENGTH_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactOrdinal),
    MessageTemplatePart::Text(" declared "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedByteCount),
    MessageTemplatePart::Text(" bytes, but content contained "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualByteCount),
    MessageTemplatePart::Text(" bytes"),
];

const LEXICAL_INVALID_CHARACTER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const LEXICAL_MISPLACED_BOM: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "byte order mark is not at the start of the source",
)];

const LEXICAL_LONE_CARRIAGE_RETURN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("lone carriage return")];

const LEXICAL_NON_ASCII_IDENTIFIER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("non-ASCII identifier character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const LEXICAL_INVALID_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid identifier")];

const LEXICAL_INVALID_OPERATOR_OR_PUNCTUATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid operator or punctuation")];

const LEXICAL_MALFORMED_NUMERIC_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("malformed numeric literal")];

const LEXICAL_INVALID_NUMERIC_SUFFIX: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid numeric literal suffix")];

const LEXICAL_MALFORMED_CHARACTER_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("malformed character literal")];

const LEXICAL_UNTERMINATED_CHARACTER_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated character literal")];

const LEXICAL_UNTERMINATED_STRING_LITERAL: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated string literal")];

const LEXICAL_UNKNOWN_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unknown escape sequence")];

const LEXICAL_INVALID_UNICODE_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid Unicode escape")];

const LEXICAL_UNTERMINATED_BLOCK_COMMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("unterminated block comment")];

const SYNTAX_EXPECTED_TOKEN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" but found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualSyntaxKind),
];

const SYNTAX_EXPECTED_EXPRESSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const SYNTAX_UNEXPECTED_EOF: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("unexpected end of file while expecting "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const DECLARATION_DUPLICATE_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate declaration of "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
];

const DECLARATION_CONFLICTING_MODULE_VISIBILITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
    MessageTemplatePart::Text(" has conflicting visibility: expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedVisibility),
    MessageTemplatePart::Text(", found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualVisibility),
];

const DECLARATION_CONFLICTING_MODULE_TRUST: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("module "),
    MessageTemplatePart::Arg(DiagnosticArgName::DeclarationName),
    MessageTemplatePart::Text(" has conflicting trust state: expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedModuleTrust),
    MessageTemplatePart::Text(", found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualModuleTrust),
];

const DECLARATION_DUPLICATE_MODIFIER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate "),
    MessageTemplatePart::Arg(DiagnosticArgName::ModifierKind),
    MessageTemplatePart::Text(" modifier"),
];

const DECLARATION_INCOMPATIBLE_MODIFIERS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ModifierKind),
    MessageTemplatePart::Text(" and "),
    MessageTemplatePart::Arg(DiagnosticArgName::ConflictingModifierKind),
    MessageTemplatePart::Text(" modifiers cannot be combined"),
];

const DECLARATION_INVALID_MODIFIER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ModifierKind),
    MessageTemplatePart::Text(" modifier is not valid on this declaration"),
];

const DECLARATION_BODY_REQUIRED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ActualSyntaxKind),
    MessageTemplatePart::Text(" requires a body"),
];

const DECLARATION_BODY_NOT_ALLOWED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ActualSyntaxKind),
    MessageTemplatePart::Text(" cannot have a body"),
];

const DECLARATION_DUPLICATE_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate "),
    MessageTemplatePart::Arg(DiagnosticArgName::DirectiveKind),
    MessageTemplatePart::Text(" directive"),
];

const DECLARATION_INCOMPATIBLE_DIRECTIVES: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::DirectiveKind),
    MessageTemplatePart::Text(" and "),
    MessageTemplatePart::Arg(DiagnosticArgName::ConflictingDirectiveKind),
    MessageTemplatePart::Text(" directives cannot be combined"),
];

const DECLARATION_INVALID_DIRECTIVE_TARGET: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::DirectiveKind),
    MessageTemplatePart::Text(" directive is not valid on this declaration"),
];

const DECLARATION_INVALID_PARAMETER_ORDER: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "positional parameters must precede named-only parameters",
)];

const DECLARATION_INVALID_MEMBER_PLACEMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ActualSyntaxKind),
    MessageTemplatePart::Text(" is not valid in this container"),
];

const DECLARATION_DUPLICATE_LIFECYCLE_SLOT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "lifecycle declaration conflicts with an earlier declaration",
)];

const BINDING_UNRESOLVED_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not resolve "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const BINDING_NAME_ALREADY_DEFINED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name is already defined: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const BINDING_INCOHERENT_ALTERNATIVE_PATTERN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "alternative patterns must bind the same names",
    )];

const CHECKING_INCOMPATIBLE_EXPRESSION_TYPE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedType),
    MessageTemplatePart::Text(", but found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_INCOMPATIBLE_PATTERN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("pattern is incompatible with "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_REFUTABLE_PATTERN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("pattern can reject values of type "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_NON_EXHAUSTIVE_MATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("match coverage is incomplete: "),
    MessageTemplatePart::Arg(DiagnosticArgName::PatternCoverage),
];

const CHECKING_UNREACHABLE_MATCH_ARM: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("match arm is unreachable because "),
    MessageTemplatePart::Arg(DiagnosticArgName::PatternUnreachability),
];

const CHECKING_UNREACHABLE_PATTERN_ALTERNATIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("pattern alternative is unreachable because "),
    MessageTemplatePart::Arg(DiagnosticArgName::PatternUnreachability),
];

const CHECKING_CANNOT_INFER_EXPRESSION_TYPE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("cannot infer the type of this "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpressionCategory),
];

const CHECKING_RANGE_BOUND_TYPE_MUST_BE_INTEGER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("range bounds must use one integer type, but found "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_NO_COMPATIBLE_PROPAGATION_BOUNDARY: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(
        DiagnosticArgName::PropagationProblem,
    )];

const CHECKING_INVALID_CONSTANT_EXPRESSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("this "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpressionCategory),
    MessageTemplatePart::Text(" cannot be evaluated as a compile-time constant"),
];

const CHECKING_INVALID_CONSTANT_OPERATION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("compile-time "),
    MessageTemplatePart::Arg(DiagnosticArgName::ConstantOperation),
    MessageTemplatePart::Text(" is not defined for these operands"),
];

const CHECKING_ARRAY_GENERATOR_CARDINALITY_NOT_PROVABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Arg(
        DiagnosticArgName::ArrayGeneratorCardinalityProblem,
    )];

const CHECKING_CONSTANT_LITERAL_NOT_REPRESENTABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("literal value cannot be represented by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const CHECKING_CONSTANT_EVALUATION_STEP_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("constant evaluation requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" operations, exceeding the limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CONSTANT_AGGREGATE_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("constant evaluation materializes "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" aggregate elements, exceeding the limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CONSTANT_EXPANSION_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("constant evaluation expands "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" elements, exceeding the limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CONSTANT_LITERAL_SIZE_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("constant evaluation reads "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" literal bytes, exceeding the limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CONSTANT_INTEGER_SIZE_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("compile-time "),
    MessageTemplatePart::Arg(DiagnosticArgName::ConstantOperation),
    MessageTemplatePart::Text(" requires an integer with "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualCount),
    MessageTemplatePart::Text(" bits, exceeding the limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CYCLIC_CONSTANT_DEFINITION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "constant definition depends on itself through a cycle",
)];

const CHECKING_CONSTANT_DIVISION_BY_ZERO: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("compile-time "),
    MessageTemplatePart::Arg(DiagnosticArgName::ConstantOperation),
    MessageTemplatePart::Text(" divides by zero"),
];

const CHECKING_CONSTANT_VALUE_NOT_REPRESENTABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("result of compile-time "),
    MessageTemplatePart::Arg(DiagnosticArgName::ConstantOperation),
    MessageTemplatePart::Text(" cannot be represented by "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
];

const BINDING_AMBIGUOUS_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is ambiguous"),
];

const BINDING_INACCESSIBLE_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is not visible here"),
];

const BINDING_WRONG_NAME_KIND: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" does not refer to a "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedNameKind),
];

const BINDING_MALFORMED_NAME: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("name "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" refers to a malformed declaration"),
];

const NOTE_SOURCE_FILE_MUST_BE_READABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source files must be readable before compilation",
)];

const NOTE_SOURCE_MUST_BE_UTF8: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source inputs must be valid UTF-8",
)];

const NOTE_SOURCE_MUST_MATCH_FORMATTER_OUTPUT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "format checks require the source to match `bray fmt` output",
    )];

const NOTE_SOURCE_IDS_ARE_COMPACT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source IDs use compact 32-bit storage",
)];

const NOTE_SOURCE_TEXT_OFFSETS_ARE_COMPACT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source byte offsets use compact 32-bit storage",
)];

const NOTE_SOURCE_INPUT_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "provide at least one source input",
)];

const NOTE_SOURCE_INPUT_NEEDS_STABLE_IDENTITY: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "source input order must fit in a stable source identity",
    )];

const NOTE_WORKER_BUDGET_MUST_BE_POSITIVE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use a worker count greater than zero",
)];

const NOTE_PACKAGE_IDENTITY_MUST_BE_VALID: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use a non-reserved lowercase name or dot-separated names, starting each name with a letter and using only letters, digits, underscores, or hyphens",
)];

const NOTE_PACKAGE_VERSION_MUST_BE_VALID: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "declare a Semantic Versioning value such as 1.2.3, or inherit a version declared by the workspace",
)];

const NOTE_CHARACTER_NOT_ACCEPTED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "remove this character or replace it with valid Bray syntax",
)];

const NOTE_BOM_ONLY_ALLOWED_AT_START: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "a UTF-8 byte order mark is accepted only at the beginning of a source unit",
)];

const NOTE_LINE_BREAKS_MUST_BE_LF_OR_CRLF: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "line breaks must be LF or CRLF outside block comments",
)];

const NOTE_IDENTIFIERS_MUST_BE_ASCII: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "identifiers use ASCII characters",
)];

const NOTE_IDENTIFIER_SPELLING_MUST_BE_VALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "identifiers must start with an ASCII letter and continue with ASCII letters, digits, or underscores",
    ),
];

const NOTE_ONLY_IMAGINARY_NUMERIC_SUFFIX: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "numeric suffixes are not supported except imaginary i",
)];

const NOTE_CHARACTER_LITERAL_MUST_CONTAIN_ONE_SCALAR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "character literals must contain exactly one Unicode scalar value",
    )];

const NOTE_CHARACTER_LITERAL_NEEDS_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "character literals must be closed",
    )];

const NOTE_STRING_LITERAL_NEEDS_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("string literals must be closed")];

const NOTE_ESCAPE_MUST_BE_KNOWN: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use one of the supported escape sequences",
)];

const NOTE_UNICODE_ESCAPE_MUST_BE_SCALAR: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "Unicode escapes must have one to six hexadecimal digits and denote a Unicode scalar value",
)];

const NOTE_BLOCK_COMMENT_NEEDS_TERMINATOR: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("block comments must be closed")];

const NOTE_CALLABLE_ABI_DIRECTIVE_MUST_NAME_SUPPORTED_ABI: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "callable ABI directives accept exactly one positional ABI name: c or system",
    )];

const NOTE_DIRECTIVE_ARGUMENT_MUST_HAVE_COMPLETE_FORM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "directive arguments must be a complete positional expression or name = expression pair",
    )];

const NOTE_INTERFACE_DEPENDENCY_CONTEXT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("while loading package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
    MessageTemplatePart::Text(" product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedProductIdentity),
    MessageTemplatePart::Text(" from "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
];
const NOTE_LINK_PLAN_CONTEXT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("while linking product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualProductIdentity),
    MessageTemplatePart::Text(" for target "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetTriple),
    MessageTemplatePart::Text(" with driver "),
    MessageTemplatePart::Arg(DiagnosticArgName::LinkerDriverIdentity),
];
const NOTE_RUNTIME_ARTIFACT_MUST_BE_USABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "select a readable runtime artifact built for the selected target and runtime ABI",
)];
const NOTE_AWAIT_DEPENDENCY_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::DependencySubjectKind),
    MessageTemplatePart::Text(" must "),
    MessageTemplatePart::Arg(DiagnosticArgName::DependencyRequirementKind),
];

const NOTE_ASYNCHRONOUS_CALLABLE_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "make the enclosing callable asynchronous or perform this operation from an asynchronous callable",
)];

const NOTE_EXECUTABLE_ENTRYPOINT_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "declare one valid module-level main function or mark one function with @entrypoint",
)];

const NOTE_ENTRYPOINT_DIRECTIVE_REQUIRES_EXECUTABLE_PRODUCT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "remove @entrypoint or declare this package product as an executable",
    )];

const NOTE_PRODUCT_ENTRY_REQUIREMENTS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use a non-generic, non-constant function without parameters or trusted caller requirements and with a supported result type",
)];

const NOTE_TEST_DIRECTIVE_REQUIREMENTS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use @test without arguments, or use @test(serial) on a function",
)];

const NOTE_UNIQUE_TEST_IDENTITY_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "rename one test so every test has a unique module-qualified declaration path",
)];

const NOTE_PUBLIC_DEPENDENCY_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "make the exposed declaration public or remove it from the public declaration's signature or contract",
)];

const NOTE_REPORT_COMPILER_DEFECT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "report this compiler defect with the command and complete diagnostic output",
)];

const NOTE_EXTERNAL_TOOL_EXIT_REQUIRES_CORRECTION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "correct the errors reported by the native linker, then build again",
    )];

const NOTE_TYPE_INFERENCE_NEEDS_CONSTRAINT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "provide a type annotation or use the expression where an expected type is known",
)];

const NOTE_CONSTANT_EXPRESSION_MUST_BE_EVALUABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "use only operations and values that can be evaluated at compile time",
    )];

const NOTE_CONSTANT_EVALUATION_MUST_FIT_LIMITS: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "reduce the compile-time work or select larger constant-evaluation limits",
    )];

const NOTE_TYPE_LAYOUT_DIRECTIVE_FORMS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "select stable, C-compatible, or transparent layout and use only compatible align, pack, and tag options",
)];

const NOTE_UNION_TAG_DIRECTIVE_FORMS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use one representable integer tag on every variant of an explicitly laid-out union, or omit all explicit variant tags",
)];

const NOTE_COPY_CONTRACT_REQUIREMENTS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "a copy contract takes no arguments and requires every stored member to be copyable without lifecycle behavior",
)];

const NOTE_STORED_TYPE_REQUIRES_INDIRECTION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "store this value behind an owned or borrowed indirection",
)];
const NOTE_SELECTION_MUST_BE_DISAMBIGUATED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "qualify the intended declaration or make the supplied types select one candidate",
)];

const NOTE_PROPAGATION_BOUNDARY_MUST_MATCH: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "return or yield a compatible nullable or result type at an enclosing boundary",
)];

const NOTE_ARRAY_GENERATOR_MUST_YIELD_ONCE_PER_ELEMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "use a source with an exact count and yield exactly once on every continuing iteration path",
    ),
];

const NOTE_CALLBACK_STATE_REQUIREMENTS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "use the first context parameter of a trusted C or system ABI callable with a native symbol directive",
)];

const NOTE_REFUTABLE_PATTERN_REQUIRES_CONDITIONAL_CONTEXT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "use this pattern in a match or another conditional pattern context",
    )];

pub(crate) const fn severity_label(severity: SeverityKind) -> &'static str {
    match severity {
        SeverityKind::Error => "error",
        SeverityKind::Warning => "warning",
        SeverityKind::Note => "note",
        SeverityKind::Help => "help",
    }
}

pub(crate) const fn note_kind(kind: DiagnosticNoteKind) -> RenderedDiagnosticNoteKind {
    match kind {
        DiagnosticNoteKind::SourceIdsAreCompact
        | DiagnosticNoteKind::SourceTextOffsetsAreCompact
        | DiagnosticNoteKind::BomOnlyAllowedAtStart
        | DiagnosticNoteKind::IdentifiersMustBeAscii
        | DiagnosticNoteKind::OnlyImaginaryNumericSuffix
        | DiagnosticNoteKind::CharacterLiteralMustContainOneScalar
        | DiagnosticNoteKind::UnicodeEscapeMustBeScalar
        | DiagnosticNoteKind::InterfaceDependencyContext
        | DiagnosticNoteKind::LinkPlanContext
        | DiagnosticNoteKind::AwaitDependencyUnavailable
        | DiagnosticNoteKind::ReportCompilerDefect => RenderedDiagnosticNoteKind::Note,
        DiagnosticNoteKind::SourceFileMustBeReadable
        | DiagnosticNoteKind::SourceMustBeUtf8
        | DiagnosticNoteKind::SourceMustMatchFormatterOutput
        | DiagnosticNoteKind::SourceInputRequired
        | DiagnosticNoteKind::SourceInputNeedsStableIdentity
        | DiagnosticNoteKind::WorkerBudgetMustBePositive
        | DiagnosticNoteKind::PackageIdentityMustBeValid
        | DiagnosticNoteKind::PackageVersionMustBeValid
        | DiagnosticNoteKind::CharacterNotAccepted
        | DiagnosticNoteKind::LineBreaksMustBeLfOrCrlf
        | DiagnosticNoteKind::IdentifierSpellingMustBeValid
        | DiagnosticNoteKind::CharacterLiteralNeedsTerminator
        | DiagnosticNoteKind::StringLiteralNeedsTerminator
        | DiagnosticNoteKind::EscapeMustBeKnown
        | DiagnosticNoteKind::BlockCommentNeedsTerminator
        | DiagnosticNoteKind::CallableAbiDirectiveMustNameSupportedAbi
        | DiagnosticNoteKind::DirectiveArgumentMustHaveCompleteForm
        | DiagnosticNoteKind::TypeInferenceNeedsConstraint
        | DiagnosticNoteKind::ConstantExpressionMustBeEvaluable
        | DiagnosticNoteKind::ConstantEvaluationMustFitLimits
        | DiagnosticNoteKind::TypeLayoutDirectiveForms
        | DiagnosticNoteKind::UnionTagDirectiveForms
        | DiagnosticNoteKind::CopyContractRequirements
        | DiagnosticNoteKind::StoredTypeRequiresIndirection
        | DiagnosticNoteKind::SelectionMustBeDisambiguated
        | DiagnosticNoteKind::PropagationBoundaryMustMatch
        | DiagnosticNoteKind::ArrayGeneratorMustYieldOncePerElement
        | DiagnosticNoteKind::CallbackStateRequirements
        | DiagnosticNoteKind::RefutablePatternRequiresConditionalContext
        | DiagnosticNoteKind::AsynchronousCallableRequired
        | DiagnosticNoteKind::ExecutableEntrypointRequired
        | DiagnosticNoteKind::EntrypointDirectiveRequiresExecutableProduct
        | DiagnosticNoteKind::ProductEntryRequirements
        | DiagnosticNoteKind::TestDirectiveRequirements
        | DiagnosticNoteKind::UniqueTestIdentityRequired
        | DiagnosticNoteKind::PublicDependencyRequired
        | DiagnosticNoteKind::ExternalToolExitRequiresCorrection
        | DiagnosticNoteKind::RuntimeArtifactMustBeUsable => RenderedDiagnosticNoteKind::Help,
    }
}

pub(crate) const fn note_heading(kind: RenderedDiagnosticNoteKind) -> &'static str {
    match kind {
        RenderedDiagnosticNoteKind::Note => "note",
        RenderedDiagnosticNoteKind::Help => "help",
    }
}

// rust-style: allow(function-too-large, reason = "the locale diagnostic catalog is intentionally one exhaustive flat mapping")
pub(crate) const fn diagnostic_template(kind: DiagnosticKind) -> MessageTemplate {
    match kind {
        DiagnosticKind::SourceFileReadFailed => MessageTemplate::new(SOURCE_FILE_READ_FAILED),
        DiagnosticKind::SourceInvalidUtf8 => MessageTemplate::new(SOURCE_INVALID_UTF8),
        DiagnosticKind::SourceTooManyInputs => MessageTemplate::new(SOURCE_TOO_MANY_INPUTS),
        DiagnosticKind::SourceTextTooLarge => MessageTemplate::new(SOURCE_TEXT_TOO_LARGE),
        DiagnosticKind::RequestMissingSourceInput => {
            MessageTemplate::new(REQUEST_MISSING_SOURCE_INPUT)
        }
        DiagnosticKind::RequestInvalidSourceInput => {
            MessageTemplate::new(REQUEST_INVALID_SOURCE_INPUT)
        }
        DiagnosticKind::RequestReservedPackageIdentity => {
            MessageTemplate::new(REQUEST_RESERVED_PACKAGE_IDENTITY)
        }
        DiagnosticKind::RequestStandardLibraryPackageIdentityRequired => {
            MessageTemplate::new(REQUEST_STANDARD_LIBRARY_PACKAGE_IDENTITY_REQUIRED)
        }
        DiagnosticKind::StandardLibraryArtifactReadFailed => {
            MessageTemplate::new(STANDARD_LIBRARY_ARTIFACT_READ_FAILED)
        }
        DiagnosticKind::StandardLibraryManifestInvalid => {
            MessageTemplate::new(STANDARD_LIBRARY_MANIFEST_INVALID)
        }
        DiagnosticKind::StandardLibraryInfrastructureFailure => {
            MessageTemplate::new(STANDARD_LIBRARY_INFRASTRUCTURE_FAILURE)
        }
        DiagnosticKind::StandardLibraryArtifactLengthMismatch => {
            MessageTemplate::new(STANDARD_LIBRARY_ARTIFACT_LENGTH_MISMATCH)
        }
        DiagnosticKind::StandardLibraryArtifactDigestMismatch => {
            MessageTemplate::new(STANDARD_LIBRARY_ARTIFACT_DIGEST_MISMATCH)
        }
        DiagnosticKind::StandardLibraryTargetUnavailable => {
            MessageTemplate::new(STANDARD_LIBRARY_TARGET_UNAVAILABLE)
        }
        DiagnosticKind::StandardLibraryRuntimeAbiMismatch => {
            MessageTemplate::new(STANDARD_LIBRARY_RUNTIME_ABI_MISMATCH)
        }
        DiagnosticKind::RequestDuplicateSourceInput => {
            MessageTemplate::new(REQUEST_DUPLICATE_SOURCE_INPUT)
        }
        DiagnosticKind::RequestInvalidWorkerBudget => {
            MessageTemplate::new(REQUEST_INVALID_WORKER_BUDGET)
        }
        DiagnosticKind::RequestUnsupportedProductEmission => {
            MessageTemplate::new(REQUEST_UNSUPPORTED_PRODUCT_EMISSION)
        }
        DiagnosticKind::InspectionReportWriteFailed => {
            MessageTemplate::new(INSPECTION_REPORT_WRITE_FAILED)
        }
        DiagnosticKind::CompilerProfileWriteFailed => {
            MessageTemplate::new(COMPILER_PROFILE_WRITE_FAILED)
        }
        DiagnosticKind::RuntimeArtifactMetadataReadFailed => {
            MessageTemplate::new(RUNTIME_ARTIFACT_METADATA_READ_FAILED)
        }
        DiagnosticKind::RuntimeArtifactMetadataInvalid => {
            MessageTemplate::new(RUNTIME_ARTIFACT_METADATA_INVALID)
        }
        DiagnosticKind::RuntimeArtifactTargetMismatch => {
            MessageTemplate::new(RUNTIME_ARTIFACT_TARGET_MISMATCH)
        }
        DiagnosticKind::RuntimeArtifactAbiMismatch => {
            MessageTemplate::new(RUNTIME_ARTIFACT_ABI_MISMATCH)
        }
        DiagnosticKind::RuntimeArtifactArchiveReadFailed => {
            MessageTemplate::new(RUNTIME_ARTIFACT_ARCHIVE_READ_FAILED)
        }
        DiagnosticKind::RuntimeArtifactArchiveInvalid => {
            MessageTemplate::new(RUNTIME_ARTIFACT_ARCHIVE_INVALID)
        }
        DiagnosticKind::RuntimeArtifactArchiveDigestMismatch => {
            MessageTemplate::new(RUNTIME_ARTIFACT_ARCHIVE_DIGEST_MISMATCH)
        }
        DiagnosticKind::ProjectManifestReadFailed => {
            MessageTemplate::new(PROJECT_MANIFEST_READ_FAILED)
        }
        DiagnosticKind::ProjectManifestParseFailed => {
            MessageTemplate::new(PROJECT_MANIFEST_PARSE_FAILED)
        }
        DiagnosticKind::ProjectManifestUnsupportedFormat => {
            MessageTemplate::new(PROJECT_MANIFEST_UNSUPPORTED_FORMAT)
        }
        DiagnosticKind::ProjectManifestInvalidPath => {
            MessageTemplate::new(PROJECT_MANIFEST_INVALID_PATH)
        }
        DiagnosticKind::ProjectManifestInvalidName => {
            MessageTemplate::new(PROJECT_MANIFEST_INVALID_NAME)
        }
        DiagnosticKind::ProjectManifestMissingSelection => {
            MessageTemplate::new(PROJECT_MANIFEST_MISSING_SELECTION)
        }
        DiagnosticKind::ProjectManifestMissingRootPackage => {
            MessageTemplate::new(PROJECT_MANIFEST_MISSING_ROOT_PACKAGE)
        }
        DiagnosticKind::ProjectManifestUndeclaredFeature => {
            MessageTemplate::new(PROJECT_MANIFEST_UNDECLARED_FEATURE)
        }
        DiagnosticKind::ProjectManifestUnknownSourceRoot => {
            MessageTemplate::new(PROJECT_MANIFEST_UNKNOWN_SOURCE_ROOT)
        }
        DiagnosticKind::ProjectManifestUnknownTarget => {
            MessageTemplate::new(PROJECT_MANIFEST_UNKNOWN_TARGET)
        }
        DiagnosticKind::ProjectManifestUnknownTargetPredicateProperty => {
            MessageTemplate::new(PROJECT_MANIFEST_UNKNOWN_TARGET_PREDICATE_PROPERTY)
        }
        DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch => {
            MessageTemplate::new(PROJECT_MANIFEST_TARGET_PREDICATE_VALUE_KIND_MISMATCH)
        }
        DiagnosticKind::ProjectManifestUnexpectedTestedLibrary => {
            MessageTemplate::new(PROJECT_MANIFEST_UNEXPECTED_TESTED_LIBRARY)
        }
        DiagnosticKind::ProjectPackageVersionInvalid => {
            MessageTemplate::new(PROJECT_PACKAGE_VERSION_INVALID)
        }
        DiagnosticKind::ProjectPackageVersionMissingWorkspace => {
            MessageTemplate::new(PROJECT_PACKAGE_VERSION_MISSING_WORKSPACE)
        }
        DiagnosticKind::ProjectPackageIdentityReserved => {
            MessageTemplate::new(PROJECT_PACKAGE_IDENTITY_RESERVED)
        }
        DiagnosticKind::ProjectStandardLibraryPackageIdentityRequired => {
            MessageTemplate::new(PROJECT_STANDARD_LIBRARY_PACKAGE_IDENTITY_REQUIRED)
        }
        DiagnosticKind::ProjectStandardLibraryRootPackageRequired => {
            MessageTemplate::new(PROJECT_STANDARD_LIBRARY_ROOT_PACKAGE_REQUIRED)
        }
        DiagnosticKind::ProjectManifestDuplicateSelection => {
            MessageTemplate::new(PROJECT_MANIFEST_DUPLICATE_SELECTION)
        }
        DiagnosticKind::ProjectSourceRootInvalid => {
            MessageTemplate::new(PROJECT_SOURCE_ROOT_INVALID)
        }
        DiagnosticKind::ProjectSourceRootContainsSymlink => {
            MessageTemplate::new(PROJECT_SOURCE_ROOT_CONTAINS_SYMLINK)
        }
        DiagnosticKind::ProjectSourceRootContainsNonUtf8Path => {
            MessageTemplate::new(PROJECT_SOURCE_ROOT_CONTAINS_NON_UTF8_PATH)
        }
        DiagnosticKind::ProjectDependencyPackageUnknown => {
            MessageTemplate::new(PROJECT_DEPENDENCY_PACKAGE_UNKNOWN)
        }
        DiagnosticKind::ProjectDependencyProductUnknown => {
            MessageTemplate::new(PROJECT_DEPENDENCY_PRODUCT_UNKNOWN)
        }
        DiagnosticKind::ProjectDependencyProductNotLibrary => {
            MessageTemplate::new(PROJECT_DEPENDENCY_PRODUCT_NOT_LIBRARY)
        }
        DiagnosticKind::ProjectDependencyProductTargetUnavailable => {
            MessageTemplate::new(PROJECT_DEPENDENCY_PRODUCT_TARGET_UNAVAILABLE)
        }
        DiagnosticKind::ProjectDependencyCycle => MessageTemplate::new(PROJECT_DEPENDENCY_CYCLE),
        DiagnosticKind::ProjectCommandSelectionInvalid => {
            MessageTemplate::new(PROJECT_COMMAND_SELECTION_INVALID)
        }
        DiagnosticKind::ProjectCommandFailed => MessageTemplate::new(PROJECT_COMMAND_FAILED),
        DiagnosticKind::ProjectCompilerDefect => MessageTemplate::new(PROJECT_COMMAND_FAILED),
        DiagnosticKind::ProjectInitializationIdentityInvalid => {
            MessageTemplate::new(PROJECT_INITIALIZATION_IDENTITY_INVALID)
        }
        DiagnosticKind::ProjectInitializationPathConflict => {
            MessageTemplate::new(PROJECT_INITIALIZATION_PATH_CONFLICT)
        }
        DiagnosticKind::ProjectInitializationWriteFailed => {
            MessageTemplate::new(PROJECT_INITIALIZATION_WRITE_FAILED)
        }
        DiagnosticKind::ProjectInitializationTargetUnsupported => {
            MessageTemplate::new(PROJECT_INITIALIZATION_TARGET_UNSUPPORTED)
        }
        DiagnosticKind::FormatterSourceNotFormatted => {
            MessageTemplate::new(FORMATTER_SOURCE_NOT_FORMATTED)
        }
        DiagnosticKind::FormatterSourceInvalidUtf8 => {
            MessageTemplate::new(FORMATTER_SOURCE_INVALID_UTF8)
        }
        DiagnosticKind::FormatterSourceTooLarge => MessageTemplate::new(FORMATTER_SOURCE_TOO_LARGE),
        DiagnosticKind::FormatterSourceWriteFailed => {
            MessageTemplate::new(FORMATTER_SOURCE_WRITE_FAILED)
        }
        DiagnosticKind::FormatterConfigurationReadFailed => {
            MessageTemplate::new(FORMATTER_CONFIGURATION_READ_FAILED)
        }
        DiagnosticKind::FormatterConfigurationMalformed => {
            MessageTemplate::new(FORMATTER_CONFIGURATION_MALFORMED)
        }
        DiagnosticKind::FormatterConfigurationUnknownRule => {
            MessageTemplate::new(FORMATTER_CONFIGURATION_UNKNOWN_RULE)
        }
        DiagnosticKind::FormatterConfigurationInvalidMaximumWidth => {
            MessageTemplate::new(FORMATTER_CONFIGURATION_INVALID_MAXIMUM_WIDTH)
        }
        DiagnosticKind::LexicalInvalidCharacter => MessageTemplate::new(LEXICAL_INVALID_CHARACTER),
        DiagnosticKind::LexicalMisplacedBom => MessageTemplate::new(LEXICAL_MISPLACED_BOM),
        DiagnosticKind::LexicalLoneCarriageReturn => {
            MessageTemplate::new(LEXICAL_LONE_CARRIAGE_RETURN)
        }
        DiagnosticKind::LexicalNonAsciiIdentifier => {
            MessageTemplate::new(LEXICAL_NON_ASCII_IDENTIFIER)
        }
        DiagnosticKind::LexicalInvalidIdentifier => {
            MessageTemplate::new(LEXICAL_INVALID_IDENTIFIER)
        }
        DiagnosticKind::LexicalInvalidOperatorOrPunctuation => {
            MessageTemplate::new(LEXICAL_INVALID_OPERATOR_OR_PUNCTUATION)
        }
        DiagnosticKind::LexicalMalformedNumericLiteral => {
            MessageTemplate::new(LEXICAL_MALFORMED_NUMERIC_LITERAL)
        }
        DiagnosticKind::LexicalInvalidNumericSuffix => {
            MessageTemplate::new(LEXICAL_INVALID_NUMERIC_SUFFIX)
        }
        DiagnosticKind::LexicalMalformedCharacterLiteral => {
            MessageTemplate::new(LEXICAL_MALFORMED_CHARACTER_LITERAL)
        }
        DiagnosticKind::LexicalUnterminatedCharacterLiteral => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_CHARACTER_LITERAL)
        }
        DiagnosticKind::LexicalUnterminatedStringLiteral => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_STRING_LITERAL)
        }
        DiagnosticKind::LexicalUnknownEscape => MessageTemplate::new(LEXICAL_UNKNOWN_ESCAPE),
        DiagnosticKind::LexicalInvalidUnicodeEscape => {
            MessageTemplate::new(LEXICAL_INVALID_UNICODE_ESCAPE)
        }
        DiagnosticKind::LexicalUnterminatedBlockComment => {
            MessageTemplate::new(LEXICAL_UNTERMINATED_BLOCK_COMMENT)
        }
        DiagnosticKind::SyntaxExpectedToken => MessageTemplate::new(SYNTAX_EXPECTED_TOKEN),
        DiagnosticKind::SyntaxExpectedExpression => {
            MessageTemplate::new(SYNTAX_EXPECTED_EXPRESSION)
        }
        DiagnosticKind::SyntaxUnexpectedEof => MessageTemplate::new(SYNTAX_UNEXPECTED_EOF),
        DiagnosticKind::SyntaxNestingLimitExceeded => {
            MessageTemplate::new(SYNTAX_NESTING_LIMIT_EXCEEDED)
        }
        DiagnosticKind::DeclarationDuplicateName => {
            MessageTemplate::new(DECLARATION_DUPLICATE_NAME)
        }
        DiagnosticKind::DeclarationConflictingModuleVisibility => {
            MessageTemplate::new(DECLARATION_CONFLICTING_MODULE_VISIBILITY)
        }
        DiagnosticKind::DeclarationConflictingModuleTrust => {
            MessageTemplate::new(DECLARATION_CONFLICTING_MODULE_TRUST)
        }
        DiagnosticKind::DeclarationDuplicateModifier => {
            MessageTemplate::new(DECLARATION_DUPLICATE_MODIFIER)
        }
        DiagnosticKind::DeclarationIncompatibleModifiers => {
            MessageTemplate::new(DECLARATION_INCOMPATIBLE_MODIFIERS)
        }
        DiagnosticKind::DeclarationInvalidModifier => {
            MessageTemplate::new(DECLARATION_INVALID_MODIFIER)
        }
        DiagnosticKind::DeclarationBodyRequired => MessageTemplate::new(DECLARATION_BODY_REQUIRED),
        DiagnosticKind::DeclarationBodyNotAllowed => {
            MessageTemplate::new(DECLARATION_BODY_NOT_ALLOWED)
        }
        DiagnosticKind::DeclarationDuplicateDirective => {
            MessageTemplate::new(DECLARATION_DUPLICATE_DIRECTIVE)
        }
        DiagnosticKind::DeclarationIncompatibleDirectives => {
            MessageTemplate::new(DECLARATION_INCOMPATIBLE_DIRECTIVES)
        }
        DiagnosticKind::DeclarationInvalidDirectiveTarget => {
            MessageTemplate::new(DECLARATION_INVALID_DIRECTIVE_TARGET)
        }
        DiagnosticKind::DeclarationInvalidParameterOrder => {
            MessageTemplate::new(DECLARATION_INVALID_PARAMETER_ORDER)
        }
        DiagnosticKind::DeclarationInvalidMemberPlacement => {
            MessageTemplate::new(DECLARATION_INVALID_MEMBER_PLACEMENT)
        }
        DiagnosticKind::DeclarationDuplicateLifecycleSlot => {
            MessageTemplate::new(DECLARATION_DUPLICATE_LIFECYCLE_SLOT)
        }
        DiagnosticKind::BindingUnresolvedName => MessageTemplate::new(BINDING_UNRESOLVED_NAME),
        DiagnosticKind::BindingAmbiguousName => MessageTemplate::new(BINDING_AMBIGUOUS_NAME),
        DiagnosticKind::BindingInaccessibleName => MessageTemplate::new(BINDING_INACCESSIBLE_NAME),
        DiagnosticKind::BindingWrongNameKind => MessageTemplate::new(BINDING_WRONG_NAME_KIND),
        DiagnosticKind::BindingMalformedName => MessageTemplate::new(BINDING_MALFORMED_NAME),
        DiagnosticKind::BindingNameAlreadyDefined => {
            MessageTemplate::new(BINDING_NAME_ALREADY_DEFINED)
        }
        DiagnosticKind::BindingIncoherentAlternativePattern => {
            MessageTemplate::new(BINDING_INCOHERENT_ALTERNATIVE_PATTERN)
        }
        DiagnosticKind::CheckingIncompatibleExpressionType => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_EXPRESSION_TYPE)
        }
        DiagnosticKind::CheckingIncompatiblePattern => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_PATTERN)
        }
        DiagnosticKind::CheckingRefutablePattern => {
            MessageTemplate::new(CHECKING_REFUTABLE_PATTERN)
        }
        DiagnosticKind::CheckingNonExhaustiveMatch => {
            MessageTemplate::new(CHECKING_NON_EXHAUSTIVE_MATCH)
        }
        DiagnosticKind::CheckingUnreachableMatchArm => {
            MessageTemplate::new(CHECKING_UNREACHABLE_MATCH_ARM)
        }
        DiagnosticKind::CheckingUnreachablePatternAlternative => {
            MessageTemplate::new(CHECKING_UNREACHABLE_PATTERN_ALTERNATIVE)
        }
        DiagnosticKind::CheckingCannotInferExpressionType => {
            MessageTemplate::new(CHECKING_CANNOT_INFER_EXPRESSION_TYPE)
        }
        DiagnosticKind::CheckingRangeBoundTypeMustBeInteger => {
            MessageTemplate::new(CHECKING_RANGE_BOUND_TYPE_MUST_BE_INTEGER)
        }
        DiagnosticKind::CheckingNoCompatiblePropagationBoundary => {
            MessageTemplate::new(CHECKING_NO_COMPATIBLE_PROPAGATION_BOUNDARY)
        }
        DiagnosticKind::CheckingInvalidConstantExpression => {
            MessageTemplate::new(CHECKING_INVALID_CONSTANT_EXPRESSION)
        }
        DiagnosticKind::CheckingInvalidConstantOperation => {
            MessageTemplate::new(CHECKING_INVALID_CONSTANT_OPERATION)
        }
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable => {
            MessageTemplate::new(CHECKING_ARRAY_GENERATOR_CARDINALITY_NOT_PROVABLE)
        }
        DiagnosticKind::CheckingInvalidStoredType => {
            MessageTemplate::new(CHECKING_INVALID_STORED_TYPE)
        }
        DiagnosticKind::CheckingRecursiveTypeRepresentation => {
            MessageTemplate::new(CHECKING_RECURSIVE_TYPE_REPRESENTATION)
        }
        DiagnosticKind::CheckingTypeRepresentationRecursionLimitExceeded => {
            MessageTemplate::new(CHECKING_TYPE_REPRESENTATION_RECURSION_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingInvalidLayoutDirective => {
            MessageTemplate::new(CHECKING_INVALID_LAYOUT_DIRECTIVE)
        }
        DiagnosticKind::CheckingInvalidCopyContract => {
            MessageTemplate::new(CHECKING_INVALID_COPY_CONTRACT)
        }
        DiagnosticKind::CheckingInvalidUnionTag => MessageTemplate::new(CHECKING_INVALID_UNION_TAG),
        DiagnosticKind::CheckingRefinementCapacityExceeded => {
            MessageTemplate::new(CHECKING_REFINEMENT_CAPACITY_EXCEEDED)
        }
        DiagnosticKind::CheckingUseOfMovedStorage => {
            MessageTemplate::new(CHECKING_USE_OF_MOVED_STORAGE)
        }
        DiagnosticKind::CheckingConflictingBorrow => {
            MessageTemplate::new(CHECKING_CONFLICTING_BORROW)
        }
        DiagnosticKind::CheckingMissingMutationAuthority => {
            MessageTemplate::new(CHECKING_MISSING_MUTATION_AUTHORITY)
        }
        DiagnosticKind::CheckingMissingStorageOwnership => {
            MessageTemplate::new(CHECKING_MISSING_STORAGE_OWNERSHIP)
        }
        DiagnosticKind::CheckingMissingTraitFulfillment => {
            MessageTemplate::new(CHECKING_MISSING_TRAIT_FULFILLMENT)
        }
        DiagnosticKind::CheckingExtraTraitFulfillment => {
            MessageTemplate::new(CHECKING_EXTRA_TRAIT_FULFILLMENT)
        }
        DiagnosticKind::CheckingIncompatibleTraitFulfillment => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_TRAIT_FULFILLMENT)
        }
        DiagnosticKind::CheckingDuplicateTraitFulfillment => {
            MessageTemplate::new(CHECKING_DUPLICATE_TRAIT_FULFILLMENT)
        }
        DiagnosticKind::CheckingOverlappingImplementation => {
            MessageTemplate::new(CHECKING_OVERLAPPING_IMPLEMENTATION)
        }
        DiagnosticKind::CheckingUngroupedImplementationOverloads => {
            MessageTemplate::new(CHECKING_UNGROUPED_IMPLEMENTATION_OVERLOADS)
        }
        DiagnosticKind::CheckingInvalidImplementationOverloadHeader => {
            MessageTemplate::new(CHECKING_INVALID_IMPLEMENTATION_OVERLOAD_HEADER)
        }
        DiagnosticKind::CheckingInvalidImplementationOverloadArm => {
            MessageTemplate::new(CHECKING_INVALID_IMPLEMENTATION_OVERLOAD_ARM)
        }
        DiagnosticKind::CheckingDuplicateImplementationOverloadArm => {
            MessageTemplate::new(CHECKING_DUPLICATE_IMPLEMENTATION_OVERLOAD_ARM)
        }
        DiagnosticKind::CheckingInvalidCallableOverloadArm => {
            MessageTemplate::new(CHECKING_INVALID_CALLABLE_OVERLOAD_ARM)
        }
        DiagnosticKind::CheckingDuplicateCallableOverloadArm => {
            MessageTemplate::new(CHECKING_DUPLICATE_CALLABLE_OVERLOAD_ARM)
        }
        DiagnosticKind::CheckingConflictingCallableOverloadFamily => {
            MessageTemplate::new(CHECKING_CONFLICTING_CALLABLE_OVERLOAD_FAMILY)
        }
        DiagnosticKind::CheckingConflictingCallableOverloadSignature => {
            MessageTemplate::new(CHECKING_CONFLICTING_CALLABLE_OVERLOAD_SIGNATURE)
        }
        DiagnosticKind::CheckingImplementationCoherenceLimitExceeded => {
            MessageTemplate::new(CHECKING_IMPLEMENTATION_COHERENCE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingCallableOverloadLimitExceeded => {
            MessageTemplate::new(CHECKING_CALLABLE_OVERLOAD_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingMissingForeignCallableDirective => {
            MessageTemplate::new(CHECKING_MISSING_FOREIGN_CALLABLE_DIRECTIVE)
        }
        DiagnosticKind::CheckingForeignCallableRequiresTrusted => {
            MessageTemplate::new(CHECKING_FOREIGN_CALLABLE_REQUIRES_TRUSTED)
        }
        DiagnosticKind::CheckingForeignCallableRequiresCapability => {
            MessageTemplate::new(CHECKING_FOREIGN_CALLABLE_REQUIRES_CAPABILITY)
        }
        DiagnosticKind::CheckingForeignCallableExecutionUnsupported => {
            MessageTemplate::new(CHECKING_FOREIGN_CALLABLE_EXECUTION_UNSUPPORTED)
        }
        DiagnosticKind::CheckingForeignAbiTypeUnsupported => {
            MessageTemplate::new(CHECKING_FOREIGN_ABI_TYPE_UNSUPPORTED)
        }
        DiagnosticKind::CheckingPlatformServiceSignatureMismatch => {
            MessageTemplate::new(CHECKING_PLATFORM_SERVICE_SIGNATURE_MISMATCH)
        }
        DiagnosticKind::CheckingInvalidNativeLinkDirective => {
            MessageTemplate::new(CHECKING_INVALID_NATIVE_LINK_DIRECTIVE)
        }
        DiagnosticKind::CheckingInvalidNativeSymbolDirective => {
            MessageTemplate::new(CHECKING_INVALID_NATIVE_SYMBOL_DIRECTIVE)
        }
        DiagnosticKind::CheckingDuplicateNativeSymbol => {
            MessageTemplate::new(CHECKING_DUPLICATE_NATIVE_SYMBOL)
        }
        DiagnosticKind::CheckingUnavailableNativeLinkInput => {
            MessageTemplate::new(CHECKING_UNAVAILABLE_NATIVE_LINK_INPUT)
        }
        DiagnosticKind::CheckingUndeclaredTrustedCapability => {
            MessageTemplate::new(CHECKING_UNDECLARED_TRUSTED_CAPABILITY)
        }
        DiagnosticKind::CheckingUnusedTrustedCapability => {
            MessageTemplate::new(CHECKING_UNUSED_TRUSTED_CAPABILITY)
        }
        DiagnosticKind::CheckingTrustedCapabilityRequiresTrustedCallable => {
            MessageTemplate::new(CHECKING_TRUSTED_CAPABILITY_REQUIRES_TRUSTED_CALLABLE)
        }
        DiagnosticKind::CheckingAwaitOutsideAsyncCallable => {
            MessageTemplate::new(CHECKING_AWAIT_OUTSIDE_ASYNC_CALLABLE)
        }
        DiagnosticKind::CheckingTaskStartOutsideAsyncCallable => {
            MessageTemplate::new(CHECKING_TASK_START_OUTSIDE_ASYNC_CALLABLE)
        }
        DiagnosticKind::CheckingUnavailableAwaitDependency => {
            MessageTemplate::new(CHECKING_UNAVAILABLE_AWAIT_DEPENDENCY)
        }
        DiagnosticKind::CheckingEntrypointNotAllowed => {
            MessageTemplate::new(CHECKING_ENTRYPOINT_NOT_ALLOWED)
        }
        DiagnosticKind::CheckingMissingEntrypoint => {
            MessageTemplate::new(CHECKING_MISSING_ENTRYPOINT)
        }
        DiagnosticKind::CheckingDuplicateEntrypoint => {
            MessageTemplate::new(CHECKING_DUPLICATE_ENTRYPOINT)
        }
        DiagnosticKind::CheckingEntryCannotBeGeneric => {
            MessageTemplate::new(CHECKING_ENTRY_CANNOT_BE_GENERIC)
        }
        DiagnosticKind::CheckingEntryCannotTakeParameters => {
            MessageTemplate::new(CHECKING_ENTRY_CANNOT_TAKE_PARAMETERS)
        }
        DiagnosticKind::CheckingInvalidEntrypointResult => {
            MessageTemplate::new(CHECKING_INVALID_ENTRYPOINT_RESULT)
        }
        DiagnosticKind::CheckingInvalidTestResult => {
            MessageTemplate::new(CHECKING_INVALID_TEST_RESULT)
        }
        DiagnosticKind::CheckingInvalidTestEntryDirective => {
            MessageTemplate::new(CHECKING_INVALID_TEST_ENTRY_DIRECTIVE)
        }
        DiagnosticKind::CheckingInvalidTestModuleDirective => {
            MessageTemplate::new(CHECKING_INVALID_TEST_MODULE_DIRECTIVE)
        }
        DiagnosticKind::CheckingDuplicateTestIdentity => {
            MessageTemplate::new(CHECKING_DUPLICATE_TEST_IDENTITY)
        }
        DiagnosticKind::CheckingEntryCannotBeConstant => {
            MessageTemplate::new(CHECKING_ENTRY_CANNOT_BE_CONSTANT)
        }
        DiagnosticKind::CheckingEntryCannotRequireTrust => {
            MessageTemplate::new(CHECKING_ENTRY_CANNOT_REQUIRE_TRUST)
        }
        DiagnosticKind::CheckingExportDependsOnInternalDeclaration => {
            MessageTemplate::new(CHECKING_EXPORT_DEPENDS_ON_INTERNAL_DECLARATION)
        }
        DiagnosticKind::CheckingConstantLiteralNotRepresentable => {
            MessageTemplate::new(CHECKING_CONSTANT_LITERAL_NOT_REPRESENTABLE)
        }
        DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_EVALUATION_STEP_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantAggregateLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_AGGREGATE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantExpansionLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_EXPANSION_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_LITERAL_SIZE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingConstantIntegerSizeLimitExceeded => {
            MessageTemplate::new(CHECKING_CONSTANT_INTEGER_SIZE_LIMIT_EXCEEDED)
        }
        DiagnosticKind::CheckingCyclicConstantDefinition => {
            MessageTemplate::new(CHECKING_CYCLIC_CONSTANT_DEFINITION)
        }
        DiagnosticKind::CheckingConstantDivisionByZero => {
            MessageTemplate::new(CHECKING_CONSTANT_DIVISION_BY_ZERO)
        }
        DiagnosticKind::CheckingConstantValueNotRepresentable => {
            MessageTemplate::new(CHECKING_CONSTANT_VALUE_NOT_REPRESENTABLE)
        }
        DiagnosticKind::CheckingNoApplicableCandidate => {
            MessageTemplate::new(CHECKING_NO_APPLICABLE_CANDIDATE)
        }
        DiagnosticKind::CheckingMutableIndexContractRequired => {
            MessageTemplate::new(CHECKING_MUTABLE_INDEX_CONTRACT_REQUIRED)
        }
        DiagnosticKind::CheckingUnknownUnionVariant => {
            MessageTemplate::new(CHECKING_UNKNOWN_UNION_VARIANT)
        }
        DiagnosticKind::CheckingAmbiguousCandidate => {
            MessageTemplate::new(CHECKING_AMBIGUOUS_CANDIDATE)
        }
        DiagnosticKind::CheckingIncompatibleCandidate => {
            MessageTemplate::new(CHECKING_INCOMPATIBLE_CANDIDATE)
        }
        DiagnosticKind::CheckingTargetRepresentationUnavailable => {
            MessageTemplate::new(CHECKING_TARGET_REPRESENTATION_UNAVAILABLE)
        }
        DiagnosticKind::CheckingTargetCallableAbiUnavailable => {
            MessageTemplate::new(CHECKING_TARGET_CALLABLE_ABI_UNAVAILABLE)
        }
        DiagnosticKind::CheckingTargetAlignmentUnsupported => {
            MessageTemplate::new(CHECKING_TARGET_ALIGNMENT_UNSUPPORTED)
        }
        DiagnosticKind::CheckingTargetAbiRepresentationUnsupported => {
            MessageTemplate::new(CHECKING_TARGET_ABI_REPRESENTATION_UNSUPPORTED)
        }
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable => {
            MessageTemplate::new(CHECKING_TARGET_MEMORY_OPERATION_UNAVAILABLE)
        }
        DiagnosticKind::CheckingInvalidTargetControlContract => {
            MessageTemplate::new(CHECKING_INVALID_TARGET_CONTROL_CONTRACT)
        }
        DiagnosticKind::CheckingInvalidAtomicMemoryOrder => {
            MessageTemplate::new(CHECKING_INVALID_ATOMIC_MEMORY_ORDER)
        }
        DiagnosticKind::CheckingInvalidCallbackStateContext => {
            MessageTemplate::new(CHECKING_INVALID_CALLBACK_STATE_CONTEXT)
        }
        DiagnosticKind::CheckingMissingTrustedMemoryGuarantees => {
            MessageTemplate::new(CHECKING_MISSING_TRUSTED_MEMORY_GUARANTEES)
        }
        DiagnosticKind::CheckingMemoryOperationAfterDeallocation => {
            MessageTemplate::new(CHECKING_MEMORY_OPERATION_AFTER_DEALLOCATION)
        }
        DiagnosticKind::CheckingUninitializedRawStorage => {
            MessageTemplate::new(CHECKING_UNINITIALIZED_RAW_STORAGE)
        }
        DiagnosticKind::CheckingDeallocationWithOutstandingObligations => {
            MessageTemplate::new(CHECKING_DEALLOCATION_WITH_OUTSTANDING_OBLIGATIONS)
        }
        DiagnosticKind::BindingInvalidCallableAbi => {
            MessageTemplate::new(BINDING_INVALID_CALLABLE_ABI)
        }
        DiagnosticKind::BindingDuplicateCallableAbi => {
            MessageTemplate::new(BINDING_DUPLICATE_CALLABLE_ABI)
        }
        DiagnosticKind::BindingCyclicModuleExport => {
            MessageTemplate::new(BINDING_CYCLIC_MODULE_EXPORT)
        }
        DiagnosticKind::BindingConflictingModuleExport => {
            MessageTemplate::new(BINDING_CONFLICTING_MODULE_EXPORT)
        }
        DiagnosticKind::BindingInvalidModuleExportTarget => {
            MessageTemplate::new(BINDING_INVALID_MODULE_EXPORT_TARGET)
        }
        DiagnosticKind::BindingMalformedDirectiveArgument => {
            MessageTemplate::new(BINDING_MALFORMED_DIRECTIVE_ARGUMENT)
        }
        DiagnosticKind::CheckingDuplicateModuleContributionDirective => {
            MessageTemplate::new(CHECKING_DUPLICATE_MODULE_CONTRIBUTION_DIRECTIVE)
        }
        DiagnosticKind::EmissionMissingContribution => {
            MessageTemplate::new(EMISSION_MISSING_CONTRIBUTION)
        }
        DiagnosticKind::EmissionInvalidContribution => {
            MessageTemplate::new(EMISSION_INVALID_CONTRIBUTION)
        }
        DiagnosticKind::EmissionArtifactReadFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_READ_FAILED)
        }
        DiagnosticKind::EmissionArtifactOpenFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_OPEN_FAILED)
        }
        DiagnosticKind::EmissionArtifactWriteFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_WRITE_FAILED)
        }
        DiagnosticKind::EmissionArtifactFlushFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_FLUSH_FAILED)
        }
        DiagnosticKind::EmissionArtifactDigestMismatch => {
            MessageTemplate::new(EMISSION_ARTIFACT_DIGEST_MISMATCH)
        }
        DiagnosticKind::EmissionArtifactLengthMismatch => {
            MessageTemplate::new(EMISSION_ARTIFACT_LENGTH_MISMATCH)
        }
        DiagnosticKind::EmissionArtifactCommitFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_COMMIT_FAILED)
        }
        DiagnosticKind::EmissionManagedPublicationUnsupported => {
            MessageTemplate::new(EMISSION_MANAGED_PUBLICATION_UNSUPPORTED)
        }
        DiagnosticKind::EmissionGenerationCollision => {
            MessageTemplate::new(EMISSION_GENERATION_COLLISION)
        }
        DiagnosticKind::EmissionGenerationManifestInvalid => {
            MessageTemplate::new(EMISSION_GENERATION_MANIFEST_INVALID)
        }
        DiagnosticKind::EmissionLinkedPlanMissing => {
            MessageTemplate::new(EMISSION_LINKED_PLAN_MISSING)
        }
        DiagnosticKind::EmissionFailed => MessageTemplate::new(EMISSION_FAILED),
        DiagnosticKind::EmissionTargetMismatch => MessageTemplate::new(EMISSION_TARGET_MISMATCH),
        DiagnosticKind::EmissionProductMismatch => MessageTemplate::new(EMISSION_PRODUCT_MISMATCH),
        DiagnosticKind::EmissionArtifactIoFailed => {
            MessageTemplate::new(EMISSION_ARTIFACT_IO_FAILED)
        }
        DiagnosticKind::CodegenUnsupportedTarget => {
            MessageTemplate::new(CODEGEN_UNSUPPORTED_TARGET)
        }
        DiagnosticKind::CodegenUnsupportedArtifact => {
            MessageTemplate::new(CODEGEN_UNSUPPORTED_ARTIFACT)
        }
        DiagnosticKind::CodegenInvalidConfiguration => {
            MessageTemplate::new(CODEGEN_INVALID_CONFIGURATION)
        }
        DiagnosticKind::CodegenResourceExhausted => {
            MessageTemplate::new(CODEGEN_RESOURCE_EXHAUSTED)
        }
        DiagnosticKind::CodegenBackendLibraryFailed => {
            MessageTemplate::new(CODEGEN_BACKEND_LIBRARY_FAILED)
        }
        DiagnosticKind::CodegenGeneratedModuleInvalid => {
            MessageTemplate::new(CODEGEN_GENERATED_MODULE_INVALID)
        }
        DiagnosticKind::CodegenArtifactConstructionFailed => {
            MessageTemplate::new(CODEGEN_ARTIFACT_CONSTRUCTION_FAILED)
        }
        DiagnosticKind::NativeProductPreparationFailed => {
            MessageTemplate::new(NATIVE_PRODUCT_PREPARATION_FAILED)
        }
        DiagnosticKind::LinkerUnsupportedTarget => MessageTemplate::new(LINKER_UNSUPPORTED_TARGET),
        DiagnosticKind::LinkerUnsupportedProduct => {
            MessageTemplate::new(LINKER_UNSUPPORTED_PRODUCT)
        }
        DiagnosticKind::LinkerUnsupportedInput => MessageTemplate::new(LINKER_UNSUPPORTED_INPUT),
        DiagnosticKind::LinkerUnsupportedInputMode => {
            MessageTemplate::new(LINKER_UNSUPPORTED_INPUT_MODE)
        }
        DiagnosticKind::LinkerUnsupportedOutput => MessageTemplate::new(LINKER_UNSUPPORTED_OUTPUT),
        DiagnosticKind::LinkerUnsupportedSearchPath => {
            MessageTemplate::new(LINKER_UNSUPPORTED_SEARCH_PATH)
        }
        DiagnosticKind::LinkerUnsupportedLinkModel => {
            MessageTemplate::new(LINKER_UNSUPPORTED_LINK_MODEL)
        }
        DiagnosticKind::LinkerUnsupportedDeadStrip => {
            MessageTemplate::new(LINKER_UNSUPPORTED_DEAD_STRIP)
        }
        DiagnosticKind::LinkerUnsupportedSectionGarbageCollection => {
            MessageTemplate::new(LINKER_UNSUPPORTED_SECTION_GARBAGE_COLLECTION)
        }
        DiagnosticKind::LinkerUnsupportedDebug => MessageTemplate::new(LINKER_UNSUPPORTED_DEBUG),
        DiagnosticKind::LinkerUnsupportedSubsystem => {
            MessageTemplate::new(LINKER_UNSUPPORTED_SUBSYSTEM)
        }
        DiagnosticKind::LinkerUnsupportedSymbol => MessageTemplate::new(LINKER_UNSUPPORTED_SYMBOL),
        DiagnosticKind::LinkerUnsupportedStartup => {
            MessageTemplate::new(LINKER_UNSUPPORTED_STARTUP)
        }
        DiagnosticKind::LinkerUnsupportedRuntime => {
            MessageTemplate::new(LINKER_UNSUPPORTED_RUNTIME)
        }
        DiagnosticKind::LinkerDriverUnavailable => MessageTemplate::new(LINKER_DRIVER_UNAVAILABLE),
        DiagnosticKind::LinkerDriverIncompatible => {
            MessageTemplate::new(LINKER_DRIVER_INCOMPATIBLE)
        }
        DiagnosticKind::LinkerInputMissing => MessageTemplate::new(LINKER_INPUT_MISSING),
        DiagnosticKind::LinkerResponseFileFailed => {
            MessageTemplate::new(LINKER_RESPONSE_FILE_FAILED)
        }
        DiagnosticKind::LinkerInvocationFailed => MessageTemplate::new(LINKER_INVOCATION_FAILED),
        DiagnosticKind::LinkerExternalToolIoFailed => {
            MessageTemplate::new(LINKER_EXTERNAL_TOOL_IO_FAILED)
        }
        DiagnosticKind::LinkerExternalToolContractFailed => {
            MessageTemplate::new(LINKER_EXTERNAL_TOOL_CONTRACT_FAILED)
        }
        DiagnosticKind::LinkerExternalToolExitedUnsuccessfully => {
            MessageTemplate::new(LINKER_EXTERNAL_TOOL_EXITED_UNSUCCESSFULLY)
        }
        DiagnosticKind::LinkerOutputMissing => MessageTemplate::new(LINKER_OUTPUT_MISSING),
        DiagnosticKind::LinkerOutputInvalid => MessageTemplate::new(LINKER_OUTPUT_INVALID),
        DiagnosticKind::LinkerResourceExhausted => MessageTemplate::new(LINKER_RESOURCE_EXHAUSTED),
        DiagnosticKind::InterfaceInvalidMagic
        | DiagnosticKind::InterfaceUnsupportedFormatRevision
        | DiagnosticKind::InterfaceUnsupportedLanguageRevision
        | DiagnosticKind::InterfaceUnsupportedEncoding
        | DiagnosticKind::InterfaceTruncated
        | DiagnosticKind::InterfaceMalformed
        | DiagnosticKind::InterfaceHashMismatch
        | DiagnosticKind::InterfaceSectionChecksumMismatch
        | DiagnosticKind::InterfaceResourceLimitExceeded
        | DiagnosticKind::InterfacePackageIdentityMismatch
        | DiagnosticKind::InterfaceProductIdentityMismatch
        | DiagnosticKind::InterfaceDuplicatePackage
        | DiagnosticKind::InterfaceMissingDependency
        | DiagnosticKind::InterfaceDependencyProductMismatch
        | DiagnosticKind::InterfaceDependencyContentMismatch
        | DiagnosticKind::InterfaceSymbolReferenceInvalid
        | DiagnosticKind::InterfaceDependencyReferenceInvalid
        | DiagnosticKind::InterfaceDependencySymbolMissing
        | DiagnosticKind::InterfaceCompilerDeclarationExported
        | DiagnosticKind::InterfaceSymbolGraphInvalid
        | DiagnosticKind::InterfaceSymbolCapacityExceeded
        | DiagnosticKind::InterfaceSemanticSymbolUnresolved
        | DiagnosticKind::InterfaceSemanticSymbolKindInvalid
        | DiagnosticKind::InterfaceSemanticValueGraphInvalid
        | DiagnosticKind::InterfaceSemanticValueInvalid
        | DiagnosticKind::InterfaceExecutableTemplateInvalid
        | DiagnosticKind::InterfaceSupportEntityInvalid
        | DiagnosticKind::InterfaceConstantCallableBodyUnavailable
        | DiagnosticKind::InterfaceExecutableTemplateUnavailable => {
            interface_diagnostic_template(kind)
        }
    }
}

pub(crate) const fn note_template(kind: DiagnosticNoteKind) -> MessageTemplate {
    match kind {
        DiagnosticNoteKind::SourceFileMustBeReadable => {
            MessageTemplate::new(NOTE_SOURCE_FILE_MUST_BE_READABLE)
        }
        DiagnosticNoteKind::SourceMustBeUtf8 => MessageTemplate::new(NOTE_SOURCE_MUST_BE_UTF8),
        DiagnosticNoteKind::SourceMustMatchFormatterOutput => {
            MessageTemplate::new(NOTE_SOURCE_MUST_MATCH_FORMATTER_OUTPUT)
        }
        DiagnosticNoteKind::SourceIdsAreCompact => {
            MessageTemplate::new(NOTE_SOURCE_IDS_ARE_COMPACT)
        }
        DiagnosticNoteKind::SourceTextOffsetsAreCompact => {
            MessageTemplate::new(NOTE_SOURCE_TEXT_OFFSETS_ARE_COMPACT)
        }
        DiagnosticNoteKind::SourceInputRequired => MessageTemplate::new(NOTE_SOURCE_INPUT_REQUIRED),
        DiagnosticNoteKind::SourceInputNeedsStableIdentity => {
            MessageTemplate::new(NOTE_SOURCE_INPUT_NEEDS_STABLE_IDENTITY)
        }
        DiagnosticNoteKind::WorkerBudgetMustBePositive => {
            MessageTemplate::new(NOTE_WORKER_BUDGET_MUST_BE_POSITIVE)
        }
        DiagnosticNoteKind::PackageIdentityMustBeValid => {
            MessageTemplate::new(NOTE_PACKAGE_IDENTITY_MUST_BE_VALID)
        }
        DiagnosticNoteKind::PackageVersionMustBeValid => {
            MessageTemplate::new(NOTE_PACKAGE_VERSION_MUST_BE_VALID)
        }
        DiagnosticNoteKind::CharacterNotAccepted => {
            MessageTemplate::new(NOTE_CHARACTER_NOT_ACCEPTED)
        }
        DiagnosticNoteKind::BomOnlyAllowedAtStart => {
            MessageTemplate::new(NOTE_BOM_ONLY_ALLOWED_AT_START)
        }
        DiagnosticNoteKind::LineBreaksMustBeLfOrCrlf => {
            MessageTemplate::new(NOTE_LINE_BREAKS_MUST_BE_LF_OR_CRLF)
        }
        DiagnosticNoteKind::IdentifiersMustBeAscii => {
            MessageTemplate::new(NOTE_IDENTIFIERS_MUST_BE_ASCII)
        }
        DiagnosticNoteKind::IdentifierSpellingMustBeValid => {
            MessageTemplate::new(NOTE_IDENTIFIER_SPELLING_MUST_BE_VALID)
        }
        DiagnosticNoteKind::OnlyImaginaryNumericSuffix => {
            MessageTemplate::new(NOTE_ONLY_IMAGINARY_NUMERIC_SUFFIX)
        }
        DiagnosticNoteKind::CharacterLiteralMustContainOneScalar => {
            MessageTemplate::new(NOTE_CHARACTER_LITERAL_MUST_CONTAIN_ONE_SCALAR)
        }
        DiagnosticNoteKind::CharacterLiteralNeedsTerminator => {
            MessageTemplate::new(NOTE_CHARACTER_LITERAL_NEEDS_TERMINATOR)
        }
        DiagnosticNoteKind::StringLiteralNeedsTerminator => {
            MessageTemplate::new(NOTE_STRING_LITERAL_NEEDS_TERMINATOR)
        }
        DiagnosticNoteKind::EscapeMustBeKnown => MessageTemplate::new(NOTE_ESCAPE_MUST_BE_KNOWN),
        DiagnosticNoteKind::UnicodeEscapeMustBeScalar => {
            MessageTemplate::new(NOTE_UNICODE_ESCAPE_MUST_BE_SCALAR)
        }
        DiagnosticNoteKind::BlockCommentNeedsTerminator => {
            MessageTemplate::new(NOTE_BLOCK_COMMENT_NEEDS_TERMINATOR)
        }
        DiagnosticNoteKind::CallableAbiDirectiveMustNameSupportedAbi => {
            MessageTemplate::new(NOTE_CALLABLE_ABI_DIRECTIVE_MUST_NAME_SUPPORTED_ABI)
        }
        DiagnosticNoteKind::DirectiveArgumentMustHaveCompleteForm => {
            MessageTemplate::new(NOTE_DIRECTIVE_ARGUMENT_MUST_HAVE_COMPLETE_FORM)
        }
        DiagnosticNoteKind::TypeInferenceNeedsConstraint => {
            MessageTemplate::new(NOTE_TYPE_INFERENCE_NEEDS_CONSTRAINT)
        }
        DiagnosticNoteKind::ConstantExpressionMustBeEvaluable => {
            MessageTemplate::new(NOTE_CONSTANT_EXPRESSION_MUST_BE_EVALUABLE)
        }
        DiagnosticNoteKind::ConstantEvaluationMustFitLimits => {
            MessageTemplate::new(NOTE_CONSTANT_EVALUATION_MUST_FIT_LIMITS)
        }
        DiagnosticNoteKind::TypeLayoutDirectiveForms => {
            MessageTemplate::new(NOTE_TYPE_LAYOUT_DIRECTIVE_FORMS)
        }
        DiagnosticNoteKind::UnionTagDirectiveForms => {
            MessageTemplate::new(NOTE_UNION_TAG_DIRECTIVE_FORMS)
        }
        DiagnosticNoteKind::CopyContractRequirements => {
            MessageTemplate::new(NOTE_COPY_CONTRACT_REQUIREMENTS)
        }
        DiagnosticNoteKind::StoredTypeRequiresIndirection => {
            MessageTemplate::new(NOTE_STORED_TYPE_REQUIRES_INDIRECTION)
        }
        DiagnosticNoteKind::SelectionMustBeDisambiguated => {
            MessageTemplate::new(NOTE_SELECTION_MUST_BE_DISAMBIGUATED)
        }
        DiagnosticNoteKind::PropagationBoundaryMustMatch => {
            MessageTemplate::new(NOTE_PROPAGATION_BOUNDARY_MUST_MATCH)
        }
        DiagnosticNoteKind::ArrayGeneratorMustYieldOncePerElement => {
            MessageTemplate::new(NOTE_ARRAY_GENERATOR_MUST_YIELD_ONCE_PER_ELEMENT)
        }
        DiagnosticNoteKind::CallbackStateRequirements => {
            MessageTemplate::new(NOTE_CALLBACK_STATE_REQUIREMENTS)
        }
        DiagnosticNoteKind::RefutablePatternRequiresConditionalContext => {
            MessageTemplate::new(NOTE_REFUTABLE_PATTERN_REQUIRES_CONDITIONAL_CONTEXT)
        }
        DiagnosticNoteKind::InterfaceDependencyContext => {
            MessageTemplate::new(NOTE_INTERFACE_DEPENDENCY_CONTEXT)
        }
        DiagnosticNoteKind::LinkPlanContext => MessageTemplate::new(NOTE_LINK_PLAN_CONTEXT),
        DiagnosticNoteKind::ExternalToolExitRequiresCorrection => {
            MessageTemplate::new(NOTE_EXTERNAL_TOOL_EXIT_REQUIRES_CORRECTION)
        }
        DiagnosticNoteKind::RuntimeArtifactMustBeUsable => {
            MessageTemplate::new(NOTE_RUNTIME_ARTIFACT_MUST_BE_USABLE)
        }
        DiagnosticNoteKind::AwaitDependencyUnavailable => {
            MessageTemplate::new(NOTE_AWAIT_DEPENDENCY_UNAVAILABLE)
        }
        DiagnosticNoteKind::AsynchronousCallableRequired => {
            MessageTemplate::new(NOTE_ASYNCHRONOUS_CALLABLE_REQUIRED)
        }
        DiagnosticNoteKind::ExecutableEntrypointRequired => {
            MessageTemplate::new(NOTE_EXECUTABLE_ENTRYPOINT_REQUIRED)
        }
        DiagnosticNoteKind::EntrypointDirectiveRequiresExecutableProduct => {
            MessageTemplate::new(NOTE_ENTRYPOINT_DIRECTIVE_REQUIRES_EXECUTABLE_PRODUCT)
        }
        DiagnosticNoteKind::ProductEntryRequirements => {
            MessageTemplate::new(NOTE_PRODUCT_ENTRY_REQUIREMENTS)
        }
        DiagnosticNoteKind::TestDirectiveRequirements => {
            MessageTemplate::new(NOTE_TEST_DIRECTIVE_REQUIREMENTS)
        }
        DiagnosticNoteKind::UniqueTestIdentityRequired => {
            MessageTemplate::new(NOTE_UNIQUE_TEST_IDENTITY_REQUIRED)
        }
        DiagnosticNoteKind::PublicDependencyRequired => {
            MessageTemplate::new(NOTE_PUBLIC_DEPENDENCY_REQUIRED)
        }
        DiagnosticNoteKind::ReportCompilerDefect => {
            MessageTemplate::new(NOTE_REPORT_COMPILER_DEFECT)
        }
    }
}

pub(crate) const fn label_heading() -> &'static str {
    "label"
}

pub(crate) const fn related_location_heading() -> &'static str {
    "related"
}
