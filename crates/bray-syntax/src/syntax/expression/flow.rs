use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    ArgumentListSyntax, BlockExpressionSyntax, ExpressionSyntax, IrrefutablePatternSyntax,
    SyntaxKind, TypeAnnotationSyntax,
};

use super::root::first_expression;

define_source_syntax_node! {
    /// Scoped `with` expression.
    pub struct WithExpressionSyntax {
        builder: WithExpressionSyntaxBuilder,
        kind: SyntaxKind::WithExpression,
        source_slot: "with_expression.source",
        node_name: "with expression",
        range_description: "with-expression",
        debug_name: "WithExpressionSyntax",
        builder_debug_name: "WithExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `with` keyword token.
                with_keyword;
                /// Appends the `with` keyword token.
                push_with_keyword;
                kind: SyntaxKind::WithKeyword;
                slot: "with_expression.with_keyword";
            },
            {
                /// Returns the required initializer equals token.
                equals_token;
                /// Appends the initializer equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "with_expression.equals_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the binding pattern child.
                irrefutable_pattern;
                /// Appends the binding pattern child.
                push_irrefutable_pattern;
                ty: IrrefutablePatternSyntax;
                kind: SyntaxKind::IrrefutablePattern;
            },
            {
                /// Returns the scoped block expression child.
                block_expression;
                /// Appends the scoped block expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
        repeated_children: [
            {
                /// Returns type annotation children in source order.
                type_annotations;
                /// Appends a type annotation child.
                push_type_annotation;
                ty: TypeAnnotationSyntax;
                kind: SyntaxKind::TypeAnnotation;
            },
            {
                /// Returns initializer expression children in source order.
                expressions;
                /// Appends an initializer expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl WithExpressionSyntax {
    /// Returns the optional type annotation child.
    pub fn type_annotation(&self) -> Option<TypeAnnotationSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::TypeAnnotation,
            TypeAnnotationSyntax::from_green,
        )
        .next()
    }

    /// Returns the initializer expression child.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

macro_rules! define_optional_operand_flow_expression {
    (
        $(#[$node_meta:meta])*
        $node:ident {
            builder: $builder:ident,
            kind: $kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            keyword_getter: $keyword_getter:ident,
            keyword_pusher: $keyword_pusher:ident,
            keyword_kind: $keyword_kind:path,
            keyword_slot: $keyword_slot:literal $(,)?
        }
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node {
                builder: $builder,
                kind: $kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: true,
                required_tokens: [
                    {
                        /// Returns the required keyword token.
                        $keyword_getter;
                        /// Appends the keyword token.
                        $keyword_pusher;
                        kind: $keyword_kind;
                        slot: $keyword_slot;
                    }
                ],
                optional_tokens: [],
                required_children: [],
                repeated_children: [
                    {
                        /// Returns operand expression children in source order.
                        expressions;
                        /// Appends an operand expression child.
                        push_expression;
                        ty: ExpressionSyntax;
                        kind: SyntaxKind::Expression;
                    }
                ],
            }
        }

        impl $node {
            /// Returns the optional operand expression child.
            pub fn expression(&self) -> Option<ExpressionSyntax> {
                first_expression(&self.source, &self.node, self.start)
            }
        }
    };
}

define_optional_operand_flow_expression! {
    /// Yield expression.
    YieldExpressionSyntax {
        builder: YieldExpressionSyntaxBuilder,
        kind: SyntaxKind::YieldExpression,
        source_slot: "yield_expression.source",
        node_name: "yield expression",
        range_description: "yield-expression",
        debug_name: "YieldExpressionSyntax",
        builder_debug_name: "YieldExpressionSyntaxBuilder",
        keyword_getter: yield_keyword,
        keyword_pusher: push_yield_keyword,
        keyword_kind: SyntaxKind::YieldKeyword,
        keyword_slot: "yield_expression.yield_keyword",
    }
}

define_optional_operand_flow_expression! {
    /// Return expression.
    ReturnExpressionSyntax {
        builder: ReturnExpressionSyntaxBuilder,
        kind: SyntaxKind::ReturnExpression,
        source_slot: "return_expression.source",
        node_name: "return expression",
        range_description: "return-expression",
        debug_name: "ReturnExpressionSyntax",
        builder_debug_name: "ReturnExpressionSyntaxBuilder",
        keyword_getter: return_keyword,
        keyword_pusher: push_return_keyword,
        keyword_kind: SyntaxKind::ReturnKeyword,
        keyword_slot: "return_expression.return_keyword",
    }
}

define_optional_operand_flow_expression! {
    /// Break expression.
    BreakExpressionSyntax {
        builder: BreakExpressionSyntaxBuilder,
        kind: SyntaxKind::BreakExpression,
        source_slot: "break_expression.source",
        node_name: "break expression",
        range_description: "break-expression",
        debug_name: "BreakExpressionSyntax",
        builder_debug_name: "BreakExpressionSyntaxBuilder",
        keyword_getter: break_keyword,
        keyword_pusher: push_break_keyword,
        keyword_kind: SyntaxKind::BreakKeyword,
        keyword_slot: "break_expression.break_keyword",
    }
}

define_source_syntax_node! {
    /// Panic expression.
    pub struct PanicExpressionSyntax {
        builder: PanicExpressionSyntaxBuilder,
        kind: SyntaxKind::PanicExpression,
        source_slot: "panic_expression.source",
        node_name: "panic expression",
        range_description: "panic-expression",
        debug_name: "PanicExpressionSyntax",
        builder_debug_name: "PanicExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `panic` keyword token.
                panic_keyword;
                /// Appends the `panic` keyword token.
                push_panic_keyword;
                kind: SyntaxKind::PanicKeyword;
                slot: "panic_expression.panic_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the panic argument list child.
                argument_list;
                /// Appends the panic argument list child.
                push_argument_list;
                ty: ArgumentListSyntax;
                kind: SyntaxKind::ArgumentList;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Continue expression.
    pub struct ContinueExpressionSyntax {
        builder: ContinueExpressionSyntaxBuilder,
        kind: SyntaxKind::ContinueExpression,
        source_slot: "continue_expression.source",
        node_name: "continue expression",
        range_description: "continue-expression",
        debug_name: "ContinueExpressionSyntax",
        builder_debug_name: "ContinueExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `continue` keyword token.
                continue_keyword;
                /// Appends the `continue` keyword token.
                push_continue_keyword;
                kind: SyntaxKind::ContinueKeyword;
                slot: "continue_expression.continue_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{
        block_expression, identifier_path_with_trailing_space, keyword, snapshot as test_snapshot,
        token_expression_from_token,
    };
    use crate::{IrrefutablePatternSyntax, SyntaxKind, SyntaxText, WithExpressionSyntax};

    #[test]
    fn with_expressions_store_pattern_initializer_and_block() {
        let snapshot = test_snapshot("syntax-with-expression-test", "with item = value {}");

        let mut pattern = IrrefutablePatternSyntax::builder(snapshot.clone(), TextSize::new(5));
        let mut builder = WithExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        pattern.push_path(identifier_path_with_trailing_space(snapshot.clone(), 5, 9));

        builder.push_with_keyword(keyword(SyntaxKind::WithKeyword, 0, 4, true));
        builder.push_irrefutable_pattern(pattern.build());

        builder.push_equals_token(keyword(SyntaxKind::EqualsToken, 10, 11, true));

        builder.push_expression(token_expression_from_token(
            snapshot.clone(),
            keyword(SyntaxKind::IdentifierToken, 12, 17, true),
        ));

        builder.push_block_expression(block_expression(snapshot, 18, false));

        let expression = builder.build();

        assert_eq!(expression.full_text(), "with item = value {}");

        assert_eq!(
            expression
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value "))
        );
    }
}
