use crate::node::define_source_syntax_node;
use crate::{BlockExpressionSyntax, IrrefutablePatternSyntax, SyntaxKind};

use super::looping::IterationSourceSyntax;

define_source_syntax_node! {
    /// Braced generator expression.
    pub struct GeneralGeneratorExpressionSyntax {
        builder: GeneralGeneratorExpressionSyntaxBuilder,
        kind: SyntaxKind::GeneralGeneratorExpression,
        source_slot: "general_generator_expression.source",
        node_name: "general generator expression",
        range_description: "general-generator-expression",
        debug_name: "GeneralGeneratorExpressionSyntax",
        builder_debug_name: "GeneralGeneratorExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "general_generator_expression.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "general_generator_expression.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the generator iteration child.
                generator_iteration_expression;
                /// Appends the generator iteration child.
                push_generator_iteration_expression;
                ty: GeneratorIterationExpressionSyntax;
                kind: SyntaxKind::GeneratorIterationExpression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Generator iteration expression.
    pub struct GeneratorIterationExpressionSyntax {
        builder: GeneratorIterationExpressionSyntaxBuilder,
        kind: SyntaxKind::GeneratorIterationExpression,
        source_slot: "generator_iteration_expression.source",
        node_name: "generator iteration expression",
        range_description: "generator-iteration-expression",
        debug_name: "GeneratorIterationExpressionSyntax",
        builder_debug_name: "GeneratorIterationExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `each` keyword token.
                each_keyword;
                /// Appends the `each` keyword token.
                push_each_keyword;
                kind: SyntaxKind::EachKeyword;
                slot: "generator_iteration_expression.each_keyword";
            },
            {
                /// Returns the required `in` keyword token.
                in_keyword;
                /// Appends the `in` keyword token.
                push_in_keyword;
                kind: SyntaxKind::InKeyword;
                slot: "generator_iteration_expression.in_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the iteration pattern child.
                irrefutable_pattern;
                /// Appends the iteration pattern child.
                push_irrefutable_pattern;
                ty: IrrefutablePatternSyntax;
                kind: SyntaxKind::IrrefutablePattern;
            },
            {
                /// Returns the iteration source child.
                iteration_source;
                /// Appends the iteration source child.
                push_iteration_source;
                ty: IterationSourceSyntax;
                kind: SyntaxKind::IterationSource;
            },
            {
                /// Returns the generator body block expression.
                block_expression;
                /// Appends the generator body block expression.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
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
        GeneralGeneratorExpressionSyntax, GeneratorIterationExpressionSyntax,
        IrrefutablePatternSyntax, IterationSourceSyntax, SyntaxKind, SyntaxText,
    };

    #[test]
    fn generator_expressions_store_iteration_children() {
        let snapshot = test_snapshot(
            "syntax-generator-expression-test",
            "{ each item in items {} }",
        );

        let mut pattern = IrrefutablePatternSyntax::builder(snapshot.clone(), TextSize::new(7));
        let mut source = IterationSourceSyntax::builder(snapshot.clone(), TextSize::new(15));

        let mut iteration =
            GeneratorIterationExpressionSyntax::builder(snapshot.clone(), TextSize::new(2));

        let mut generator =
            GeneralGeneratorExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        pattern.push_path(identifier_path_with_trailing_space(snapshot.clone(), 7, 11));

        source.push_expression(token_expression_from_token(
            snapshot.clone(),
            keyword(SyntaxKind::IdentifierToken, 15, 20, true),
        ));

        iteration.push_each_keyword(keyword(SyntaxKind::EachKeyword, 2, 6, true));
        iteration.push_irrefutable_pattern(pattern.build());
        iteration.push_in_keyword(keyword(SyntaxKind::InKeyword, 12, 14, true));
        iteration.push_iteration_source(source.build());
        iteration.push_block_expression(block_expression(snapshot.clone(), 21, true));

        generator.push_open_brace_token(keyword(SyntaxKind::OpenBraceToken, 0, 1, true));
        generator.push_generator_iteration_expression(iteration.build());
        generator.push_close_brace_token(crate::test_support::token(
            SyntaxKind::CloseBraceToken,
            24,
            25,
        ));

        let generator = generator.build();

        assert_eq!(generator.full_text(), "{ each item in items {} }");

        assert_eq!(
            generator.generator_iteration_expression().full_text(),
            "each item in items {} "
        );
    }
}
