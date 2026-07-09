use crate::node::define_source_syntax_node;
use crate::{
    BlockExpressionSyntax, ExpressionSyntax, IrrefutablePatternSyntax, SyntaxKind, SyntaxToken,
};

use super::root::first_expression;

define_source_syntax_node! {
    /// While expression.
    pub struct WhileExpressionSyntax {
        builder: WhileExpressionSyntaxBuilder,
        kind: SyntaxKind::WhileExpression,
        source_slot: "while_expression.source",
        node_name: "while expression",
        range_description: "while-expression",
        debug_name: "WhileExpressionSyntax",
        builder_debug_name: "WhileExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `while` keyword token.
                while_keyword;
                /// Appends the `while` keyword token.
                push_while_keyword;
                kind: SyntaxKind::WhileKeyword;
                slot: "while_expression.while_keyword";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional `else` keyword token.
                else_keyword;
                /// Appends the `else` keyword token.
                push_else_keyword;
                kind: SyntaxKind::ElseKeyword;
                slot: "while_expression.else_keyword";
            }
        ],
        required_children: [],
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
                /// Returns block-expression children in source order.
                block_expressions;
                /// Appends a block-expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
    }
}

impl WhileExpressionSyntax {
    /// Returns the condition expression child.
    pub fn condition_expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

define_source_syntax_node! {
    /// For expression.
    pub struct ForExpressionSyntax {
        builder: ForExpressionSyntaxBuilder,
        kind: SyntaxKind::ForExpression,
        source_slot: "for_expression.source",
        node_name: "for expression",
        range_description: "for-expression",
        debug_name: "ForExpressionSyntax",
        builder_debug_name: "ForExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `for` keyword token.
                for_keyword;
                /// Appends the `for` keyword token.
                push_for_keyword;
                kind: SyntaxKind::ForKeyword;
                slot: "for_expression.for_keyword";
            },
            {
                /// Returns the required `in` keyword token.
                in_keyword;
                /// Appends the `in` keyword token.
                push_in_keyword;
                kind: SyntaxKind::InKeyword;
                slot: "for_expression.in_keyword";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional `else` keyword token.
                else_keyword;
                /// Appends the `else` keyword token.
                push_else_keyword;
                kind: SyntaxKind::ElseKeyword;
                slot: "for_expression.else_keyword";
            }
        ],
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
            }
        ],
        repeated_children: [
            {
                /// Returns block-expression children in source order.
                block_expressions;
                /// Appends a block-expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Iteration source.
    pub struct IterationSourceSyntax {
        builder: IterationSourceSyntaxBuilder,
        kind: SyntaxKind::IterationSource,
        source_slot: "iteration_source.source",
        node_name: "iteration source",
        range_description: "iteration-source",
        debug_name: "IterationSourceSyntax",
        builder_debug_name: "IterationSourceSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional `mut` iteration source mode token.
                mut_keyword;
                /// Appends the `mut` iteration source mode token.
                push_mut_keyword;
                kind: SyntaxKind::MutKeyword;
                slot: "iteration_source.mut_keyword";
            },
            {
                /// Returns the optional `move` iteration source mode token.
                move_keyword;
                /// Appends the `move` iteration source mode token.
                push_move_keyword;
                kind: SyntaxKind::MoveKeyword;
                slot: "iteration_source.move_keyword";
            }
        ],
        required_children: [
            {
                /// Returns the source expression child.
                expression;
                /// Appends the source expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl IterationSourceSyntax {
    /// Returns the iteration mode token when present.
    pub fn mode_token(&self) -> Option<SyntaxToken> {
        self.mut_keyword().or_else(|| self.move_keyword())
    }
}

define_source_syntax_node! {
    /// Infinite loop expression.
    pub struct LoopExpressionSyntax {
        builder: LoopExpressionSyntaxBuilder,
        kind: SyntaxKind::LoopExpression,
        source_slot: "loop_expression.source",
        node_name: "loop expression",
        range_description: "loop-expression",
        debug_name: "LoopExpressionSyntax",
        builder_debug_name: "LoopExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `loop` keyword token.
                loop_keyword;
                /// Appends the `loop` keyword token.
                push_loop_keyword;
                kind: SyntaxKind::LoopKeyword;
                slot: "loop_expression.loop_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the loop body block expression.
                block_expression;
                /// Appends the loop body block expression.
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

    use crate::test_support::{keyword, snapshot as test_snapshot, token_expression};
    use crate::{IterationSourceSyntax, SyntaxKind, SyntaxText};

    #[test]
    fn iteration_sources_store_optional_mode_and_expression() {
        let snapshot = test_snapshot("syntax-iteration-source-test", "mut values");
        let mut source = IterationSourceSyntax::builder(snapshot.clone(), TextSize::ZERO);

        source.push_mut_keyword(keyword(SyntaxKind::MutKeyword, 0, 3, true));

        source.push_expression(token_expression(
            snapshot,
            SyntaxKind::IdentifierToken,
            4,
            10,
        ));

        let source = source.build();

        assert_eq!(source.full_text(), "mut values");

        assert_eq!(
            source.mode_token().map(|token| token.kind()),
            Some(SyntaxKind::MutKeyword)
        );
    }
}
