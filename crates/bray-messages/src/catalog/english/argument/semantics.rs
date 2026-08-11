use super::source::{format_english_quoted_text, format_english_type};
use bray_diagnostics::{
    DiagnosticArrayGeneratorCardinalityProblem, DiagnosticArrayLength,
    DiagnosticCallbackStateProblem, DiagnosticConstantOperation, DiagnosticCopyContractProblem,
    DiagnosticExpressionCategory, DiagnosticLayoutOption, DiagnosticLayoutProblem,
    DiagnosticMemoryOperation, DiagnosticPatternCoverage, DiagnosticPatternMissingCase,
    DiagnosticPatternUnreachability, DiagnosticPropagationProblem, DiagnosticRefinementCapacity,
    DiagnosticRefinementCapacitySurface, DiagnosticStorageAccess, DiagnosticStorageAccessPurpose,
    DiagnosticStorageProjection, DiagnosticStorageRoot, DiagnosticStoredTypeProblem,
    DiagnosticUnionTagProblem, DiagnosticYieldCardinality,
};
use std::fmt::Write as _;

pub(super) fn format_english_pattern_coverage(coverage: &DiagnosticPatternCoverage) -> String {
    let mut missing = coverage
        .missing()
        .iter()
        .map(format_english_pattern_missing_case)
        .collect::<Vec<_>>();

    if coverage.omitted_count() != 0 {
        missing.push(format!("{} additional cases", coverage.omitted_count()));
    }

    format!(
        "{} is missing {}",
        format_english_type(coverage.subject_type()),
        missing.join(", ")
    )
}

fn format_english_pattern_missing_case(case: &DiagnosticPatternMissingCase) -> String {
    match case {
        DiagnosticPatternMissingCase::NullableAbsent => "the absent case".to_owned(),
        DiagnosticPatternMissingCase::NullablePresent => "a present value".to_owned(),
        DiagnosticPatternMissingCase::Boolean(value) => value.to_string(),
        DiagnosticPatternMissingCase::UnionVariant(name) => {
            format!("variant {}", format_english_quoted_text(name))
        }
        DiagnosticPatternMissingCase::RemainingValues => {
            "a catch-all for remaining values".to_owned()
        }
    }
}

pub(super) const fn format_english_pattern_unreachability(
    reason: DiagnosticPatternUnreachability,
) -> &'static str {
    match reason {
        DiagnosticPatternUnreachability::CoveredByEarlierPattern => {
            "an earlier pattern already covers every matching value"
        }
        DiagnosticPatternUnreachability::GuardAlwaysFalse => "its guard is always false",
    }
}

const fn format_english_layout_option(option: DiagnosticLayoutOption) -> &'static str {
    match option {
        DiagnosticLayoutOption::Alignment => "alignment",
        DiagnosticLayoutOption::Packing => "packing",
        DiagnosticLayoutOption::Tag => "tag type",
    }
}

pub(super) fn format_english_layout_problem(problem: &DiagnosticLayoutProblem) -> String {
    match problem {
        DiagnosticLayoutProblem::MissingMode => String::from("the layout mode is missing"),
        DiagnosticLayoutProblem::UnsupportedMode(mode) => {
            format!("layout mode {mode:?} is not supported")
        }
        DiagnosticLayoutProblem::DuplicateOption(option) => format!(
            "the {} option is declared more than once",
            format_english_layout_option(*option)
        ),
        DiagnosticLayoutProblem::UnknownOption(option) => {
            format!("layout option {option:?} is not recognized")
        }
        DiagnosticLayoutProblem::UnexpectedPositionalArgument => {
            String::from("the layout directive has an extra positional argument")
        }
        DiagnosticLayoutProblem::OptionNotConstant(option) => format!(
            "the {} option is not a nonnegative integer constant",
            format_english_layout_option(*option)
        ),
        DiagnosticLayoutProblem::OptionNotPowerOfTwo { option, value } => format!(
            "the {} value {value} is not a power of two",
            format_english_layout_option(*option)
        ),
        DiagnosticLayoutProblem::TransparentUnion => {
            String::from("transparent layout cannot represent a union")
        }
        DiagnosticLayoutProblem::TransparentFieldCount { actual } => {
            format!("transparent layout requires exactly one field but the type has {actual}")
        }
        DiagnosticLayoutProblem::TransparentOption(option) => format!(
            "transparent layout cannot also specify {}",
            format_english_layout_option(*option)
        ),
        DiagnosticLayoutProblem::PackingRequiresStable => {
            String::from("packing is supported only by stable layout")
        }
        DiagnosticLayoutProblem::PackingRequiresPlainStorage => {
            String::from("packing cannot represent storage with lifecycle behavior")
        }
        DiagnosticLayoutProblem::TagRequiresUnion => {
            String::from("an explicit tag type is valid only for a union")
        }
        DiagnosticLayoutProblem::CUnionRequiresTag => {
            String::from("C-compatible union layout requires an explicit tag type")
        }
    }
}

pub(super) fn format_english_union_tag_problem(problem: &DiagnosticUnionTagProblem) -> String {
    match problem {
        DiagnosticUnionTagProblem::UnsupportedType(ty) => {
            format!("tag type {ty:?} is not a supported integer type")
        }
        DiagnosticUnionTagProblem::RequiresExplicitLayout => {
            String::from("an explicit variant tag requires an explicit union layout")
        }
        DiagnosticUnionTagProblem::ArgumentCount { actual } => {
            format!("a variant tag requires one value but {actual} were supplied")
        }
        DiagnosticUnionTagProblem::ValueNotConstant => {
            String::from("the variant tag is not an integer constant")
        }
        DiagnosticUnionTagProblem::DuplicateValue => {
            String::from("the variant tag value is already used by another variant")
        }
        DiagnosticUnionTagProblem::ValueOutsideSelectedType {
            signed,
            width_bits,
            value_negative,
            value_bits,
        } => {
            let bounds = format_integer_bounds(*signed, *width_bits);

            format!(
                "the {} variant tag requires {value_bits} magnitude bits, outside the selected {} {width_bits}-bit range {bounds}",
                if *value_negative {
                    "negative"
                } else {
                    "nonnegative"
                },
                if *signed { "signed" } else { "unsigned" },
            )
        }
        DiagnosticUnionTagProblem::PartialExplicitTags { explicit, total } => {
            format!("explicit tags are present on {explicit} of {total} union variants")
        }
        DiagnosticUnionTagProblem::NegativeInferredValue => {
            String::from("inferred union tags include a negative value")
        }
        DiagnosticUnionTagProblem::InferredValueTooWide {
            actual_bits,
            maximum_bits,
        } => format!(
            "inferred union tags require {actual_bits} bits but at most {maximum_bits} are supported"
        ),
    }
}

fn format_integer_bounds(signed: bool, width_bits: u16) -> String {
    let width = u32::from(width_bits);

    if signed {
        let magnitude_bits = width.saturating_sub(1);

        let maximum = 1_u128
            .checked_shl(magnitude_bits)
            .map_or(u128::MAX, |bound| bound.saturating_sub(1));

        let minimum_magnitude = 1_u128.checked_shl(magnitude_bits).unwrap_or(u128::MAX);

        return format!("-{minimum_magnitude} through {maximum}");
    }

    let maximum = 1_u128
        .checked_shl(width)
        .map_or(u128::MAX, |bound| bound.saturating_sub(1));

    format!("0 through {maximum}")
}

pub(super) fn format_english_copy_contract_problem(
    problem: DiagnosticCopyContractProblem,
) -> String {
    match problem {
        DiagnosticCopyContractProblem::LifecycleBehavior => {
            String::from("the type declares lifecycle behavior that prevents implicit copying")
        }
        DiagnosticCopyContractProblem::UnexpectedArguments { actual } => {
            format!("the copy directive does not accept arguments but {actual} were supplied")
        }
        DiagnosticCopyContractProblem::ConditionalMembersRequireGenericType => {
            String::from("conditionally copyable members require a generic containing type")
        }
        DiagnosticCopyContractProblem::NonCopyableMember => {
            String::from("a stored member does not provide a copy contract")
        }
    }
}

pub(super) fn format_english_stored_type_problem(problem: DiagnosticStoredTypeProblem) -> String {
    match problem {
        DiagnosticStoredTypeProblem::Slice => {
            String::from("a slice has no finite inline representation")
        }
        DiagnosticStoredTypeProblem::TraitView => {
            String::from("a trait view has no finite inline representation")
        }
        DiagnosticStoredTypeProblem::Generator => {
            String::from("a generator has no finite inline representation")
        }
        DiagnosticStoredTypeProblem::ContextualSelf => {
            String::from("contextual Self has no resolved inline representation")
        }
        DiagnosticStoredTypeProblem::ReferencedTypeHasNoFiniteRepresentation => {
            String::from("the referenced type has no finite inline representation")
        }
    }
}

pub(super) fn format_english_propagation_problem(problem: &DiagnosticPropagationProblem) -> String {
    match problem {
        DiagnosticPropagationProblem::NullableBoundaryUnavailable {
            operand,
            available_boundaries,
        } => format!(
            "cannot propagate {} because no enclosing boundary returns a nullable type{}",
            format_english_type(operand),
            format_available_types(available_boundaries, "return"),
        ),
        DiagnosticPropagationProblem::ResultBoundaryUnavailable {
            source_error,
            available_errors,
        } => format!(
            "cannot propagate error type {} because no enclosing result boundary accepts it{}",
            format_english_type(source_error),
            format_available_types(available_errors, "accept"),
        ),
    }
}

fn format_available_types(types: &[bray_diagnostics::DiagnosticType], relation: &str) -> String {
    if types.is_empty() {
        return String::new();
    }

    let types = types
        .iter()
        .map(format_english_type)
        .collect::<Vec<_>>()
        .join(", ");

    format!("; available boundaries {relation} {types}")
}

pub(super) fn format_english_array_generator_problem(
    problem: &DiagnosticArrayGeneratorCardinalityProblem,
) -> String {
    match problem {
        DiagnosticArrayGeneratorCardinalityProblem::SourceCountUnavailable {
            source,
            element,
            required,
        } => format!(
            "fixed-array generator of {} from {} requires {}, but the source iteration count is not statically known",
            format_english_type(element),
            format_english_type(source),
            format_english_array_length(*required),
        ),
        DiagnosticArrayGeneratorCardinalityProblem::SourceLengthMismatch {
            source,
            element,
            source_length,
            required,
        } => format!(
            "fixed-array generator of {} from {} selects {} but requires {}",
            format_english_type(element),
            format_english_type(source),
            format_english_array_length(*source_length),
            format_english_array_length(*required),
        ),
        DiagnosticArrayGeneratorCardinalityProblem::YieldCountNotExact {
            element,
            source_length,
            required,
            actual,
        } => format!(
            "fixed-array generator of {} over {} can yield {} per iteration but requires a result of {}",
            format_english_type(element),
            format_english_array_length(*source_length),
            format_english_yield_cardinality(*actual),
            format_english_array_length(*required),
        ),
    }
}

fn format_english_array_length(length: DiagnosticArrayLength) -> String {
    match length {
        DiagnosticArrayLength::Exact(length) => format!("{length} elements"),
        DiagnosticArrayLength::Symbolic => String::from("a symbolic number of elements"),
    }
}

pub(super) fn format_english_refinement_capacity(capacity: DiagnosticRefinementCapacity) -> String {
    format!(
        "flow-sensitive analysis requires {} {}, exceeding the configured maximum of {}",
        capacity.actual(),
        format_english_refinement_capacity_surface(capacity.surface()),
        capacity.maximum(),
    )
}

const fn format_english_refinement_capacity_surface(
    surface: DiagnosticRefinementCapacitySurface,
) -> &'static str {
    match surface {
        DiagnosticRefinementCapacitySurface::RefinementEntries => "refinement entries",
        DiagnosticRefinementCapacitySurface::RetainedStateCells => "retained state cells",
        DiagnosticRefinementCapacitySurface::PublishedRefinements => "published refinements",
    }
}

pub(super) const fn format_english_memory_operation(
    operation: DiagnosticMemoryOperation,
) -> &'static str {
    match operation {
        DiagnosticMemoryOperation::AddressOf => "shared address operation",
        DiagnosticMemoryOperation::MutableAddressOf => "mutable address operation",
        DiagnosticMemoryOperation::NullPointer => "null pointer construction",
        DiagnosticMemoryOperation::PointerNullCheck => "null pointer check",
        DiagnosticMemoryOperation::PointerElementOffset => "pointer element offset",
        DiagnosticMemoryOperation::PointerByteOffset => "pointer byte offset",
        DiagnosticMemoryOperation::PointerReinterpretation => "pointer reinterpretation",
        DiagnosticMemoryOperation::CallbackState => "callback state access",
        DiagnosticMemoryOperation::PointerRead => "pointer read",
        DiagnosticMemoryOperation::PointerWrite => "pointer write",
        DiagnosticMemoryOperation::MemoryCopy => "non-overlapping memory copy",
        DiagnosticMemoryOperation::OverlappingMemoryCopy => "overlapping memory copy",
        DiagnosticMemoryOperation::SizeDetermination => "determine a type's size",
        DiagnosticMemoryOperation::AlignmentDetermination => "determine a type's alignment",
        DiagnosticMemoryOperation::StrideDetermination => "determine a type's stride",
        DiagnosticMemoryOperation::LayoutDetermination => "determine a type's memory layout",
        DiagnosticMemoryOperation::RawAllocation => "raw allocation",
        DiagnosticMemoryOperation::RawDeallocation => "raw deallocation",
        DiagnosticMemoryOperation::Allocation => "allocation",
        DiagnosticMemoryOperation::Deallocation => "deallocation",
        DiagnosticMemoryOperation::RawBufferCapacity => "raw buffer capacity access",
        DiagnosticMemoryOperation::RawBufferInitializedCount => {
            "raw buffer initialized-count access"
        }
        DiagnosticMemoryOperation::RawBufferPointer => "raw buffer pointer access",
        DiagnosticMemoryOperation::RawBufferInitializedSlice => {
            "raw buffer initialized slice access"
        }
        DiagnosticMemoryOperation::MutableRawBufferInitializedSlice => {
            "mutable raw buffer initialized slice access"
        }
        DiagnosticMemoryOperation::RawBufferSparePointer => "raw buffer spare pointer access",
        DiagnosticMemoryOperation::RawBufferSetInitializedCount => {
            "raw buffer initialized-count update"
        }
        DiagnosticMemoryOperation::RawBufferRelease => "raw buffer release",
        DiagnosticMemoryOperation::RawBufferReplace => "raw buffer replacement",
        DiagnosticMemoryOperation::ByteBufferFill => "byte buffer fill",
        DiagnosticMemoryOperation::ByteBufferCopy => "byte buffer copy",
        DiagnosticMemoryOperation::ByteBufferRead => "byte buffer read",
        DiagnosticMemoryOperation::SliceLength => "slice length access",
    }
}

pub(super) fn format_english_callback_state_problem(
    problem: DiagnosticCallbackStateProblem,
) -> String {
    match problem {
        DiagnosticCallbackStateProblem::OutsideCallable => {
            String::from("callback state is requested outside a callable")
        }
        DiagnosticCallbackStateProblem::LanguageAbi => {
            String::from("callback state is requested from a Bray-ABI callable")
        }
        DiagnosticCallbackStateProblem::CallableNotTrusted => {
            String::from("callback state is requested from an untrusted callable")
        }
        DiagnosticCallbackStateProblem::ReceiverPresent => {
            String::from("callback state is requested from a callable with a receiver")
        }
        DiagnosticCallbackStateProblem::MissingSymbolDirective => String::from(
            "callback state is requested from a callable without a native symbol directive",
        ),
        DiagnosticCallbackStateProblem::MissingContextParameter => {
            String::from("callback state is requested from a callable without a context parameter")
        }
        DiagnosticCallbackStateProblem::ContextArgumentNotName => {
            String::from("callback state context is not a direct parameter reference")
        }
        DiagnosticCallbackStateProblem::ContextArgumentNotParameter => {
            String::from("callback state context does not reference a callable parameter")
        }
        DiagnosticCallbackStateProblem::ContextParameterNotFirst { actual_ordinal } => format!(
            "callback state context references parameter {}, not the first parameter",
            actual_ordinal.saturating_add(1)
        ),
    }
}

pub(super) fn format_english_storage_access(access: &DiagnosticStorageAccess) -> String {
    let purpose = match access.purpose() {
        DiagnosticStorageAccessPurpose::Read => "read",
        DiagnosticStorageAccessPurpose::Initialize => "initialization",
        DiagnosticStorageAccessPurpose::Write => "write",
        DiagnosticStorageAccessPurpose::Move => "move",
        DiagnosticStorageAccessPurpose::Copy => "copy",
        DiagnosticStorageAccessPurpose::SharedBorrow => "shared borrow",
        DiagnosticStorageAccessPurpose::MutableBorrow => "mutable borrow",
        DiagnosticStorageAccessPurpose::Assignment => "assignment",
        DiagnosticStorageAccessPurpose::MemberSelection => "member selection",
        DiagnosticStorageAccessPurpose::IndexSelection => "index selection",
        DiagnosticStorageAccessPurpose::SliceSelection => "slice selection",
        DiagnosticStorageAccessPurpose::Projection => "projected access",
    };

    let root = match access.root() {
        DiagnosticStorageRoot::Local => "local storage",
        DiagnosticStorageRoot::Parameter => "parameter storage",
        DiagnosticStorageRoot::Receiver => "receiver storage",
        DiagnosticStorageRoot::AnonymousParameter => "anonymous parameter storage",
        DiagnosticStorageRoot::PredicateParameter => "predicate parameter storage",
        DiagnosticStorageRoot::PostconditionResult => "postcondition result storage",
        DiagnosticStorageRoot::Result => "result storage",
        DiagnosticStorageRoot::Temporary => "temporary storage",
        DiagnosticStorageRoot::CustomIndexBorrow => "custom-index borrow storage",
        DiagnosticStorageRoot::IterationCursor => "iteration cursor storage",
        DiagnosticStorageRoot::IterationElement => "iteration element storage",
        DiagnosticStorageRoot::Allocation => "allocated storage",
        DiagnosticStorageRoot::CompilerCreated => "compiler-provided storage",
        DiagnosticStorageRoot::Alternative => "pattern-selected storage",
        DiagnosticStorageRoot::Borrow => "borrowed storage",
        DiagnosticStorageRoot::BorrowedStorage => "stored borrow",
        DiagnosticStorageRoot::OwnedIndirection => "owned indirect storage",
        DiagnosticStorageRoot::Recovery => "recovered storage",
    };

    let mut path = String::new();

    for projection in access.projections() {
        match projection {
            DiagnosticStorageProjection::ProductField(name) => {
                let _ = write!(path, ".{name}");
            }
            DiagnosticStorageProjection::TupleElement(ordinal) => {
                let _ = write!(path, ".{ordinal}");
            }
            DiagnosticStorageProjection::ElementFromStart(ordinal) => {
                let _ = write!(path, "[from start {ordinal}]");
            }
            DiagnosticStorageProjection::ElementFromEnd(ordinal) => {
                let _ = write!(path, "[from end {ordinal}]");
            }
            DiagnosticStorageProjection::ActiveUnionPayloadField { variant, field } => {
                let _ = write!(path, ".{variant}.{field}");
            }
            DiagnosticStorageProjection::IndexedElement => path.push_str("[index]"),
            DiagnosticStorageProjection::SliceRange { has_start, has_end } => {
                let bounds = match (*has_start, *has_end) {
                    (true, true) => "start..end",
                    (true, false) => "start..",
                    (false, true) => "..end",
                    (false, false) => "..",
                };

                let _ = write!(path, "[{bounds}]");
            }
            DiagnosticStorageProjection::NullableValue => path.push_str(".present value"),
            DiagnosticStorageProjection::OwnedTarget => path.push_str(".owned target"),
        }
    }

    format!(
        "{purpose} of {root}{path} with type {}",
        format_english_type(access.reached_type())
    )
}

const fn format_english_yield_cardinality(cardinality: DiagnosticYieldCardinality) -> &'static str {
    match cardinality {
        DiagnosticYieldCardinality::Zero => "zero values",
        DiagnosticYieldCardinality::Multiple => "multiple values",
        DiagnosticYieldCardinality::ZeroOrOne => "zero or one value",
        DiagnosticYieldCardinality::OneOrMultiple => "one or multiple values",
        DiagnosticYieldCardinality::ZeroOrMultiple => "zero or multiple values",
        DiagnosticYieldCardinality::ZeroOneOrMultiple => "zero, one, or multiple values",
        DiagnosticYieldCardinality::Break => "no value when a break exits the iteration",
        DiagnosticYieldCardinality::Unknown => "an unprovable number of values",
    }
}

pub(super) const fn format_english_constant_operation(
    operation: DiagnosticConstantOperation,
) -> &'static str {
    match operation {
        DiagnosticConstantOperation::Add => "addition",
        DiagnosticConstantOperation::Subtract => "subtraction",
        DiagnosticConstantOperation::Multiply => "multiplication",
        DiagnosticConstantOperation::Divide => "division",
        DiagnosticConstantOperation::Remainder => "remainder",
        DiagnosticConstantOperation::Exponentiate => "exponentiation",
        DiagnosticConstantOperation::LogicalAnd => "logical conjunction",
        DiagnosticConstantOperation::LogicalOr => "logical disjunction",
        DiagnosticConstantOperation::LogicalNot => "logical negation",
        DiagnosticConstantOperation::BitwiseAnd => "bitwise conjunction",
        DiagnosticConstantOperation::BitwiseOr => "bitwise disjunction",
        DiagnosticConstantOperation::BitwiseXor => "bitwise exclusive disjunction",
        DiagnosticConstantOperation::BitwiseNot => "bitwise negation",
        DiagnosticConstantOperation::ShiftLeft => "left shift",
        DiagnosticConstantOperation::ShiftRight => "right shift",
        DiagnosticConstantOperation::Equal => "equality comparison",
        DiagnosticConstantOperation::NotEqual => "inequality comparison",
        DiagnosticConstantOperation::Less => "less-than comparison",
        DiagnosticConstantOperation::LessEqual => "less-than-or-equal comparison",
        DiagnosticConstantOperation::Greater => "greater-than comparison",
        DiagnosticConstantOperation::GreaterEqual => "greater-than-or-equal comparison",
        DiagnosticConstantOperation::MatrixMultiply => "matrix multiplication",
        DiagnosticConstantOperation::Conversion => "conversion",
    }
}

pub(super) const fn format_english_expression_category(
    kind: DiagnosticExpressionCategory,
) -> &'static str {
    match kind {
        DiagnosticExpressionCategory::Block => "block expression",
        DiagnosticExpressionCategory::Literal => "literal",
        DiagnosticExpressionCategory::NameReference => "name reference",
        DiagnosticExpressionCategory::PatternReference => "pattern reference",
        DiagnosticExpressionCategory::UnaryOperation => "unary operation",
        DiagnosticExpressionCategory::BinaryOperation => "binary operation",
        DiagnosticExpressionCategory::Assignment => "assignment",
        DiagnosticExpressionCategory::Call => "call",
        DiagnosticExpressionCategory::Conversion => "conversion",
        DiagnosticExpressionCategory::AnonymousCallable => "anonymous callable",
        DiagnosticExpressionCategory::Await => "await expression",
        DiagnosticExpressionCategory::Aggregate => "aggregate expression",
        DiagnosticExpressionCategory::Indexing => "indexing operation",
        DiagnosticExpressionCategory::Propagation => "propagation expression",
        DiagnosticExpressionCategory::ControlFlow => "control-flow expression",
        DiagnosticExpressionCategory::Generator => "generator expression",
        DiagnosticExpressionCategory::Construction => "construction expression",
        DiagnosticExpressionCategory::MemberAccess => "member access",
        DiagnosticExpressionCategory::TraitMemberAccess => "trait-qualified member access",
        DiagnosticExpressionCategory::Recovered => "recovered expression",
    }
}
