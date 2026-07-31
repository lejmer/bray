// rust-style: allow(module-too-large, reason = "the English diagnostic catalog is intentionally one exhaustive flat message mapping")

use bray_diagnostics::{DiagnosticArgName, DiagnosticKind, DiagnosticNoteKind, SeverityKind};

use crate::catalog::{MessageTemplate, MessageTemplatePart};
use crate::rendered_diagnostic::RenderedDiagnosticNoteKind;

use super::interface::diagnostic_template as interface_diagnostic_template;

const SOURCE_FILE_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read source file "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const SOURCE_INVALID_UTF8: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "source input contains invalid UTF-8",
)];

const SOURCE_TOO_MANY_INPUTS: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("too many source inputs: "),
    MessageTemplatePart::Arg(DiagnosticArgName::SourceCount),
];

const SOURCE_TEXT_TOO_LARGE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("source text is too large: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ByteCount),
    MessageTemplatePart::Text(" bytes"),
];

const INSPECTION_REPORT_WRITE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not write inspection report to "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const PROJECT_MANIFEST_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read Bray project manifest "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const PROJECT_MANIFEST_PARSE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("Bray project manifest does not match the required schema: "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_MANIFEST_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid Bray project manifest selection "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_MANIFEST_DUPLICATE_SELECTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("duplicate Bray project manifest selection "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_SOURCE_ROOT_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid project-owned source root "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" selected by "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_PACKAGE_UNKNOWN: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is absent from the explicit workspace inventory in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_PRODUCT_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("dependency product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" is not a declared library product in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_DEPENDENCY_CYCLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("package dependency cycle includes "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
    MessageTemplatePart::Text(" in "),
    MessageTemplatePart::Arg(DiagnosticArgName::FilePath),
];

const PROJECT_COMMAND_SELECTION_INVALID: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("project command selection is not present in the explicit graph: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const PROJECT_COMMAND_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("project command operation failed: "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

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

const REQUEST_MISSING_SOURCE_INPUT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("no source inputs were provided")];

const REQUEST_INVALID_SOURCE_INPUT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("source input is invalid")];

const REQUEST_DUPLICATE_SOURCE_INPUT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("source input "),
    MessageTemplatePart::Arg(DiagnosticArgName::InputIndex),
    MessageTemplatePart::Text(" selects a source that is already present"),
];

const REQUEST_INVALID_WORKER_BUDGET: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "worker budget must be greater than zero",
)];

const REQUEST_UNSUPPORTED_PRODUCT_EMISSION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "the selected product configuration cannot form a complete emission request",
)];

const SYNTAX_NESTING_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("syntax nesting exceeds the maximum depth of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_NO_APPLICABLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("no applicable "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate"),
];

const CHECKING_AMBIGUOUS_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" selection is ambiguous"),
];

const CHECKING_INACCESSIBLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("matching "),
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate is inaccessible"),
];

const CHECKING_INCOMPATIBLE_CANDIDATE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::SelectionKind),
    MessageTemplatePart::Text(" candidate is incompatible with the supplied expressions"),
];

const CHECKING_TARGET_REPRESENTATION_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected target does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetRepresentation),
    MessageTemplatePart::Text(" representation"),
];

const CHECKING_TARGET_CALLABLE_ABI_UNAVAILABLE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("selected target does not provide the required "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" callable ABI"),
];

const CHECKING_TARGET_ALIGNMENT_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("required "),
    MessageTemplatePart::Arg(DiagnosticArgName::AlignmentKind),
    MessageTemplatePart::Text(" alignment "),
    MessageTemplatePart::Arg(DiagnosticArgName::RequiredAlignment),
    MessageTemplatePart::Text(" exceeds the selected target maximum of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumAlignment),
];

const CHECKING_TARGET_ABI_REPRESENTATION_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("the selected "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" callable ABI does not accept "),
    MessageTemplatePart::Arg(DiagnosticArgName::TargetRepresentation),
    MessageTemplatePart::Text(" values by value"),
];

const CHECKING_TARGET_MEMORY_OPERATION_UNAVAILABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "selected target does not provide this compiler-provided memory operation",
    )];

const BINDING_INVALID_CALLABLE_ABI: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid callable ABI directive")];

const BINDING_DUPLICATE_CALLABLE_ABI: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "callable ABI directive is repeated",
)];

const BINDING_PREDICATE_BODY_REQUIRED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "ordinary predicate declaration requires a body",
)];

const BINDING_TRUSTED_PREDICATE_BODY_NOT_ALLOWED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "trusted predicate declaration cannot have a body",
    )];

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

const CHECKING_INVALID_STORED_TYPE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "stored type does not have a finite outer representation",
)];

const CHECKING_RECURSIVE_TYPE_REPRESENTATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "declared type has an inline recursive representation",
    )];

const CHECKING_TYPE_REPRESENTATION_RECURSION_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("type representation analysis exceeded its recursion limit of "),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_INVALID_LAYOUT_DIRECTIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid type layout contract")];

const CHECKING_INVALID_COPY_CONTRACT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "type does not satisfy its copy contract",
)];

const CHECKING_INVALID_UNION_TAG: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid union tag contract")];

const CHECKING_REFINEMENT_CAPACITY_EXCEEDED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "program requires too many flow-sensitive facts",
)];

const CHECKING_USE_OF_UNINITIALIZED_STORAGE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "storage is used before it is initialized",
)];

const CHECKING_USE_OF_MOVED_STORAGE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "storage is used after its value was moved",
)];

const CHECKING_CONFLICTING_BORROW: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "operation conflicts with an active borrow",
)];

const CHECKING_MISSING_MUTATION_AUTHORITY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "operation requires mutable access to storage",
)];

const CHECKING_TYPE_IS_NOT_COPYABLE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "value type does not support implicit copying",
)];

const CHECKING_MISSING_STORAGE_OWNERSHIP: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "operation requires ownership of the reached storage",
)];

const CHECKING_INACTIVE_STORAGE_PROJECTION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "selected storage is not active on this control-flow path",
)];

const CHECKING_MISSING_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait implementation does not fulfill "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_EXTRA_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait has no member named "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_INCOMPATIBLE_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait fulfillment is incompatible with "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_DUPLICATE_TRAIT_FULFILLMENT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("trait member is fulfilled more than once: "),
    MessageTemplatePart::Arg(DiagnosticArgName::TraitMemberName),
];

const CHECKING_OVERLAPPING_IMPLEMENTATION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "implementation overlaps another participating implementation",
)];

const CHECKING_UNGROUPED_IMPLEMENTATION_OVERLOADS: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "implementations sharing a subject and trait must belong to one overload family",
    )];

const CHECKING_INVALID_IMPLEMENTATION_OVERLOAD_HEADER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "implementation overload header must name a supported subject and trait",
    )];

const CHECKING_INVALID_IMPLEMENTATION_OVERLOAD_ARM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "implementation overload arm is duplicated or incompatible with its family",
    )];

const CHECKING_INVALID_CALLABLE_OVERLOAD_ARM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "callable overload arm must name an accessible callable with a compatible call context",
    )];

const CHECKING_DUPLICATE_CALLABLE_OVERLOAD_ARM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "callable declaration occurs more than once in this overload family",
    )];

const CHECKING_CONFLICTING_CALLABLE_OVERLOAD_FAMILY: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "callable declaration cannot belong to more than one overload family",
    )];

const CHECKING_CONFLICTING_CALLABLE_OVERLOAD_SIGNATURE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "callable overload arms have indistinguishable selection signatures",
    )];

const CHECKING_IMPLEMENTATION_COHERENCE_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "implementation coherence analysis exceeded its pairwise comparison limit of ",
    ),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_CALLABLE_OVERLOAD_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text(
        "callable overload analysis exceeded its pairwise comparison limit of ",
    ),
    MessageTemplatePart::Arg(DiagnosticArgName::MaximumCount),
];

const CHECKING_MISSING_FOREIGN_CALLABLE_DIRECTIVE: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("foreign callable requires "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
];

const CHECKING_FOREIGN_CALLABLE_REQUIRES_TRUSTED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "foreign callable must establish a trusted boundary",
    )];

const CHECKING_FOREIGN_CALLABLE_REQUIRES_CAPABILITY: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("foreign callable requires capability "),
    MessageTemplatePart::Arg(DiagnosticArgName::ReferencedName),
];

const CHECKING_FOREIGN_CALLABLE_EXECUTION_UNSUPPORTED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "asynchronous callable cannot cross a foreign ABI boundary",
    )];

const CHECKING_FOREIGN_ABI_TYPE_UNSUPPORTED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ActualType),
    MessageTemplatePart::Text(" cannot cross the "),
    MessageTemplatePart::Arg(DiagnosticArgName::CallableAbi),
    MessageTemplatePart::Text(" ABI boundary"),
];

const CHECKING_INVALID_NATIVE_LINK_DIRECTIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "link directive must provide a non-empty constant string name and a supported link kind",
    )];

const CHECKING_INVALID_NATIVE_SYMBOL_DIRECTIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "symbol directive must provide a non-empty constant string name",
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
    "await is only permitted in an asynchronous callable body",
)];

const CHECKING_TASK_START_OUTSIDE_ASYNC_CALLABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "a future can only be started from an asynchronous callable body",
    )];

const CHECKING_UNAVAILABLE_AWAIT_DEPENDENCY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "awaited computation requires semantic state that is not available here",
)];

const CHECKING_ENTRYPOINT_NOT_ALLOWED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "entrypoint directive is not allowed for this product",
)];

const CHECKING_MISSING_ENTRYPOINT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "executable product requires an entry point",
)];

const CHECKING_DUPLICATE_ENTRYPOINT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "executable product has more than one entry point",
)];

const CHECKING_ENTRY_CANNOT_BE_GENERIC: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "product entry function cannot declare generic parameters",
)];

const CHECKING_ENTRY_CANNOT_TAKE_PARAMETERS: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "product entry function cannot require caller-supplied parameters",
)];

const CHECKING_INVALID_ENTRYPOINT_RESULT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "entry point result must be unit, Result<unit, E>, or i32",
)];

const CHECKING_INVALID_TEST_RESULT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "test entry result must be unit or Result<unit, E>",
)];

const CHECKING_ENTRY_CANNOT_BE_CONSTANT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "product entry function cannot be constant",
)];

const CHECKING_ENTRY_CANNOT_REQUIRE_TRUST: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "product entry function cannot expose trusted caller obligations",
)];

const CHECKING_EXPORT_DEPENDS_ON_INTERNAL_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "public signature exposes an internal declaration",
    )];

const EMISSION_MISSING_CONTRIBUTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("missing required "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact contribution"),
];

const EMISSION_INVALID_CONTRIBUTION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact contribution does not match the emission plan"),
];

const EMISSION_ARTIFACT_READ_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not read "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact content: "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_OPEN_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not open artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_WRITE_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not write artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_FLUSH_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not flush artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_COMMIT_FAILED: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("could not commit artifact output "),
    MessageTemplatePart::Arg(DiagnosticArgName::OutputSink),
    MessageTemplatePart::Text(": "),
    MessageTemplatePart::Arg(DiagnosticArgName::IoErrorKind),
];

const EMISSION_ARTIFACT_DIGEST_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact declared digest "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedArtifactDigest),
    MessageTemplatePart::Text(", but content digest was "),
    MessageTemplatePart::Arg(DiagnosticArgName::ActualArtifactDigest),
];

const EMISSION_ARTIFACT_LENGTH_MISMATCH: &[MessageTemplatePart] = &[
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactKind),
    MessageTemplatePart::Text(" artifact declared "),
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

const CHECKING_REFUTABLE_PATTERN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("pattern must be irrefutable")];

const CHECKING_NON_EXHAUSTIVE_MATCH: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "match does not cover every possible value",
)];

const CHECKING_UNREACHABLE_MATCH_ARM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("match arm is unreachable")];

const CHECKING_UNREACHABLE_PATTERN_ALTERNATIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "pattern alternative is unreachable",
    )];

const CHECKING_CANNOT_INFER_EXPRESSION_TYPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("cannot infer expression type")];

const CHECKING_NO_COMPATIBLE_PROPAGATION_BOUNDARY: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "no compatible propagation boundary is available",
    )];

const CHECKING_INVALID_CONSTANT_EXPRESSION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "expression is not valid in compile-time constant context",
)];

const CHECKING_ARRAY_LENGTH_NOT_POSITIVE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "array length must be greater than zero",
)];

const CHECKING_ARRAY_GENERATOR_CARDINALITY_NOT_PROVABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "array generator element count cannot be proven",
    )];

const CHECKING_CONSTANT_LITERAL_NOT_REPRESENTABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "literal value cannot be represented by its selected type",
    )];

const CHECKING_CONSTANT_EVALUATION_STEP_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its operation limit",
    )];

const CHECKING_CONSTANT_AGGREGATE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its aggregate element limit",
    )];

const CHECKING_CONSTANT_EXPANSION_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its expansion limit",
    )];

const CHECKING_CONSTANT_LITERAL_SIZE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its literal size limit",
    )];

const CHECKING_CONSTANT_INTEGER_SIZE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant evaluation exceeded its exact integer size limit",
    )];

const CHECKING_CYCLIC_CONSTANT_DEFINITION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "constant definition depends on itself through a cycle",
)];

const CHECKING_CONSTANT_DIVISION_BY_ZERO: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "constant operation divides by zero",
)];

const CHECKING_CONSTANT_VALUE_NOT_REPRESENTABLE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "constant result cannot be represented by its selected type",
    )];

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

const NOTE_INTERFACE_DEPENDENCY_CONTEXT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("while loading package "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedPackageIdentity),
    MessageTemplatePart::Text(" product "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedProductIdentity),
    MessageTemplatePart::Text(" from "),
    MessageTemplatePart::Arg(DiagnosticArgName::ArtifactPath),
];

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
        | DiagnosticNoteKind::InterfaceDependencyContext => RenderedDiagnosticNoteKind::Note,
        DiagnosticNoteKind::SourceFileMustBeReadable
        | DiagnosticNoteKind::SourceMustBeUtf8
        | DiagnosticNoteKind::SourceInputRequired
        | DiagnosticNoteKind::SourceInputNeedsStableIdentity
        | DiagnosticNoteKind::WorkerBudgetMustBePositive
        | DiagnosticNoteKind::CharacterNotAccepted
        | DiagnosticNoteKind::LineBreaksMustBeLfOrCrlf
        | DiagnosticNoteKind::IdentifierSpellingMustBeValid
        | DiagnosticNoteKind::CharacterLiteralNeedsTerminator
        | DiagnosticNoteKind::StringLiteralNeedsTerminator
        | DiagnosticNoteKind::EscapeMustBeKnown
        | DiagnosticNoteKind::BlockCommentNeedsTerminator => RenderedDiagnosticNoteKind::Help,
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
        DiagnosticKind::ProjectManifestReadFailed => {
            MessageTemplate::new(PROJECT_MANIFEST_READ_FAILED)
        }
        DiagnosticKind::ProjectManifestParseFailed => {
            MessageTemplate::new(PROJECT_MANIFEST_PARSE_FAILED)
        }
        DiagnosticKind::ProjectManifestInvalid => MessageTemplate::new(PROJECT_MANIFEST_INVALID),
        DiagnosticKind::ProjectManifestDuplicateSelection => {
            MessageTemplate::new(PROJECT_MANIFEST_DUPLICATE_SELECTION)
        }
        DiagnosticKind::ProjectSourceRootInvalid => {
            MessageTemplate::new(PROJECT_SOURCE_ROOT_INVALID)
        }
        DiagnosticKind::ProjectDependencyPackageUnknown => {
            MessageTemplate::new(PROJECT_DEPENDENCY_PACKAGE_UNKNOWN)
        }
        DiagnosticKind::ProjectDependencyProductInvalid => {
            MessageTemplate::new(PROJECT_DEPENDENCY_PRODUCT_INVALID)
        }
        DiagnosticKind::ProjectDependencyCycle => MessageTemplate::new(PROJECT_DEPENDENCY_CYCLE),
        DiagnosticKind::ProjectCommandSelectionInvalid => {
            MessageTemplate::new(PROJECT_COMMAND_SELECTION_INVALID)
        }
        DiagnosticKind::ProjectCommandFailed => MessageTemplate::new(PROJECT_COMMAND_FAILED),
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
        DiagnosticKind::CheckingNoCompatiblePropagationBoundary => {
            MessageTemplate::new(CHECKING_NO_COMPATIBLE_PROPAGATION_BOUNDARY)
        }
        DiagnosticKind::CheckingInvalidConstantExpression => {
            MessageTemplate::new(CHECKING_INVALID_CONSTANT_EXPRESSION)
        }
        DiagnosticKind::CheckingArrayLengthNotPositive => {
            MessageTemplate::new(CHECKING_ARRAY_LENGTH_NOT_POSITIVE)
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
        DiagnosticKind::CheckingUseOfUninitializedStorage => {
            MessageTemplate::new(CHECKING_USE_OF_UNINITIALIZED_STORAGE)
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
        DiagnosticKind::CheckingTypeIsNotCopyable => {
            MessageTemplate::new(CHECKING_TYPE_IS_NOT_COPYABLE)
        }
        DiagnosticKind::CheckingMissingStorageOwnership => {
            MessageTemplate::new(CHECKING_MISSING_STORAGE_OWNERSHIP)
        }
        DiagnosticKind::CheckingInactiveStorageProjection => {
            MessageTemplate::new(CHECKING_INACTIVE_STORAGE_PROJECTION)
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
        DiagnosticKind::CheckingAmbiguousCandidate => {
            MessageTemplate::new(CHECKING_AMBIGUOUS_CANDIDATE)
        }
        DiagnosticKind::CheckingInaccessibleCandidate => {
            MessageTemplate::new(CHECKING_INACCESSIBLE_CANDIDATE)
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
        DiagnosticKind::BindingInvalidCallableAbi => {
            MessageTemplate::new(BINDING_INVALID_CALLABLE_ABI)
        }
        DiagnosticKind::BindingDuplicateCallableAbi => {
            MessageTemplate::new(BINDING_DUPLICATE_CALLABLE_ABI)
        }
        DiagnosticKind::BindingPredicateBodyRequired => {
            MessageTemplate::new(BINDING_PREDICATE_BODY_REQUIRED)
        }
        DiagnosticKind::BindingTrustedPredicateBodyNotAllowed => {
            MessageTemplate::new(BINDING_TRUSTED_PREDICATE_BODY_NOT_ALLOWED)
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
        | DiagnosticKind::InterfaceDependencyGraphInvalid
        | DiagnosticKind::InterfaceSemanticFactsInvalid
        | DiagnosticKind::InterfaceConstantCallableBodyUnavailable => {
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
        DiagnosticNoteKind::InterfaceDependencyContext => {
            MessageTemplate::new(NOTE_INTERFACE_DEPENDENCY_CONTEXT)
        }
    }
}
