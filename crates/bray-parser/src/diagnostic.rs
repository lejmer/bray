use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    SeverityKind,
};
use bray_source::{SourceSnapshot, SourceSpan, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

pub(crate) fn expected_token(
    snapshot: &SourceSnapshot,
    expected: SyntaxKind,
    actual: &SyntaxToken,
) -> Diagnostic {
    let span = SourceSpan::empty(snapshot.source_id(), actual.start());
    let expected_arg = DiagnosticArg::expected_syntax_kind(expected);

    diagnostic(span, DiagnosticKind::SyntaxExpectedToken)
        .with_arg(expected_arg.clone())
        .with_arg(DiagnosticArg::actual_syntax_kind(actual.kind()))
        .with_optional_arg(token_text_arg(snapshot, actual))
        .with_label(
            DiagnosticLabel::primary(DiagnosticLabelKind::ExpectedTokenInsertionPoint, span)
                .with_arg(expected_arg),
        )
}

#[allow(dead_code)]
pub(crate) fn unexpected_token(snapshot: &SourceSnapshot, actual: &SyntaxToken) -> Diagnostic {
    let span = SourceSpan::new(snapshot.source_id(), actual.range());
    let actual_arg = DiagnosticArg::actual_syntax_kind(actual.kind());

    diagnostic(span, DiagnosticKind::SyntaxUnexpectedToken)
        .with_arg(actual_arg.clone())
        .with_optional_arg(token_text_arg(snapshot, actual))
        .with_label(
            DiagnosticLabel::primary(DiagnosticLabelKind::UnexpectedToken, span)
                .with_arg(actual_arg),
        )
}

pub(crate) fn unexpected_eof(
    snapshot: &SourceSnapshot,
    expected: SyntaxKind,
    eof: &SyntaxToken,
) -> Diagnostic {
    let span = SourceSpan::empty(snapshot.source_id(), eof.start());
    let expected_arg = DiagnosticArg::expected_syntax_kind(expected);

    diagnostic(span, DiagnosticKind::SyntaxUnexpectedEof)
        .with_arg(expected_arg)
        .with_arg(DiagnosticArg::actual_syntax_kind(eof.kind()))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::UnexpectedEof,
            span,
        ))
}

#[allow(dead_code)]
pub(crate) fn skipped_syntax(snapshot: &SourceSnapshot, range: TextRange) -> Diagnostic {
    let span = SourceSpan::new(snapshot.source_id(), range);

    diagnostic(span, DiagnosticKind::SyntaxSkippedSyntax).with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::SkippedSyntax,
        span,
    ))
}

trait WithOptionalArg {
    fn with_optional_arg(self, arg: Option<DiagnosticArg>) -> Self;
}

impl WithOptionalArg for Diagnostic {
    fn with_optional_arg(self, arg: Option<DiagnosticArg>) -> Self {
        match arg {
            Some(arg) => self.with_arg(arg),
            None => self,
        }
    }
}

fn diagnostic(span: SourceSpan, kind: DiagnosticKind) -> Diagnostic {
    Diagnostic::new(diagnostic_id(span.start()), kind, SeverityKind::Error).with_primary_span(span)
}

fn diagnostic_id(start: TextSize) -> DiagnosticId {
    DiagnosticId::new(start.bytes())
}

fn token_text_arg(snapshot: &SourceSnapshot, token: &SyntaxToken) -> Option<DiagnosticArg> {
    let text = token.text(snapshot.text())?;

    if text.is_empty() {
        return None;
    }

    Some(DiagnosticArg::token_text(text))
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, DiagnosticLabelKind,
    };
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxToken};
    use bray_testing::test_source_snapshot as snapshot;

    use super::{expected_token, skipped_syntax, unexpected_eof, unexpected_token};

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
    }

    #[test]
    fn unexpected_token_diagnostic_covers_the_actual_token() {
        let snapshot = snapshot(";");
        let token = SyntaxToken::new(
            SyntaxKind::SemicolonToken,
            TextRange::new(TextSize::ZERO, TextSize::new(1)),
        );

        let diagnostic = unexpected_token(&snapshot, &token);

        assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxUnexpectedToken);
        assert_eq!(
            diagnostic.primary_span().map(|span| span.range()),
            Some(token.range())
        );

        let [label] = diagnostic.labels() else {
            panic!("expected one syntax label: {diagnostic:?}");
        };

        assert_eq!(label.kind(), DiagnosticLabelKind::UnexpectedToken);
        assert_eq!(label.span().range(), token.range());
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
    }

    #[test]
    fn skipped_syntax_diagnostic_covers_the_skipped_range() {
        let snapshot = snapshot("abc");
        let skipped = TextRange::new(TextSize::new(1), TextSize::new(3));

        let diagnostic = skipped_syntax(&snapshot, skipped);

        assert_eq!(diagnostic.kind(), DiagnosticKind::SyntaxSkippedSyntax);
        assert_eq!(
            diagnostic.primary_span(),
            Some(bray_source::SourceSpan::new(snapshot.source_id(), skipped))
        );

        let [label] = diagnostic.labels() else {
            panic!("expected one skipped-syntax label: {diagnostic:?}");
        };

        assert_eq!(label.kind(), DiagnosticLabelKind::SkippedSyntax);
        assert_eq!(label.span().range(), skipped);
    }
}
