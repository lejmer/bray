use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Literal expression.
    pub struct LiteralExpressionSyntax {
        builder: LiteralExpressionSyntaxBuilder,
        kind: SyntaxKind::LiteralExpression,
        source_slot: "literal_expression.source",
        node_name: "literal expression",
        range_description: "literal-expression",
        debug_name: "LiteralExpressionSyntax",
        builder_debug_name: "LiteralExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl LiteralExpressionSyntax {
    /// Returns the literal token.
    pub fn literal_token(&self) -> Option<SyntaxToken> {
        self.tokens().next()
    }
}

impl LiteralExpressionSyntaxBuilder {
    /// Appends a literal token.
    pub fn push_literal_token(&mut self, token: SyntaxToken) {
        assert!(
            is_literal_expression_token(token.kind()),
            "literal_expression.literal_token expected a literal token"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Unit expression.
    pub struct UnitExpressionSyntax {
        builder: UnitExpressionSyntaxBuilder,
        kind: SyntaxKind::UnitExpression,
        source_slot: "unit_expression.source",
        node_name: "unit expression",
        range_description: "unit-expression",
        debug_name: "UnitExpressionSyntax",
        builder_debug_name: "UnitExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required `unit` keyword token.
                unit_keyword;
                /// Appends the `unit` keyword token.
                push_unit_keyword;
                kind: SyntaxKind::UnitKeyword;
                slot: "unit_expression.unit_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Absence expression.
    pub struct AbsenceExpressionSyntax {
        builder: AbsenceExpressionSyntaxBuilder,
        kind: SyntaxKind::AbsenceExpression,
        source_slot: "absence_expression.source",
        node_name: "absence expression",
        range_description: "absence-expression",
        debug_name: "AbsenceExpressionSyntax",
        builder_debug_name: "AbsenceExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required `none` keyword token.
                none_keyword;
                /// Appends the `none` keyword token.
                push_none_keyword;
                kind: SyntaxKind::NoneKeyword;
                slot: "absence_expression.none_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Leading-dot variant expression.
    pub struct LeadingDotVariantExpressionSyntax {
        builder: LeadingDotVariantExpressionSyntaxBuilder,
        kind: SyntaxKind::LeadingDotVariantExpression,
        source_slot: "leading_dot_variant_expression.source",
        node_name: "leading-dot variant expression",
        range_description: "leading-dot-variant-expression",
        debug_name: "LeadingDotVariantExpressionSyntax",
        builder_debug_name: "LeadingDotVariantExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required dot token.
                dot_token;
                /// Appends the dot token.
                push_dot_token;
                kind: SyntaxKind::DotToken;
                slot: "leading_dot_variant_expression.dot_token";
            },
            {
                /// Returns the required variant name token.
                identifier_token;
                /// Appends the variant name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "leading_dot_variant_expression.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

fn is_literal_expression_token(kind: SyntaxKind) -> bool {
    kind.is_literal() || matches!(kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
}
