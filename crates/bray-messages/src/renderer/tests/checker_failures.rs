use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticCheckerFailure, DiagnosticCheckerNode,
    DiagnosticCheckerSymbol, DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
    DiagnosticExpressionCategory, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticStorageFlowFailure,
    SeverityKind,
};
use bray_source::{SourceId, SourceSpan, TextRange, TextSize};

use crate::RenderedDiagnosticNoteKind;
use crate::catalog::{INTERNAL_COMPILER_ERROR, forbidden_internal_term};
use crate::renderer::DiagnosticRenderer;

#[test]
fn checker_compiler_defects_hide_internal_identities_and_retain_source_context() {
    use DiagnosticCheckerFailure as CheckerFailure;
    use DiagnosticStorageFlowFailure as StorageFlowFailure;

    let expression = DiagnosticCheckerNode::new("expression", 17, 23);
    let block = DiagnosticCheckerNode::new("block", 19, 29);
    let pattern = DiagnosticCheckerNode::new("pattern", 31, 37);
    let callable = DiagnosticCheckerSymbol::new("callable_overload", 41);

    let cases = [
        CheckerFailure::StorageFlow(StorageFlowFailure::FlowConstruction("duplicate_suspension")),
        CheckerFailure::StorageFlow(StorageFlowFailure::FlowConstruction("future_reason")),
        CheckerFailure::StorageFlow(StorageFlowFailure::ForeignDependencyContract),
        CheckerFailure::StorageFlow(StorageFlowFailure::UnresolvedDependencyWitness { expression }),
        CheckerFailure::StorageFlow(StorageFlowFailure::DependencyContractsConstruction(
            "invalid_borrow",
        )),
        CheckerFailure::StorageFlow(StorageFlowFailure::DependencyContractsConstruction(
            "future_reason",
        )),
        CheckerFailure::StorageFlow(StorageFlowFailure::AsyncConstruction("foreign_unit")),
        CheckerFailure::StorageFlow(StorageFlowFailure::AsyncConstruction("future_reason")),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingAwaitDependencyContract {
            expression,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingDependencyContract {
            expression,
            contract_unit: 43,
            contract: 47,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::CallableParameterCountMismatch {
            callable,
            signature_parameters: 3,
            type_parameters: 2,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::CallableTypeNotCallable { callable }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingBorrowCapability {
            unit: 53,
            borrow: 59,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingExitOrigin { exit: expression }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingBlock { block }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingStorageAccess {
            unit: 61,
            access: 67,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingStorageIdentity {
            unit: 71,
            identity: 73,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingStorageSymbolName {
            symbol: callable,
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::UnbalancedScopes {
            open_scope: Some(block),
        }),
        CheckerFailure::StorageFlow(StorageFlowFailure::UnbalancedScopes { open_scope: None }),
        CheckerFailure::StorageFlow(StorageFlowFailure::MissingPattern { pattern }),
        CheckerFailure::InvalidStorageOperation {
            expression,
            access: 79,
            status: "conflicting_borrow",
        },
        CheckerFailure::InvalidStorageOperation {
            expression,
            access: 83,
            status: "future_status",
        },
    ];

    let span = SourceSpan::new(
        SourceId::new(5),
        TextRange::new(TextSize::new(8), TextSize::new(13)),
    );

    for (index, failure) in cases.into_iter().enumerate() {
        let id = u32::try_from(index)
            .map(DiagnosticId::new)
            .unwrap_or_else(|_| panic!("checker failure inventory must fit diagnostic IDs"));

        let diagnostic = Diagnostic::new(
            id,
            DiagnosticKind::CheckingCompilerDefect,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(DiagnosticArg::emission_failure(
            DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::Checker(
                failure,
            )),
        ))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::CompilerDefectSource,
            span,
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.primary_span(), Some(span));

        assert!(rendered.message().starts_with(INTERNAL_COMPILER_ERROR));

        let [label] = rendered.labels() else {
            panic!("expected one compiler-defect source label: {rendered:?}");
        };

        assert_eq!(label.span(), span);

        assert!(!label.message().is_empty());

        let [note] = rendered.notes() else {
            panic!("expected one compiler-defect reporting note: {rendered:?}");
        };

        assert_eq!(note.kind(), DiagnosticNoteKind::ReportCompilerDefect);
        assert_eq!(note.rendered_kind(), RenderedDiagnosticNoteKind::Note);

        assert!(!note.message().is_empty());

        assert_eq!(forbidden_internal_term(rendered.message()), None);
        assert!(!rendered.message().contains('#'));
    }
}

#[test]
fn renderer_localizes_semantic_analysis_limits() {
    let recursion = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingTypeRepresentationRecursionLimitExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_count(18))
    .with_arg(DiagnosticArg::maximum_count(17));

    assert_eq!(
        DiagnosticRenderer::english().render(&recursion).message(),
        "type representation analysis required 18 nested declarations but the limit is 17"
    );

    let cases = [
        (
            DiagnosticKind::CheckingImplementationCoherenceLimitExceeded,
            "implementation coherence comparison 18 exceeds the configured limit of 17",
        ),
        (
            DiagnosticKind::CheckingCallableOverloadLimitExceeded,
            "callable overload comparison 18 exceeds the configured limit of 17",
        ),
    ];

    for (kind, expected) in cases {
        let diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_arg(DiagnosticArg::actual_count(18))
            .with_arg(DiagnosticArg::maximum_count(17));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), expected);
    }

    let singular = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingCallableOverloadLimitExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_count(2))
    .with_arg(DiagnosticArg::maximum_count(1));

    assert_eq!(
        DiagnosticRenderer::english().render(&singular).message(),
        "callable overload comparison 2 exceeds the configured limit of 1"
    );
}

#[test]
fn trusted_contract_errors_identify_the_operation_and_correction() {
    let span = SourceSpan::new(
        SourceId::new(0),
        TextRange::new(TextSize::new(10), TextSize::new(24)),
    );

    for (kind, category, note, expected) in [
        (
            DiagnosticKind::CheckingTrustedObligationNotProven,
            DiagnosticExpressionCategory::Call,
            DiagnosticNoteKind::TrustedObligationEvidenceRequired,
            "this call requires a trusted condition that is not established by live evidence",
        ),
        (
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
            DiagnosticExpressionCategory::NameReference,
            DiagnosticNoteKind::TrustedWitnessTransferRequired,
            "this name reference would copy or separate a value without preserving its trusted guarantees",
        ),
    ] {
        let diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
            .with_primary_span(span)
            .with_arg(DiagnosticArg::expression_category(category))
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::TrustedObligationFailure,
                span,
            ))
            .with_label(
                DiagnosticLabel::secondary(DiagnosticLabelKind::TrustedCallable, span)
                    .with_arg(DiagnosticArg::declaration_name("observe")),
            )
            .with_note(DiagnosticNote::new(note));

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert_eq!(rendered.message(), expected);
        assert_eq!(rendered.primary_span(), Some(span));

        assert_eq!(
            rendered.labels()[1].message(),
            "trusted requirement of 'observe'"
        );

        assert_eq!(
            rendered.notes()[0].rendered_kind(),
            RenderedDiagnosticNoteKind::Help
        );

        assert!(
            rendered.notes()[0].message().contains("preserv")
                || rendered.notes()[0]
                    .message()
                    .contains("enclosing declaration")
        );

        assert!(forbidden_internal_term(rendered.message()).is_none());
    }
}
