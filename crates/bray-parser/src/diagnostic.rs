use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticSourceEdit, DiagnosticSuggestion, DiagnosticSuggestionKind, SeverityKind,
};
use bray_source::{SourceSnapshot, SourceSpan, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

pub(crate) fn expected_token(
    snapshot: &SourceSnapshot,
    expected: SyntaxKind,
    actual: &SyntaxToken,
) -> Diagnostic {
    let span = SourceSpan::empty(snapshot.source_id(), actual.start());

    let mut diagnostic = expected_token_at(
        snapshot,
        expected,
        actual,
        span,
        DiagnosticLabelKind::ExpectedTokenInsertionPoint,
    );

    if let Some(suggestion) = insertion_suggestion(expected, span) {
        diagnostic = diagnostic.with_suggestion(suggestion);
    }

    diagnostic
}

pub(crate) fn expected_operator(
    snapshot: &SourceSnapshot,
    expected: SyntaxKind,
    actual: &SyntaxToken,
) -> Diagnostic {
    expected_token_at(
        snapshot,
        expected,
        actual,
        SourceSpan::new(snapshot.source_id(), actual.range()),
        DiagnosticLabelKind::InvalidOperatorOrPunctuation,
    )
}

fn expected_token_at(
    snapshot: &SourceSnapshot,
    expected: SyntaxKind,
    actual: &SyntaxToken,
    span: SourceSpan,
    label: DiagnosticLabelKind,
) -> Diagnostic {
    let expected_arg = DiagnosticArg::expected_syntax_kind(expected);

    diagnostic(span, DiagnosticKind::SyntaxExpectedToken)
        .with_arg(expected_arg.clone())
        .with_arg(DiagnosticArg::actual_syntax_kind(actual.kind()))
        .with_optional_arg(token_text_arg(snapshot, actual))
        .with_label(DiagnosticLabel::primary(label, span).with_arg(expected_arg))
}

pub(crate) fn unexpected_eof(
    snapshot: &SourceSnapshot,
    expected: SyntaxKind,
    eof: &SyntaxToken,
) -> Diagnostic {
    let span = SourceSpan::empty(snapshot.source_id(), eof.start());
    let expected_arg = DiagnosticArg::expected_syntax_kind(expected);

    let mut diagnostic = diagnostic(span, DiagnosticKind::SyntaxUnexpectedEof)
        .with_arg(expected_arg)
        .with_arg(DiagnosticArg::actual_syntax_kind(eof.kind()))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::UnexpectedEof,
            span,
        ));

    if let Some(suggestion) = insertion_suggestion(expected, span) {
        diagnostic = diagnostic.with_suggestion(suggestion);
    }

    diagnostic
}

pub(crate) fn expected_expression(snapshot: &SourceSnapshot, actual: &SyntaxToken) -> Diagnostic {
    let span = SourceSpan::new(snapshot.source_id(), actual.range());
    let expected_arg = DiagnosticArg::expected_syntax_kind(SyntaxKind::Expression);

    diagnostic(span, DiagnosticKind::SyntaxExpectedExpression)
        .with_arg(expected_arg.clone())
        .with_arg(DiagnosticArg::actual_syntax_kind(actual.kind()))
        .with_optional_arg(token_text_arg(snapshot, actual))
        .with_label(
            DiagnosticLabel::primary(DiagnosticLabelKind::ExpectedExpression, span)
                .with_arg(expected_arg),
        )
}

pub(crate) fn nesting_limit_exceeded(
    snapshot: &SourceSnapshot,
    actual: &SyntaxToken,
    maximum_depth: usize,
) -> Diagnostic {
    let span = SourceSpan::new(snapshot.source_id(), actual.range());
    let maximum_depth = u64::try_from(maximum_depth).unwrap_or(u64::MAX);

    diagnostic(span, DiagnosticKind::SyntaxNestingLimitExceeded)
        .with_arg(DiagnosticArg::maximum_count(maximum_depth))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::NestingLimitExceeded,
            span,
        ))
}

pub(crate) fn invalid_directive_target(
    snapshot: &SourceSnapshot,
    marker: &SyntaxToken,
    name: &SyntaxToken,
    directive_kind: SyntaxKind,
) -> Diagnostic {
    let span = SourceSpan::new(snapshot.source_id(), marker.range().cover(name.range()));

    diagnostic(span, DiagnosticKind::SyntaxInvalidDirectiveTarget)
        .with_arg(DiagnosticArg::directive_kind(directive_kind))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidDeclaration,
            span,
        ))
}

fn diagnostic(span: SourceSpan, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(diagnostic_id(span.start()), kind, SeverityKind::Error).with_primary_span(span)
}

pub(crate) fn diagnostic_id(start: TextSize) -> DiagnosticId {
    DiagnosticId::new(start.bytes())
}

fn token_text_arg(snapshot: &SourceSnapshot, token: &SyntaxToken) -> Option<DiagnosticArg> {
    let text = token.text(snapshot.text())?;

    if text.is_empty() {
        return None;
    }

    Some(DiagnosticArg::token_text(text))
}

fn insertion_suggestion(expected: SyntaxKind, span: SourceSpan) -> Option<DiagnosticSuggestion> {
    let replacement = expected.fixed_text()?;

    Some(
        DiagnosticSuggestion::maybe_edit(
            DiagnosticSuggestionKind::InsertExpectedSyntax,
            DiagnosticSourceEdit::new(span, replacement),
        )
        .with_arg(DiagnosticArg::expected_syntax_kind(expected)),
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind};
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxToken};
    use bray_testing::test_source_snapshot as snapshot;

    use super::{
        expected_expression, expected_token, invalid_directive_target, nesting_limit_exceeded,
        unexpected_eof,
    };

    #[test]
    fn unexpected_condition_operator_does_not_suggest_inserting_before_it() {
        let snapshot = snapshot("||");

        let token = SyntaxToken::new(
            SyntaxKind::PipePipeToken,
            TextRange::new(TextSize::ZERO, TextSize::new(2)),
        );

        let diagnostic =
            super::expected_operator(&snapshot, SyntaxKind::AmpersandAmpersandToken, &token);

        assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxExpectedToken);

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::new(
                snapshot.source_id(),
                token.range()
            ))
        );

        assert!(diagnostic.suggestions().is_empty());

        assert!(
            diagnostic
                .args()
                .contains(&DiagnosticArg::expected_syntax_kind(
                    SyntaxKind::AmpersandAmpersandToken
                ))
        );

        assert!(
            diagnostic
                .args()
                .contains(&DiagnosticArg::actual_syntax_kind(
                    SyntaxKind::PipePipeToken
                ))
        );
    }

    #[test]
    fn expected_token_diagnostic_uses_an_insertion_point_span() {
        let snapshot = snapshot("main");

        let token = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::ZERO, TextSize::new(4)),
        );

        let diagnostic = expected_token(&snapshot, SyntaxKind::FuncKeyword, &token);

        assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxExpectedToken);

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::empty(
                snapshot.source_id(),
                TextSize::ZERO
            ))
        );

        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword),
                DiagnosticArg::actual_syntax_kind(SyntaxKind::IdentifierToken),
                DiagnosticArg::new(
                    DiagnosticArgName::TokenText,
                    DiagnosticArgValue::TokenText(String::from("main"))
                ),
            ]
        );

        let [suggestion] = diagnostic.suggestions() else {
            panic!("expected one exact insertion suggestion: {diagnostic:?}");
        };

        let [edit] = suggestion.edits() else {
            panic!("expected one insertion edit: {suggestion:?}");
        };

        assert_eq!(
            edit.span(),
            diagnostic.primary_span().expect("test span exists")
        );

        assert_eq!(edit.replacement(), "func");
    }

    #[test]
    fn unexpected_eof_diagnostic_uses_the_eof_insertion_point() {
        let snapshot = snapshot("main");
        let eof = SyntaxToken::end_of_file(TextSize::new(4));

        let diagnostic = unexpected_eof(&snapshot, SyntaxKind::CloseBraceToken, &eof);

        assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxUnexpectedEof);

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::empty(
                snapshot.source_id(),
                TextSize::new(4)
            ))
        );

        let [suggestion] = diagnostic.suggestions() else {
            panic!("expected one exact EOF insertion suggestion: {diagnostic:?}");
        };

        assert_eq!(suggestion.edits()[0].replacement(), "}");
    }

    #[test]
    fn non_fixed_expected_syntax_is_actionable_without_an_insertion() {
        let snapshot = snapshot("value");

        let token = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::ZERO, TextSize::new(5)),
        );

        let expected = expected_token(&snapshot, SyntaxKind::Expression, &token);

        assert!(expected.suggestions().is_empty());
        bray_testing::assert_goal_state_diagnostic(&expected);

        let eof = SyntaxToken::end_of_file(TextSize::new(5));
        let unexpected = unexpected_eof(&snapshot, SyntaxKind::IdentifierToken, &eof);

        assert!(unexpected.suggestions().is_empty());
        bray_testing::assert_goal_state_diagnostic(&unexpected);
    }

    #[test]
    fn invalid_directive_targets_cover_the_directive_name() {
        let snapshot = snapshot("@link");

        let marker = SyntaxToken::new(
            SyntaxKind::AtToken,
            TextRange::new(TextSize::ZERO, TextSize::new(1)),
        );

        let name = SyntaxToken::new(
            SyntaxKind::IdentifierToken,
            TextRange::new(TextSize::new(1), TextSize::new(5)),
        );

        let diagnostic =
            invalid_directive_target(&snapshot, &marker, &name, SyntaxKind::LinkDirective);

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::new(
                snapshot.source_id(),
                TextRange::new(TextSize::ZERO, TextSize::new(5))
            ))
        );

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::SyntaxInvalidDirectiveTarget
        );

        bray_testing::assert_goal_state_diagnostic(&diagnostic);
    }

    #[test]
    fn expected_expression_diagnostic_covers_the_actual_token() {
        let snapshot = snapshot("@value");

        let token = SyntaxToken::new(
            SyntaxKind::AtToken,
            TextRange::new(TextSize::ZERO, TextSize::new(1)),
        );

        let diagnostic = expected_expression(&snapshot, &token);

        assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxExpectedExpression);

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::new(
                snapshot.source_id(),
                TextRange::new(TextSize::ZERO, TextSize::new(1))
            ))
        );

        assert_eq!(
            diagnostic.args(),
            &[
                DiagnosticArg::expected_syntax_kind(SyntaxKind::Expression),
                DiagnosticArg::actual_syntax_kind(SyntaxKind::AtToken),
                DiagnosticArg::new(
                    DiagnosticArgName::TokenText,
                    DiagnosticArgValue::TokenText(String::from("@"))
                ),
            ]
        );
    }

    #[test]
    fn nesting_limit_diagnostic_keeps_the_limit_typed() {
        let snapshot = snapshot("(");

        let token = SyntaxToken::new(
            SyntaxKind::OpenParenToken,
            TextRange::new(TextSize::ZERO, TextSize::new(1)),
        );

        let diagnostic = nesting_limit_exceeded(&snapshot, &token, 128);

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::SyntaxNestingLimitExceeded
        );

        assert_eq!(
            diagnostic.args(),
            &[DiagnosticArg::new(
                DiagnosticArgName::MaximumCount,
                DiagnosticArgValue::Count(128)
            )]
        );

        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::new(
                snapshot.source_id(),
                TextRange::new(TextSize::ZERO, TextSize::new(1))
            ))
        );
    }
}
