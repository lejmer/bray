use bray_diagnostics::{
    Diagnostic, DiagnosticAlignmentKind, DiagnosticArg, DiagnosticArrayGeneratorCardinalityProblem,
    DiagnosticArrayLength, DiagnosticCallbackStateProblem, DiagnosticConstructionInputRejection,
    DiagnosticDependencyRequirementKind, DiagnosticDependencySubjectKind,
    DiagnosticExpressionCategory, DiagnosticId, DiagnosticKind, DiagnosticLayoutOption,
    DiagnosticLayoutProblem, DiagnosticMemoryOperation, DiagnosticNamedType, DiagnosticNote,
    DiagnosticNoteKind, DiagnosticPatternCoverage, DiagnosticPatternMissingCase,
    DiagnosticPropagationProblem, DiagnosticRefinementCapacity,
    DiagnosticRefinementCapacitySurface, DiagnosticRejectedSelectionCandidate,
    DiagnosticSelectionCandidate, DiagnosticSelectionCandidateIdentity,
    DiagnosticSelectionCandidateSignature, DiagnosticSelectionCandidates, DiagnosticSelectionKind,
    DiagnosticSelectionRejectionReason, DiagnosticSelectionRejections, DiagnosticSourceInput,
    DiagnosticSourceInputOrigin, DiagnosticStorageAccess, DiagnosticStorageAccessPurpose,
    DiagnosticStorageProjection, DiagnosticStorageRoot, DiagnosticType, DiagnosticTypeArgument,
    DiagnosticUnionTagProblem, DiagnosticYieldCardinality, SeverityKind,
};
use bray_source::TextSize;

use crate::renderer::DiagnosticRenderer;
use crate::{DiagnosticLocale, RenderedDiagnosticNoteKind};

#[test]
fn scoped_selection_failures_identify_enter_and_exit() {
    let renderer = DiagnosticRenderer::english();

    for (kind, expected) in [
        (
            DiagnosticSelectionKind::ScopeEnter,
            "no applicable scope enter candidate",
        ),
        (
            DiagnosticSelectionKind::ScopeExit,
            "no applicable scope exit candidate",
        ),
    ] {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingNoApplicableCandidate,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::selection_kind(kind));

        let rendered = renderer.render(&diagnostic);

        assert_eq!(rendered.message(), expected);
    }
}

#[test]
fn renderer_localizes_checker_diagnostics() {
    let incompatible = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingIncompatibleExpressionType,
        SeverityKind::Error,
    )
    .with_arg(bray_diagnostics::DiagnosticArg::expected_type(
        bray_diagnostics::DiagnosticType::Boolean,
    ))
    .with_arg(bray_diagnostics::DiagnosticArg::actual_type(
        bray_diagnostics::DiagnosticType::Tuple(2),
    ));

    let unresolved = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingCannotInferExpressionType,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expression_category(
        DiagnosticExpressionCategory::NameReference,
    ));

    let ambiguous = Diagnostic::new(
        DiagnosticId::new(4),
        DiagnosticKind::CheckingAmbiguousCandidate,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::selection_kind(
        DiagnosticSelectionKind::Operator,
    ))
    .with_arg(DiagnosticArg::selection_candidates(
        DiagnosticSelectionCandidates::new([
            DiagnosticSelectionCandidate::new(
                DiagnosticSelectionCandidateIdentity::BuiltIn,
                DiagnosticSelectionCandidateSignature::Operation {
                    operand_types: Box::new([DiagnosticType::I32, DiagnosticType::I32]),
                    result_type: Some(DiagnosticType::I32),
                },
            ),
            DiagnosticSelectionCandidate::new(
                DiagnosticSelectionCandidateIdentity::ExpressionValue,
                DiagnosticSelectionCandidateSignature::Callable {
                    parameter_types: Box::new([DiagnosticType::I32, DiagnosticType::I32]),
                    result_type: DiagnosticType::I32,
                },
            ),
        ]),
    ));

    let incompatible_candidate = Diagnostic::new(
        DiagnosticId::new(10),
        DiagnosticKind::CheckingIncompatibleCandidate,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::selection_kind(
        DiagnosticSelectionKind::Construction,
    ))
    .with_arg(DiagnosticArg::selection_rejections(
        DiagnosticSelectionRejections::try_from_prefix(
            [DiagnosticRejectedSelectionCandidate::new(
                DiagnosticSelectionCandidate::new(
                    DiagnosticSelectionCandidateIdentity::BuiltIn,
                    DiagnosticSelectionCandidateSignature::Operation {
                        operand_types: Box::new([DiagnosticType::I32]),
                        result_type: Some(DiagnosticType::I32),
                    },
                ),
                DiagnosticSelectionRejectionReason::ConstructionInput(
                    DiagnosticConstructionInputRejection::UnknownName {
                        provided: String::from("y"),
                        accepted: Box::new([String::from("x")]),
                    },
                ),
            )],
            1,
        )
        .unwrap_or_else(|_| panic!("one rejection must fit the diagnostic bound")),
    ));

    let target_alignment = Diagnostic::new(
        DiagnosticId::new(5),
        DiagnosticKind::CheckingTargetAlignmentUnsupported,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::alignment_kind(
        DiagnosticAlignmentKind::Storage,
    ))
    .with_arg(DiagnosticArg::required_alignment(64))
    .with_arg(DiagnosticArg::maximum_alignment(16));

    let target_alignment =
        target_alignment.with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"));

    let incompatible_pattern = Diagnostic::new(
        DiagnosticId::new(6),
        DiagnosticKind::CheckingIncompatiblePattern,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_type(
        bray_diagnostics::DiagnosticType::Boolean,
    ));

    let non_exhaustive_match = Diagnostic::new(
        DiagnosticId::new(7),
        DiagnosticKind::CheckingNonExhaustiveMatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::pattern_coverage(
        DiagnosticPatternCoverage::new(
            DiagnosticType::Boolean,
            [DiagnosticPatternMissingCase::Boolean(false)],
            0,
        ),
    ));

    let named_type_mismatch = Diagnostic::new(
        DiagnosticId::new(8),
        DiagnosticKind::CheckingIncompatibleExpressionType,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expected_type(DiagnosticType::Named(
        DiagnosticNamedType::new(
            [String::from("collection"), String::from("MoveCursor")],
            [DiagnosticTypeArgument::Type(DiagnosticType::I32)],
        ),
    )))
    .with_arg(DiagnosticArg::actual_type(DiagnosticType::Named(
        DiagnosticNamedType::new(
            [String::from("collection"), String::from("ReadCursor")],
            [DiagnosticTypeArgument::Type(DiagnosticType::I32)],
        ),
    )));

    let unavailable_await_dependency = Diagnostic::new(
        DiagnosticId::new(9),
        DiagnosticKind::CheckingUnavailableAwaitDependency,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::dependency_subject_kind(
        DiagnosticDependencySubjectKind::Storage,
    ))
    .with_arg(DiagnosticArg::dependency_requirement_kind(
        DiagnosticDependencyRequirementKind::StorageAlive,
    ));

    let renderer = DiagnosticRenderer::english();

    assert_eq!(
        renderer.render(&incompatible).message(),
        "expected bool, but found tuple type with 2 elements"
    );

    assert_eq!(
        renderer.render(&named_type_mismatch).message(),
        "expected collection.MoveCursor<i32>, but found collection.ReadCursor<i32>"
    );

    assert_eq!(
        renderer.render(&unavailable_await_dependency).message(),
        "await requires storage to remain alive"
    );

    let carried_value_dependency = Diagnostic::new(
        DiagnosticId::new(10),
        DiagnosticKind::CheckingUnavailableAwaitDependency,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::dependency_subject_kind(
        DiagnosticDependencySubjectKind::Storage,
    ))
    .with_arg(DiagnosticArg::dependency_requirement_kind(
        DiagnosticDependencyRequirementKind::ValueDependencies,
    ));

    assert_eq!(
        renderer.render(&carried_value_dependency).message(),
        "await requires storage to preserve the dependencies carried by its value"
    );

    assert_eq!(
        renderer.render(&unresolved).message(),
        "cannot infer the type of this name reference"
    );

    assert_eq!(
        renderer.render(&ambiguous).message(),
        concat!(
            "operator selection is ambiguous between built-in operation with signature ",
            "(i32, i32) -> i32 and callable expression value with signature ",
            "(i32, i32) -> i32"
        )
    );

    assert_eq!(
        renderer.render(&incompatible_candidate).message(),
        concat!(
            "construction operation candidates reject the supplied expressions: ",
            "built-in operation with signature (i32) -> i32: input name y is not accepted. ",
            "Candidate input names are x"
        )
    );

    assert_eq!(
        renderer.render(&target_alignment).message(),
        "required storage alignment 64 exceeds target 'x86_64-unknown-linux-gnu' maximum of 16"
    );

    assert_eq!(
        renderer.render(&incompatible_pattern).message(),
        "pattern is incompatible with bool"
    );

    assert_eq!(
        renderer.render(&non_exhaustive_match).message(),
        "match coverage is incomplete: bool is missing false"
    );
}

#[test]
fn renderer_localizes_type_representation_causes() {
    let invalid_layout = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingInvalidLayoutDirective,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::layout_problem(
        DiagnosticLayoutProblem::OptionNotPowerOfTwo {
            option: DiagnosticLayoutOption::Alignment,
            value: 6,
        },
    ));

    let invalid_tag = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingInvalidUnionTag,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::union_tag_problem(
        DiagnosticUnionTagProblem::ValueOutsideSelectedType {
            signed: false,
            width_bits: 8,
            value_negative: false,
            value_bits: 9,
        },
    ));

    let renderer = DiagnosticRenderer::english();

    assert_eq!(
        renderer.render(&invalid_layout).message(),
        "the alignment value 6 is not a power of two"
    );

    assert_eq!(
        renderer.render(&invalid_tag).message(),
        "the nonnegative variant tag requires 9 magnitude bits, outside the selected unsigned 8-bit range 0 through 255"
    );
}

#[test]
fn renderer_localizes_propagation_and_array_cardinality_causes() {
    let result = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::propagation_problem(
        DiagnosticPropagationProblem::ResultBoundaryUnavailable {
            source_error: DiagnosticType::I32,
            available_errors: vec![DiagnosticType::I64].into_boxed_slice(),
        },
    ));

    let nullable = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::propagation_problem(
        DiagnosticPropagationProblem::NullableBoundaryUnavailable {
            operand: DiagnosticType::Nullable,
            available_boundaries: vec![DiagnosticType::I32].into_boxed_slice(),
        },
    ));

    let unavailable_count = Diagnostic::new(
        DiagnosticId::new(2),
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::array_generator_cardinality_problem(
        DiagnosticArrayGeneratorCardinalityProblem::SourceCountUnavailable {
            source: DiagnosticType::Named(DiagnosticNamedType::new(
                [String::from("app"), String::from("Items")],
                [],
            )),
            element: DiagnosticType::Boolean,
            required: DiagnosticArrayLength::Exact(2),
        },
    ));

    let divergent_yield = Diagnostic::new(
        DiagnosticId::new(3),
        DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::array_generator_cardinality_problem(
        DiagnosticArrayGeneratorCardinalityProblem::YieldCountNotExact {
            element: DiagnosticType::Boolean,
            source_length: DiagnosticArrayLength::Symbolic,
            required: DiagnosticArrayLength::Exact(2),
            actual: DiagnosticYieldCardinality::Multiple,
        },
    ));

    let renderer = DiagnosticRenderer::english();

    assert_eq!(
        renderer.render(&result).message(),
        "cannot propagate error type i32 because no enclosing result boundary accepts it. Available boundaries accept i64"
    );

    assert_eq!(
        renderer.render(&nullable).message(),
        "cannot propagate nullable type because no enclosing boundary returns a nullable type. Available boundaries return i32"
    );

    assert_eq!(
        renderer.render(&unavailable_count).message(),
        "fixed-array generator of bool from app.Items requires 2 elements, but the source iteration count is not statically known"
    );

    assert_eq!(
        renderer.render(&divergent_yield).message(),
        "fixed-array generator of bool over a symbolic number of elements can yield multiple values per iteration but requires a result of 2 elements"
    );
}

#[test]
fn renderer_localizes_exact_refinement_capacity() {
    let capacity = DiagnosticRefinementCapacity::try_new(
        DiagnosticRefinementCapacitySurface::PublishedRefinements,
        16_777_217,
        16_777_216,
    )
    .unwrap_or_else(|| panic!("limit plus one must be a capacity violation"));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingRefinementCapacityExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::refinement_capacity(capacity));

    assert_eq!(
        DiagnosticRenderer::english().render(&diagnostic).message(),
        "flow-sensitive analysis requires 16777217 published refinements, exceeding the configured maximum of 16777216"
    );
}

#[test]
fn renderer_names_raw_buffer_prefix_operations() {
    for (operation, name) in [
        (DiagnosticMemoryOperation::RawBufferPush, "raw buffer push"),
        (DiagnosticMemoryOperation::RawBufferPop, "raw buffer pop"),
    ] {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::target_triple("wasm32-unknown-unknown"))
        .with_arg(DiagnosticArg::memory_operation(operation));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            format!("target 'wasm32-unknown-unknown' does not provide the required {name}"),
        );
    }
}

#[test]
fn renderer_localizes_memory_operation_and_callback_causes() {
    let unavailable = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple("wasm32-unknown-unknown"))
    .with_arg(DiagnosticArg::memory_operation(
        DiagnosticMemoryOperation::PointerRead,
    ));

    let invalid = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingInvalidTargetControlContract,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple("x86_64-unknown-linux-gnu"))
    .with_arg(DiagnosticArg::memory_operation(
        DiagnosticMemoryOperation::InlineAssembly,
    ));

    let callback = Diagnostic::new(
        DiagnosticId::new(2),
        DiagnosticKind::CheckingInvalidCallbackStateContext,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::callback_state_problem(
        DiagnosticCallbackStateProblem::ContextParameterNotFirst { actual_ordinal: 2 },
    ))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::CallbackStateRequirements,
    ));

    let renderer = DiagnosticRenderer::english();

    assert_eq!(
        renderer.render(&unavailable).message(),
        "target 'wasm32-unknown-unknown' does not provide the required pointer read"
    );

    assert_eq!(
        renderer.render(&invalid).message(),
        "the inline assembly contract is invalid for target 'x86_64-unknown-linux-gnu'"
    );

    let callback = renderer.render(&callback);

    assert_eq!(
        callback.message(),
        "callback state context references parameter 3, not the first parameter"
    );

    assert_eq!(
        callback.notes()[0].message(),
        "use the first context parameter of a trusted C or system ABI callable with a native symbol directive"
    );
}

#[test]
fn renderer_localizes_exact_storage_access_path() {
    let access = DiagnosticStorageAccess::new(
        DiagnosticStorageAccessPurpose::MutableBorrow,
        DiagnosticStorageRoot::Parameter,
        [
            DiagnosticStorageProjection::ProductField(String::from("payload")),
            DiagnosticStorageProjection::TupleElement(1),
        ],
        DiagnosticType::I32,
    );

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingMissingMutationAuthority,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::storage_access(access));

    assert_eq!(
        DiagnosticRenderer::english().render(&diagnostic).message(),
        "mutable borrow of parameter storage.payload.1 with type i32 has no mutable access to the reached storage"
    );
}

#[test]
fn renderer_formats_typed_arguments_for_utf8_diagnostics() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(2),
        DiagnosticKind::SourceInvalidUtf8,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::source_input(DiagnosticSourceInput::new(
        0,
        bray_source::SourceInputKind::VirtualText,
        DiagnosticSourceInputOrigin::Name("test source".to_owned()),
    )))
    .with_arg(DiagnosticArg::text_offset(TextSize::new(4)))
    .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

    let rendered = DiagnosticRenderer::new(DiagnosticLocale::English).render(&diagnostic);

    assert_eq!(
        rendered.message(),
        "virtual text source input 0 named 'test source' contains invalid UTF-8 starting at byte offset 4"
    );

    let [note] = rendered.notes() else {
        panic!("expected one rendered note: {rendered:?}");
    };

    assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Help);
    assert_eq!(note.message(), "source inputs must be valid UTF-8");
}
