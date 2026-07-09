use bray_diagnostics::DiagnosticBag;
use bray_syntax::{ExpressionSyntax, SyntaxKind, SyntaxText};
use bray_testing::test_source_store as source_store;

use crate::test_support::source;

use super::super::state::Parser;

pub(super) fn assert_expression_cases_until_semicolon_for_test(cases: &[(&str, &str)]) {
    for (source_text, expected_text) in cases {
        let (expression, diagnostics) = parse_expression_until_semicolon_for_test(source_text);

        assert_eq!(expression.full_text(), *expected_text, "{source_text}");
        assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
    }
}

pub(super) fn parse_expression_until_semicolon_for_test(
    source_text: &str,
) -> (ExpressionSyntax, DiagnosticBag) {
    let sources = source_store([source_text]);
    let snapshot = source(&sources, 0);

    let mut parser = Parser::new(snapshot);
    let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

    let expression = parser.parse_expression_until(&mut boundary);
    let diagnostics = parser.finish();

    (expression, diagnostics)
}
