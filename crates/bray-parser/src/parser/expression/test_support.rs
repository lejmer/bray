use bray_diagnostics::DiagnosticBag;
use bray_syntax::{ExpressionSyntax, PrimaryExpressionSyntax, SyntaxKind, SyntaxText};
use bray_testing::test_source_store as source_store;

use crate::test_support::source;

use super::super::state::Parser;

pub(super) fn assert_expression_cases_until_semicolon_for_test(cases: &[(&str, &str)]) {
    assert_expression_cases_until_kind_for_test(cases, SyntaxKind::SemicolonToken);
}

pub(super) fn assert_expression_cases_to_eof_for_test(cases: &[(&str, &str)]) {
    assert_expression_cases_until_kind_for_test(cases, SyntaxKind::EndOfFileToken);
}

fn assert_expression_cases_until_kind_for_test(cases: &[(&str, &str)], boundary: SyntaxKind) {
    for (source_text, expected_text) in cases {
        let (expression, diagnostics) = parse_expression_until_kind_for_test(source_text, boundary);

        assert_eq!(expression.full_text(), *expected_text, "{source_text}");
        assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
    }
}

pub(super) fn parse_expression_until_semicolon_for_test(
    source_text: &str,
) -> (ExpressionSyntax, DiagnosticBag) {
    parse_expression_until_kind_for_test(source_text, SyntaxKind::SemicolonToken)
}

pub(super) fn parse_expression_to_eof_for_test(
    source_text: &str,
) -> (ExpressionSyntax, DiagnosticBag) {
    parse_expression_until_kind_for_test(source_text, SyntaxKind::EndOfFileToken)
}

fn parse_expression_until_kind_for_test(
    source_text: &str,
    boundary: SyntaxKind,
) -> (ExpressionSyntax, DiagnosticBag) {
    let sources = source_store([source_text]);
    let snapshot = source(&sources, 0);

    let mut parser = Parser::new(snapshot);
    let mut at_boundary = |parser: &mut Parser| parser.at(boundary);

    let expression = parser.parse_expression_until(&mut at_boundary);
    let diagnostics = parser.finish();

    (expression, diagnostics)
}

pub(super) fn primary_contains_child_kind_for_test(
    primary: &PrimaryExpressionSyntax,
    kind: SyntaxKind,
) -> bool {
    match kind {
        SyntaxKind::AbsenceExpression => primary.absence_expressions().next().is_some(),
        SyntaxKind::ArrayExpression => primary.array_expressions().next().is_some(),
        SyntaxKind::AssertionExpression => primary.assertion_expressions().next().is_some(),
        SyntaxKind::AwaitExpression => primary.await_expressions().next().is_some(),
        SyntaxKind::BooleanFoldExpression => primary.boolean_fold_expressions().next().is_some(),
        SyntaxKind::BorrowExpression => primary.borrow_expressions().next().is_some(),
        SyntaxKind::CatchExpression => primary.catch_expressions().next().is_some(),
        SyntaxKind::GroupedExpression => primary.grouped_expressions().next().is_some(),
        SyntaxKind::LeadingDotVariantExpression => {
            primary.leading_dot_variant_expressions().next().is_some()
        }
        SyntaxKind::LiteralExpression => primary.literal_expressions().next().is_some(),
        SyntaxKind::ResultPropagationExpression => {
            primary.result_propagation_expressions().next().is_some()
        }
        SyntaxKind::StructConstructionBody => primary.struct_construction_bodies().next().is_some(),
        SyntaxKind::TrustBoundaryExpression => {
            primary.trust_boundary_expressions().next().is_some()
        }
        SyntaxKind::TupleExpression => primary.tuple_expressions().next().is_some(),
        SyntaxKind::TypeFormConstructionExpression => primary
            .type_form_construction_expressions()
            .next()
            .is_some(),
        SyntaxKind::UnitExpression => primary.unit_expressions().next().is_some(),
        _ => false,
    }
}
