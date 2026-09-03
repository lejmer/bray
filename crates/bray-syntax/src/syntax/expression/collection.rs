use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, GeneratorIterationExpressionSyntax, SyntaxKind};

define_source_syntax_node! {
    /// Parenthesized grouped expression.
    pub struct GroupedExpressionSyntax {
        builder: GroupedExpressionSyntaxBuilder,
        kind: SyntaxKind::GroupedExpression,
        source_slot: "grouped_expression.source",
        node_name: "grouped expression",
        range_description: "grouped-expression",
        debug_name: "GroupedExpressionSyntax",
        builder_debug_name: "GroupedExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "grouped_expression.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "grouped_expression.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the grouped expression child.
                expression;
                /// Appends the grouped expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Tuple expression.
    pub struct TupleExpressionSyntax {
        builder: TupleExpressionSyntaxBuilder,
        kind: SyntaxKind::TupleExpression,
        source_slot: "tuple_expression.source",
        node_name: "tuple expression",
        range_description: "tuple-expression",
        debug_name: "TupleExpressionSyntax",
        builder_debug_name: "TupleExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "tuple_expression.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "tuple_expression.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "tuple_expression.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns tuple element expressions in source order.
                expressions;
                /// Appends a tuple element expression.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for TupleExpressionSyntax {}

define_source_syntax_node! {
    /// Array expression.
    pub struct ArrayExpressionSyntax {
        builder: ArrayExpressionSyntaxBuilder,
        kind: SyntaxKind::ArrayExpression,
        source_slot: "array_expression.source",
        node_name: "array expression",
        range_description: "array-expression",
        debug_name: "ArrayExpressionSyntax",
        builder_debug_name: "ArrayExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "array_expression.open_bracket_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "array_expression.close_bracket_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "array_expression.comma_token";
            },
            {
                /// Returns the repeated-array semicolon token.
                semicolon_token;
                /// Appends the repeated-array semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "array_expression.semicolon_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns array expressions in source order.
                expressions;
                /// Appends an array expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            },
            {
                /// Returns array generator-iteration children in source order.
                generator_iteration_expressions;
                /// Appends an array generator-iteration child.
                push_generator_iteration_expression;
                ty: GeneratorIterationExpressionSyntax;
                kind: SyntaxKind::GeneratorIterationExpression;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for ArrayExpressionSyntax {}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{snapshot as test_snapshot, token};
    use crate::{
        ExpressionSyntax, LiteralExpressionSyntax, PrimaryExpressionSyntax, SyntaxKind, SyntaxText,
        TupleExpressionSyntax,
    };

    #[test]
    fn tuple_expressions_store_typed_children() {
        let tuple_snapshot = test_snapshot("syntax-tuple-expression-test", "(true,)");

        let mut literal =
            LiteralExpressionSyntax::builder(tuple_snapshot.clone(), TextSize::new(1));

        let mut tuple = TupleExpressionSyntax::builder(tuple_snapshot.clone(), TextSize::ZERO);

        literal.push_literal_token(token(SyntaxKind::TrueKeyword, 1, 5));

        let mut literal_primary =
            PrimaryExpressionSyntax::builder(tuple_snapshot.clone(), TextSize::new(1));

        literal_primary.push_literal_expression(literal.build());

        let mut literal_expression = ExpressionSyntax::builder(tuple_snapshot, TextSize::new(1));

        literal_expression.push_primary_expression(literal_primary.build());

        tuple.push_open_paren_token(token(SyntaxKind::OpenParenToken, 0, 1));
        tuple.push_expression(literal_expression.build());
        tuple.push_separator_token(token(SyntaxKind::CommaToken, 5, 6));
        tuple.push_close_paren_token(token(SyntaxKind::CloseParenToken, 6, 7));

        assert_eq!(tuple.build().full_text(), "(true,)");
    }
}
