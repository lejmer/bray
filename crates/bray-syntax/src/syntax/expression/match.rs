use crate::node::define_source_syntax_node;
use crate::{BlockExpressionSyntax, CasePatternSyntax, ExpressionSyntax, SyntaxKind};

use super::root::first_expression;

define_source_syntax_node! {
    /// Match expression.
    pub struct MatchExpressionSyntax {
        builder: MatchExpressionSyntaxBuilder,
        kind: SyntaxKind::MatchExpression,
        source_slot: "match_expression.source",
        node_name: "match expression",
        range_description: "match-expression",
        debug_name: "MatchExpressionSyntax",
        builder_debug_name: "MatchExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `match` keyword token.
                match_keyword;
                /// Appends the `match` keyword token.
                push_match_keyword;
                kind: SyntaxKind::MatchKeyword;
                slot: "match_expression.match_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the match subject child.
                match_subject;
                /// Appends the match subject child.
                push_match_subject;
                ty: MatchSubjectSyntax;
                kind: SyntaxKind::MatchSubject;
            },
            {
                /// Returns the match body child.
                match_body;
                /// Appends the match body child.
                push_match_body;
                ty: MatchBodySyntax;
                kind: SyntaxKind::MatchBody;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Match subject.
    pub struct MatchSubjectSyntax {
        builder: MatchSubjectSyntaxBuilder,
        kind: SyntaxKind::MatchSubject,
        source_slot: "match_subject.source",
        node_name: "match subject",
        range_description: "match-subject",
        debug_name: "MatchSubjectSyntax",
        builder_debug_name: "MatchSubjectSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional `consume` keyword token.
                consume_keyword;
                /// Appends the `consume` keyword token.
                push_consume_keyword;
                kind: SyntaxKind::ConsumeKeyword;
                slot: "match_subject.consume_keyword";
            }
        ],
        required_children: [
            {
                /// Returns the subject expression child.
                expression;
                /// Appends the subject expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Match body.
    pub struct MatchBodySyntax {
        builder: MatchBodySyntaxBuilder,
        kind: SyntaxKind::MatchBody,
        source_slot: "match_body.source",
        node_name: "match body",
        range_description: "match-body",
        debug_name: "MatchBodySyntax",
        builder_debug_name: "MatchBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "match_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "match_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns match-arm children in source order.
                match_arms;
                /// Appends a match-arm child.
                push_match_arm;
                ty: MatchArmSyntax;
                kind: SyntaxKind::MatchArm;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Match arm.
    pub struct MatchArmSyntax {
        builder: MatchArmSyntaxBuilder,
        kind: SyntaxKind::MatchArm,
        source_slot: "match_arm.source",
        node_name: "match arm",
        range_description: "match-arm",
        debug_name: "MatchArmSyntax",
        builder_debug_name: "MatchArmSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `case` keyword token.
                case_keyword;
                /// Appends the `case` keyword token.
                push_case_keyword;
                kind: SyntaxKind::CaseKeyword;
                slot: "match_arm.case_keyword";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional `when` guard keyword token.
                when_keyword;
                /// Appends the `when` guard keyword token.
                push_when_keyword;
                kind: SyntaxKind::WhenKeyword;
                slot: "match_arm.when_keyword";
            }
        ],
        required_children: [
            {
                /// Returns the case pattern child.
                case_pattern;
                /// Appends the case pattern child.
                push_case_pattern;
                ty: CasePatternSyntax;
                kind: SyntaxKind::CasePattern;
            },
            {
                /// Returns the arm body block expression.
                block_expression;
                /// Appends the arm body block expression.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
        repeated_children: [
            {
                /// Returns guard expression children in source order.
                expressions;
                /// Appends a guard expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl MatchArmSyntax {
    /// Returns the optional guard expression child.
    pub fn guard_expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{
        block_expression, identifier_path_with_trailing_space, keyword, snapshot as test_snapshot,
        token_expression_from_token,
    };
    use crate::{
        CasePatternSyntax, MatchArmSyntax, MatchBodySyntax, MatchExpressionSyntax,
        MatchSubjectSyntax, SyntaxKind, SyntaxText,
    };

    #[test]
    fn match_expressions_store_subject_body_and_arms() {
        let snapshot = test_snapshot(
            "syntax-match-expression-test",
            "match value { case item {} }",
        );

        let mut subject = MatchSubjectSyntax::builder(snapshot.clone(), TextSize::new(6));
        let mut pattern = CasePatternSyntax::builder(snapshot.clone(), TextSize::new(19));
        let mut arm = MatchArmSyntax::builder(snapshot.clone(), TextSize::new(14));
        let mut body = MatchBodySyntax::builder(snapshot.clone(), TextSize::new(12));
        let mut expression = MatchExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        subject.push_expression(token_expression_from_token(
            snapshot.clone(),
            keyword(SyntaxKind::IdentifierToken, 6, 11, true),
        ));

        pattern.push_path(identifier_path_with_trailing_space(
            snapshot.clone(),
            19,
            23,
        ));

        arm.push_case_keyword(keyword(SyntaxKind::CaseKeyword, 14, 18, true));
        arm.push_case_pattern(pattern.build());
        arm.push_block_expression(block_expression(snapshot.clone(), 24, true));

        body.push_open_brace_token(keyword(SyntaxKind::OpenBraceToken, 12, 13, true));
        body.push_match_arm(arm.build());
        body.push_close_brace_token(crate::test_support::token(
            SyntaxKind::CloseBraceToken,
            27,
            28,
        ));

        expression.push_match_keyword(keyword(SyntaxKind::MatchKeyword, 0, 5, true));
        expression.push_match_subject(subject.build());
        expression.push_match_body(body.build());

        let expression = expression.build();

        assert_eq!(expression.full_text(), "match value { case item {} }");
        assert_eq!(expression.match_body().match_arms().count(), 1);
    }
}
