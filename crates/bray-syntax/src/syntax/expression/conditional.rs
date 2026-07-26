use crate::node::{child_nodes, define_source_syntax_node};
use crate::{BlockExpressionSyntax, ExpressionSyntax, SyntaxKind};

use super::root::first_expression;

define_source_syntax_node! {
    /// Conditional `if` expression.
    pub struct ConditionalExpressionSyntax {
        builder: ConditionalExpressionSyntaxBuilder,
        kind: SyntaxKind::ConditionalExpression,
        source_slot: "conditional_expression.source",
        node_name: "conditional expression",
        range_description: "conditional-expression",
        debug_name: "ConditionalExpressionSyntax",
        builder_debug_name: "ConditionalExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `if` keyword token.
                if_keyword;
                /// Appends the `if` keyword token.
                push_if_keyword;
                kind: SyntaxKind::IfKeyword;
                slot: "conditional_expression.if_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the required consequent block expression.
                block_expression;
                /// Appends the consequent block expression.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
        repeated_children: [
            {
                /// Returns condition expressions in source order.
                expressions;
                /// Appends a condition expression.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            },
            {
                /// Returns conditional-else children in source order.
                conditional_elses;
                /// Appends a conditional-else child.
                push_conditional_else;
                ty: ConditionalElseSyntax;
                kind: SyntaxKind::ConditionalElse;
            }
        ],
    }
}

impl ConditionalExpressionSyntax {
    /// Returns the condition expression child.
    pub fn condition_expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }

    /// Returns the optional else clause child.
    pub fn conditional_else(&self) -> Option<ConditionalElseSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::ConditionalElse,
            ConditionalElseSyntax::from_green,
        )
        .next()
    }
}

define_source_syntax_node! {
    /// Conditional else clause.
    pub struct ConditionalElseSyntax {
        builder: ConditionalElseSyntaxBuilder,
        kind: SyntaxKind::ConditionalElse,
        source_slot: "conditional_else.source",
        node_name: "conditional else",
        range_description: "conditional-else",
        debug_name: "ConditionalElseSyntax",
        builder_debug_name: "ConditionalElseSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `else` keyword token.
                else_keyword;
                /// Appends the `else` keyword token.
                push_else_keyword;
                kind: SyntaxKind::ElseKeyword;
                slot: "conditional_else.else_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns block-expression children in source order.
                block_expressions;
                /// Appends a block-expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            },
            {
                /// Returns nested conditional-expression children in source order.
                conditional_expressions;
                /// Appends a nested conditional-expression child.
                push_conditional_expression;
                ty: ConditionalExpressionSyntax;
                kind: SyntaxKind::ConditionalExpression;
            }
        ],
    }
}

impl ConditionalElseSyntax {
    /// Returns the block-expression child when this is a block else.
    pub fn block_expression(&self) -> Option<BlockExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::BlockExpression,
            BlockExpressionSyntax::from_green,
        )
        .next()
    }

    /// Returns the conditional-expression child when this is an else-if.
    pub fn conditional_expression(&self) -> Option<ConditionalExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::ConditionalExpression,
            ConditionalExpressionSyntax::from_green,
        )
        .next()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{
        block_expression, keyword, snapshot as test_snapshot, token_expression_from_token,
    };
    use crate::{ConditionalElseSyntax, ConditionalExpressionSyntax, SyntaxKind, SyntaxText};

    #[test]
    fn conditional_expressions_store_condition_blocks_and_else_clause() {
        let snapshot = test_snapshot("syntax-conditional-expression-test", "if ready {} else {}");

        let mut conditional =
            ConditionalExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        let mut clause = ConditionalElseSyntax::builder(snapshot.clone(), TextSize::new(12));

        conditional.push_if_keyword(keyword(SyntaxKind::IfKeyword, 0, 2, true));

        conditional.push_expression(token_expression_from_token(
            snapshot.clone(),
            keyword(SyntaxKind::IdentifierToken, 3, 8, true),
        ));

        conditional.push_block_expression(block_expression(snapshot.clone(), 9, true));

        clause.push_else_keyword(keyword(SyntaxKind::ElseKeyword, 12, 16, true));
        clause.push_block_expression(block_expression(snapshot, 18, false));
        conditional.push_conditional_else(clause.build());

        let conditional = conditional.build();

        assert_eq!(conditional.full_text(), "if ready {} else {}");
        assert!(conditional.condition_expression().is_some());
        assert!(conditional.conditional_else().is_some());
    }
}
