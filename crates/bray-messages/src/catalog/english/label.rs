use bray_diagnostics::{DiagnosticArgName, DiagnosticLabelKind, DiagnosticLabelStyle};

use crate::catalog::{MessageTemplate, MessageTemplatePart};

const INVALID_CHARACTER: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("invalid character "),
    MessageTemplatePart::Arg(DiagnosticArgName::Character),
];

const MISPLACED_BOM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("misplaced byte order mark")];

const LONE_CARRIAGE_RETURN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("lone carriage return")];

const NON_ASCII_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("non-ASCII identifier")];

const INVALID_IDENTIFIER: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid identifier")];

const INVALID_OPERATOR_OR_PUNCTUATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid operator or punctuation")];

const MALFORMED_LITERAL: &[MessageTemplatePart] = &[MessageTemplatePart::Text("malformed literal")];

const INVALID_NUMERIC_SUFFIX: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("suffix starts here")];

const UNTERMINATED_CHARACTER_LITERAL_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("character literal starts here")];

const UNTERMINATED_STRING_LITERAL_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("string literal starts here")];

const UNKNOWN_ESCAPE: &[MessageTemplatePart] = &[MessageTemplatePart::Text("unknown escape")];

const INVALID_UNICODE_ESCAPE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid Unicode escape")];

const UNTERMINATED_BLOCK_COMMENT_START: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("block comment starts here")];

const EXPECTED_TOKEN_INSERTION_POINT: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("insert "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" here"),
];

const EXPECTED_EXPRESSION: &[MessageTemplatePart] = &[
    MessageTemplatePart::Text("expected "),
    MessageTemplatePart::Arg(DiagnosticArgName::ExpectedSyntaxKind),
    MessageTemplatePart::Text(" here"),
];

const NESTING_LIMIT_EXCEEDED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "syntax nesting exceeds the limit here",
)];

const UNEXPECTED_EOF: &[MessageTemplatePart] = &[MessageTemplatePart::Text("source ends here")];

const NAME_REFERENCE: &[MessageTemplatePart] = &[MessageTemplatePart::Text("name reference")];

const NAME_DEFINITION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("conflicting name definition")];

const ALTERNATIVE_PATTERN: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("incoherent alternative pattern")];
const PATTERN_FAILURE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("pattern rejected here")];
const MATCH_COVERAGE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("match coverage checked here")];

const CALLABLE_ABI_DIRECTIVE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("callable ABI directive")];

const DIRECTIVE_ARGUMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("malformed directive argument")];

const MODULE_EXPORT: &[MessageTemplatePart] = &[MessageTemplatePart::Text("module export")];

const INVALID_RANGE_ELEMENT_TYPE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this range uses a non-integer element type",
)];

const INCOMPATIBLE_EXPRESSION_TYPE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this expression has the incompatible type",
)];
const SELECTION_FAILURE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("selection fails here")];
const TRAIT_FULFILLMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("trait fulfillment checked here")];

const UNCONSTRAINED_EXPRESSION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this expression has no determined type",
)];

const INVALID_CONSTANT_EXPRESSION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "compile-time evaluation fails here",
)];

const INVALID_TYPE_REPRESENTATION_CONTRACT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "representation contract fails here",
)];

const INCOMPATIBLE_PROPAGATION_BOUNDARY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "propagation has no compatible boundary",
)];

const UNPROVEN_ARRAY_GENERATOR_CARDINALITY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "fixed-array length is not provable here",
)];

const UNSUPPORTED_TARGET_REQUIREMENT: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this operation requires an unavailable target capability",
)];

const REFINEMENT_CAPACITY_EXCEEDED: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this unit exceeds the configured flow-sensitive analysis capacity",
)];
const MEMORY_OPERATION_FAILURE: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("memory operation fails here")];

const DUPLICATE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("duplicate declaration")];

const INVALID_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("invalid declaration form")];

const INVALID_PRODUCT_ENTRY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this callable does not satisfy the selected product entry contract",
)];

const INVALID_FOREIGN_BOUNDARY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this declaration does not satisfy the foreign boundary contract",
)];

const OVERLAPPING_IMPLEMENTATION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this implementation overlaps another applicable implementation",
)];

const IMPLEMENTATION_COHERENCE_LIMIT_EXCEEDED: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "coherence comparison reached the configured limit here",
    )];

const UNGROUPED_IMPLEMENTATION_OVERLOAD: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this implementation requires an explicit overload family",
)];

const INVALID_OVERLOAD: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this overload declaration is invalid",
)];

const DUPLICATE_OVERLOAD_ARM: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("this overload arm is repeated")];

const CONFLICTING_OVERLOAD: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this overload declaration conflicts with another declaration",
)];

const INVALID_PRODUCT_CONFIGURATION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this declaration is invalid for the selected product",
)];

const DUPLICATE_MODULE_CONTRIBUTION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this module contribution directive is repeated",
)];

const MOVED_STORAGE_USE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "storage is used after being moved",
)];
const CONFLICTING_BORROW_OPERATION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this operation conflicts with a live borrow",
)];
const MISSING_MUTATION_AUTHORITY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this operation requires mutation authority",
)];
const MISSING_STORAGE_OWNERSHIP: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this operation requires storage ownership",
)];
const ESCAPING_STORAGE_DEPENDENCY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this value carries a dependency beyond the referenced storage's scope",
)];
const DEPENDENCY_SELECTION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this dependency selected the failing artifact",
)];
const UNAVAILABLE_AWAIT_DEPENDENCY: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "the required state is unavailable at this await",
)];
const INVALID_ASYNC_OPERATION: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "this operation requires an asynchronous callable",
)];
const INVALID_TRUSTED_CAPABILITY_REQUIREMENT: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text(
        "this trusted capability requirement is rejected",
    )];

const CONFLICTING_MODULE_DECLARATION: &[MessageTemplatePart] =
    &[MessageTemplatePart::Text("conflicting module declaration")];
const COMPILER_DEFECT_SOURCE: &[MessageTemplatePart] = &[MessageTemplatePart::Text(
    "the internal compiler error occurred while compiling this source construct",
)];

pub(crate) const fn style(style: DiagnosticLabelStyle) -> &'static str {
    match style {
        DiagnosticLabelStyle::Primary => "primary source",
        DiagnosticLabelStyle::Secondary => "secondary source",
    }
}

pub(crate) const fn template(kind: DiagnosticLabelKind) -> MessageTemplate {
    match kind {
        DiagnosticLabelKind::InvalidCharacter => MessageTemplate::new(INVALID_CHARACTER),
        DiagnosticLabelKind::MisplacedBom => MessageTemplate::new(MISPLACED_BOM),
        DiagnosticLabelKind::LoneCarriageReturn => MessageTemplate::new(LONE_CARRIAGE_RETURN),
        DiagnosticLabelKind::NonAsciiIdentifier => MessageTemplate::new(NON_ASCII_IDENTIFIER),
        DiagnosticLabelKind::InvalidIdentifier => MessageTemplate::new(INVALID_IDENTIFIER),
        DiagnosticLabelKind::InvalidOperatorOrPunctuation => {
            MessageTemplate::new(INVALID_OPERATOR_OR_PUNCTUATION)
        }
        DiagnosticLabelKind::MalformedLiteral => MessageTemplate::new(MALFORMED_LITERAL),
        DiagnosticLabelKind::InvalidNumericSuffix => MessageTemplate::new(INVALID_NUMERIC_SUFFIX),
        DiagnosticLabelKind::UnterminatedCharacterLiteralStart => {
            MessageTemplate::new(UNTERMINATED_CHARACTER_LITERAL_START)
        }
        DiagnosticLabelKind::UnterminatedStringLiteralStart => {
            MessageTemplate::new(UNTERMINATED_STRING_LITERAL_START)
        }
        DiagnosticLabelKind::UnknownEscape => MessageTemplate::new(UNKNOWN_ESCAPE),
        DiagnosticLabelKind::InvalidUnicodeEscape => MessageTemplate::new(INVALID_UNICODE_ESCAPE),
        DiagnosticLabelKind::UnterminatedBlockCommentStart => {
            MessageTemplate::new(UNTERMINATED_BLOCK_COMMENT_START)
        }
        DiagnosticLabelKind::ExpectedTokenInsertionPoint => {
            MessageTemplate::new(EXPECTED_TOKEN_INSERTION_POINT)
        }
        DiagnosticLabelKind::ExpectedExpression => MessageTemplate::new(EXPECTED_EXPRESSION),
        DiagnosticLabelKind::NestingLimitExceeded => MessageTemplate::new(NESTING_LIMIT_EXCEEDED),
        DiagnosticLabelKind::UnexpectedEof => MessageTemplate::new(UNEXPECTED_EOF),
        DiagnosticLabelKind::NameReference => MessageTemplate::new(NAME_REFERENCE),
        DiagnosticLabelKind::NameDefinition => MessageTemplate::new(NAME_DEFINITION),
        DiagnosticLabelKind::AlternativePattern => MessageTemplate::new(ALTERNATIVE_PATTERN),
        DiagnosticLabelKind::PatternFailure => MessageTemplate::new(PATTERN_FAILURE),
        DiagnosticLabelKind::MatchCoverage => MessageTemplate::new(MATCH_COVERAGE),
        DiagnosticLabelKind::CallableAbiDirective => MessageTemplate::new(CALLABLE_ABI_DIRECTIVE),
        DiagnosticLabelKind::DirectiveArgument => MessageTemplate::new(DIRECTIVE_ARGUMENT),
        DiagnosticLabelKind::ModuleExport => MessageTemplate::new(MODULE_EXPORT),
        DiagnosticLabelKind::IncompatibleExpressionType => {
            MessageTemplate::new(INCOMPATIBLE_EXPRESSION_TYPE)
        }
        DiagnosticLabelKind::SelectionFailure => MessageTemplate::new(SELECTION_FAILURE),
        DiagnosticLabelKind::TraitFulfillment => MessageTemplate::new(TRAIT_FULFILLMENT),
        DiagnosticLabelKind::UnconstrainedExpression => {
            MessageTemplate::new(UNCONSTRAINED_EXPRESSION)
        }
        DiagnosticLabelKind::InvalidRangeElementType => {
            MessageTemplate::new(INVALID_RANGE_ELEMENT_TYPE)
        }
        DiagnosticLabelKind::InvalidConstantExpression => {
            MessageTemplate::new(INVALID_CONSTANT_EXPRESSION)
        }
        DiagnosticLabelKind::InvalidTypeRepresentationContract => {
            MessageTemplate::new(INVALID_TYPE_REPRESENTATION_CONTRACT)
        }
        DiagnosticLabelKind::IncompatiblePropagationBoundary => {
            MessageTemplate::new(INCOMPATIBLE_PROPAGATION_BOUNDARY)
        }
        DiagnosticLabelKind::UnprovenArrayGeneratorCardinality => {
            MessageTemplate::new(UNPROVEN_ARRAY_GENERATOR_CARDINALITY)
        }
        DiagnosticLabelKind::UnsupportedTargetRequirement => {
            MessageTemplate::new(UNSUPPORTED_TARGET_REQUIREMENT)
        }
        DiagnosticLabelKind::RefinementCapacityExceeded => {
            MessageTemplate::new(REFINEMENT_CAPACITY_EXCEEDED)
        }
        DiagnosticLabelKind::MemoryOperationFailure => {
            MessageTemplate::new(MEMORY_OPERATION_FAILURE)
        }
        DiagnosticLabelKind::DuplicateDeclaration => MessageTemplate::new(DUPLICATE_DECLARATION),
        DiagnosticLabelKind::InvalidDeclaration => MessageTemplate::new(INVALID_DECLARATION),
        DiagnosticLabelKind::InvalidProductEntry => MessageTemplate::new(INVALID_PRODUCT_ENTRY),
        DiagnosticLabelKind::InvalidForeignBoundary => {
            MessageTemplate::new(INVALID_FOREIGN_BOUNDARY)
        }
        DiagnosticLabelKind::OverlappingImplementation => {
            MessageTemplate::new(OVERLAPPING_IMPLEMENTATION)
        }
        DiagnosticLabelKind::ImplementationCoherenceLimitExceeded => {
            MessageTemplate::new(IMPLEMENTATION_COHERENCE_LIMIT_EXCEEDED)
        }
        DiagnosticLabelKind::UngroupedImplementationOverload => {
            MessageTemplate::new(UNGROUPED_IMPLEMENTATION_OVERLOAD)
        }
        DiagnosticLabelKind::InvalidOverload => MessageTemplate::new(INVALID_OVERLOAD),
        DiagnosticLabelKind::DuplicateOverloadArm => MessageTemplate::new(DUPLICATE_OVERLOAD_ARM),
        DiagnosticLabelKind::ConflictingOverload => MessageTemplate::new(CONFLICTING_OVERLOAD),
        DiagnosticLabelKind::InvalidProductConfiguration => {
            MessageTemplate::new(INVALID_PRODUCT_CONFIGURATION)
        }
        DiagnosticLabelKind::DuplicateModuleContribution => {
            MessageTemplate::new(DUPLICATE_MODULE_CONTRIBUTION)
        }
        DiagnosticLabelKind::MovedStorageUse => MessageTemplate::new(MOVED_STORAGE_USE),
        DiagnosticLabelKind::ConflictingBorrowOperation => {
            MessageTemplate::new(CONFLICTING_BORROW_OPERATION)
        }
        DiagnosticLabelKind::MissingMutationAuthority => {
            MessageTemplate::new(MISSING_MUTATION_AUTHORITY)
        }
        DiagnosticLabelKind::MissingStorageOwnership => {
            MessageTemplate::new(MISSING_STORAGE_OWNERSHIP)
        }
        DiagnosticLabelKind::EscapingStorageDependency => {
            MessageTemplate::new(ESCAPING_STORAGE_DEPENDENCY)
        }
        DiagnosticLabelKind::DependencySelection => MessageTemplate::new(DEPENDENCY_SELECTION),
        DiagnosticLabelKind::UnavailableAwaitDependency => {
            MessageTemplate::new(UNAVAILABLE_AWAIT_DEPENDENCY)
        }
        DiagnosticLabelKind::InvalidAsyncOperation => MessageTemplate::new(INVALID_ASYNC_OPERATION),
        DiagnosticLabelKind::InvalidTrustedCapabilityRequirement => {
            MessageTemplate::new(INVALID_TRUSTED_CAPABILITY_REQUIREMENT)
        }
        DiagnosticLabelKind::ConflictingModuleDeclaration => {
            MessageTemplate::new(CONFLICTING_MODULE_DECLARATION)
        }
        DiagnosticLabelKind::CompilerDefectSource => MessageTemplate::new(COMPILER_DEFECT_SOURCE),
    }
}
