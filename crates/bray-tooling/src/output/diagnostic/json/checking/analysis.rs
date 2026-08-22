use serde::Serialize;

use super::super::{
    DiagnosticProblemJson, DiagnosticTypeJson, problem, problem_array_length, problem_count_u64,
    problem_text, problem_type, problem_types,
};

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticPatternCoverageJson {
    subject_type: DiagnosticTypeJson,
    missing_cases: Vec<DiagnosticPatternMissingCaseJson>,
    omitted_count: u64,
}

impl DiagnosticPatternCoverageJson {
    pub(in crate::output::diagnostic::json) fn from_coverage(
        coverage: &bray_diagnostics::DiagnosticPatternCoverage,
    ) -> Self {
        Self {
            subject_type: DiagnosticTypeJson::from_type(coverage.subject_type()),
            missing_cases: coverage
                .missing()
                .iter()
                .map(DiagnosticPatternMissingCaseJson::from_case)
                .collect(),
            omitted_count: coverage.omitted_count(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DiagnosticPatternMissingCaseJson {
    NullableAbsent,
    NullablePresent,
    Boolean { value: bool },
    UnionVariant { name: String },
    RemainingValues,
}

impl DiagnosticPatternMissingCaseJson {
    fn from_case(case: &bray_diagnostics::DiagnosticPatternMissingCase) -> Self {
        match case {
            bray_diagnostics::DiagnosticPatternMissingCase::NullableAbsent => Self::NullableAbsent,
            bray_diagnostics::DiagnosticPatternMissingCase::NullablePresent => {
                Self::NullablePresent
            }
            bray_diagnostics::DiagnosticPatternMissingCase::Boolean(value) => {
                Self::Boolean { value: *value }
            }
            bray_diagnostics::DiagnosticPatternMissingCase::UnionVariant(name) => {
                Self::UnionVariant { name: name.clone() }
            }
            bray_diagnostics::DiagnosticPatternMissingCase::RemainingValues => {
                Self::RemainingValues
            }
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticStorageAccessJson {
    purpose: &'static str,
    root: &'static str,
    projections: Vec<DiagnosticProblemJson>,
    reached_type: DiagnosticTypeJson,
}

impl DiagnosticStorageAccessJson {
    pub(in crate::output::diagnostic::json) fn from_access(
        access: &bray_diagnostics::DiagnosticStorageAccess,
    ) -> Self {
        Self {
            purpose: access.purpose().as_str(),
            root: access.root().as_str(),
            projections: access
                .projections()
                .iter()
                .map(storage_projection_json)
                .collect(),
            reached_type: DiagnosticTypeJson::from_type(access.reached_type()),
        }
    }
}

fn storage_projection_json(
    projection: &bray_diagnostics::DiagnosticStorageProjection,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticStorageProjection as Projection;

    let context = match projection {
        Projection::ProductField(name) => vec![problem_text("name", name)],
        Projection::TupleElement(ordinal)
        | Projection::ElementFromStart(ordinal)
        | Projection::ElementFromEnd(ordinal) => vec![problem_count_u64("ordinal", *ordinal)],
        Projection::ActiveUnionPayloadField { variant, field } => vec![
            problem_text("variant", variant),
            problem_text("field", field),
        ],
        Projection::SliceRange { has_start, has_end } => vec![
            problem_text("start", if *has_start { "present" } else { "omitted" }),
            problem_text("end", if *has_end { "present" } else { "omitted" }),
        ],
        Projection::IndexedElement | Projection::NullableValue | Projection::OwnedTarget => {
            Vec::new()
        }
    };

    DiagnosticProblemJson {
        reason: projection.as_str(),
        context,
    }
}

pub(in crate::output::diagnostic::json) fn layout_problem_json(
    problem: &bray_diagnostics::DiagnosticLayoutProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticLayoutProblem as Problem;

    let context = match problem {
        Problem::UnsupportedMode(mode) => vec![problem_text("mode", mode)],
        Problem::DuplicateOption(option)
        | Problem::OptionNotConstant(option)
        | Problem::TransparentOption(option) => {
            vec![problem_text("option", option.as_str())]
        }
        Problem::UnknownOption(option) => vec![problem_text("option", option)],
        Problem::OptionNotPowerOfTwo { option, value } => vec![
            problem_text("option", option.as_str()),
            problem_count_u64("value", *value),
        ],
        Problem::TransparentFieldCount { actual } => {
            vec![problem_count_u64("actual", *actual)]
        }
        Problem::OpaqueSizeNotAligned { size, alignment } => vec![
            problem_count_u64("size", *size),
            problem_count_u64("alignment", *alignment),
        ],
        Problem::MissingMode
        | Problem::UnexpectedPositionalArgument
        | Problem::TransparentUnion
        | Problem::PackingRequiresStable
        | Problem::PackingRequiresPlainStorage
        | Problem::TagRequiresUnion
        | Problem::CUnionRequiresTag
        | Problem::OpaqueSizeRequiresBodylessStruct
        | Problem::BodylessStructRequiresSizeAndAlignment
        | Problem::OpaqueStorageRequiresStableOrC
        | Problem::TaglessUnionRequiresCLayout => Vec::new(),
    };

    DiagnosticProblemJson {
        reason: problem.category(),
        context,
    }
}

pub(in crate::output::diagnostic::json) fn union_tag_problem_json(
    problem: &bray_diagnostics::DiagnosticUnionTagProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticUnionTagProblem as Problem;

    let context = match problem {
        Problem::UnsupportedType(ty) => vec![problem_text("type", ty)],
        Problem::ArgumentCount { actual } => vec![problem_count_u64("actual", *actual)],
        Problem::PartialExplicitTags { explicit, total } => vec![
            problem_count_u64("explicit", *explicit),
            problem_count_u64("total", *total),
        ],
        Problem::InferredValueTooWide {
            actual_bits,
            maximum_bits,
        } => vec![
            problem_count_u64("actual_bits", *actual_bits),
            problem_count_u64("maximum_bits", *maximum_bits),
        ],
        Problem::ValueOutsideSelectedType {
            signed,
            width_bits,
            value_negative,
            value_bits,
        } => vec![
            problem_text("signedness", if *signed { "signed" } else { "unsigned" }),
            problem_count_u64("width_bits", u64::from(*width_bits)),
            problem_text(
                "value_sign",
                if *value_negative {
                    "negative"
                } else {
                    "nonnegative"
                },
            ),
            problem_count_u64("value_bits", *value_bits),
        ],
        Problem::RequiresExplicitLayout
        | Problem::ValueNotConstant
        | Problem::DuplicateValue
        | Problem::NegativeInferredValue
        | Problem::TaglessUnionHasVariantTag => Vec::new(),
    };

    DiagnosticProblemJson {
        reason: problem.category(),
        context,
    }
}

pub(in crate::output::diagnostic::json) fn copy_contract_problem_json(
    problem: bray_diagnostics::DiagnosticCopyContractProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticCopyContractProblem as Problem;

    let context = match problem {
        Problem::UnexpectedArguments { actual } => vec![problem_count_u64("actual", actual)],
        Problem::LifecycleBehavior
        | Problem::ConditionalMembersRequireGenericType
        | Problem::NonCopyableMember => Vec::new(),
    };

    DiagnosticProblemJson {
        reason: problem.category(),
        context,
    }
}

pub(in crate::output::diagnostic::json) fn propagation_problem_json(
    problem: &bray_diagnostics::DiagnosticPropagationProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticPropagationProblem as Problem;

    let context = match problem {
        Problem::NullableBoundaryUnavailable {
            operand,
            available_boundaries,
        } => vec![
            problem_type("operand", operand),
            problem_types("available_boundaries", available_boundaries),
        ],
        Problem::ResultBoundaryUnavailable {
            source_error,
            available_errors,
        } => vec![
            problem_type("source_error", source_error),
            problem_types("available_errors", available_errors),
        ],
    };

    DiagnosticProblemJson {
        reason: problem.category(),
        context,
    }
}

pub(in crate::output::diagnostic::json) fn array_generator_problem_json(
    problem: &bray_diagnostics::DiagnosticArrayGeneratorCardinalityProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticArrayGeneratorCardinalityProblem as Problem;

    let context = match problem {
        Problem::SourceCountUnavailable {
            source,
            element,
            required,
        } => vec![
            problem_type("source", source),
            problem_type("element", element),
            problem_array_length("required", *required),
        ],
        Problem::SourceLengthMismatch {
            source,
            element,
            source_length,
            required,
        } => vec![
            problem_type("source", source),
            problem_type("element", element),
            problem_array_length("source_length", *source_length),
            problem_array_length("required", *required),
        ],
        Problem::YieldCountNotExact {
            element,
            source_length,
            required,
            actual,
        } => vec![
            problem_type("element", element),
            problem_array_length("source_length", *source_length),
            problem_array_length("required", *required),
            problem_text("actual", actual.as_str()),
        ],
    };

    DiagnosticProblemJson {
        reason: problem.category(),
        context,
    }
}

pub(in crate::output::diagnostic::json) fn refinement_capacity_json(
    capacity: bray_diagnostics::DiagnosticRefinementCapacity,
) -> DiagnosticProblemJson {
    problem(
        "capacity_exceeded",
        [
            problem_text("surface", capacity.surface().as_str()),
            problem_count_u64("actual", capacity.actual()),
            problem_count_u64("maximum", capacity.maximum()),
        ],
    )
}

pub(in crate::output::diagnostic::json) fn callback_state_problem_json(
    problem_value: bray_diagnostics::DiagnosticCallbackStateProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticCallbackStateProblem as Problem;

    let context = match problem_value {
        Problem::ContextParameterNotFirst { actual_ordinal } => {
            vec![problem_count_u64("actual_ordinal", actual_ordinal)]
        }
        Problem::OutsideCallable
        | Problem::LanguageAbi
        | Problem::CallableNotTrusted
        | Problem::ReceiverPresent
        | Problem::MissingSymbolDirective
        | Problem::MissingContextParameter
        | Problem::ContextArgumentNotName
        | Problem::ContextArgumentNotParameter => Vec::new(),
    };

    DiagnosticProblemJson {
        reason: problem_value.as_str(),
        context,
    }
}
