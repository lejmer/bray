use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind};

macro_rules! define_required_operand_keyword_expression {
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
                required_children: [
                    {
                        /// Returns the operand expression child.
                        expression;
                        /// Appends the operand expression child.
                        push_expression;
                        ty: ExpressionSyntax;
                        kind: SyntaxKind::Expression;
                    }
                ],
            }
        }
    };
}

define_source_syntax_node! {
    /// Borrow expression.
    pub struct BorrowExpressionSyntax {
        builder: BorrowExpressionSyntaxBuilder,
        kind: SyntaxKind::BorrowExpression,
        source_slot: "borrow_expression.source",
        node_name: "borrow expression",
        range_description: "borrow-expression",
        debug_name: "BorrowExpressionSyntax",
        builder_debug_name: "BorrowExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required ampersand token.
                ampersand_token;
                /// Appends the ampersand token.
                push_ampersand_token;
                kind: SyntaxKind::AmpersandToken;
                slot: "borrow_expression.ampersand_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional `mut` keyword token.
                mut_keyword;
                /// Appends the optional `mut` keyword token.
                push_mut_keyword;
                kind: SyntaxKind::MutKeyword;
                slot: "borrow_expression.mut_keyword";
            }
        ],
        required_children: [
            {
                /// Returns the borrowed expression child.
                expression;
                /// Appends the borrowed expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_required_operand_keyword_expression! {
    /// Trusted boundary expression.
    TrustBoundaryExpressionSyntax {
        builder: TrustBoundaryExpressionSyntaxBuilder,
        kind: SyntaxKind::TrustBoundaryExpression,
        source_slot: "trust_boundary_expression.source",
        node_name: "trust boundary expression",
        range_description: "trust-boundary-expression",
        debug_name: "TrustBoundaryExpressionSyntax",
        builder_debug_name: "TrustBoundaryExpressionSyntaxBuilder",
        keyword_getter: trusted_keyword,
        keyword_pusher: push_trusted_keyword,
        keyword_kind: SyntaxKind::TrustedKeyword,
        keyword_slot: "trust_boundary_expression.trusted_keyword",
    }
}

define_required_operand_keyword_expression! {
    /// Result propagation expression.
    ResultPropagationExpressionSyntax {
        builder: ResultPropagationExpressionSyntaxBuilder,
        kind: SyntaxKind::ResultPropagationExpression,
        source_slot: "result_propagation_expression.source",
        node_name: "result propagation expression",
        range_description: "result-propagation-expression",
        debug_name: "ResultPropagationExpressionSyntax",
        builder_debug_name: "ResultPropagationExpressionSyntaxBuilder",
        keyword_getter: try_keyword,
        keyword_pusher: push_try_keyword,
        keyword_kind: SyntaxKind::TryKeyword,
        keyword_slot: "result_propagation_expression.try_keyword",
    }
}

define_required_operand_keyword_expression! {
    /// Catch expression.
    CatchExpressionSyntax {
        builder: CatchExpressionSyntaxBuilder,
        kind: SyntaxKind::CatchExpression,
        source_slot: "catch_expression.source",
        node_name: "catch expression",
        range_description: "catch-expression",
        debug_name: "CatchExpressionSyntax",
        builder_debug_name: "CatchExpressionSyntaxBuilder",
        keyword_getter: catch_keyword,
        keyword_pusher: push_catch_keyword,
        keyword_kind: SyntaxKind::CatchKeyword,
        keyword_slot: "catch_expression.catch_keyword",
    }
}

define_required_operand_keyword_expression! {
    /// Await expression.
    AwaitExpressionSyntax {
        builder: AwaitExpressionSyntaxBuilder,
        kind: SyntaxKind::AwaitExpression,
        source_slot: "await_expression.source",
        node_name: "await expression",
        range_description: "await-expression",
        debug_name: "AwaitExpressionSyntax",
        builder_debug_name: "AwaitExpressionSyntaxBuilder",
        keyword_getter: await_keyword,
        keyword_pusher: push_await_keyword,
        keyword_kind: SyntaxKind::AwaitKeyword,
        keyword_slot: "await_expression.await_keyword",
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{keyword, snapshot as test_snapshot, token_expression_from_token};
    use crate::{BorrowExpressionSyntax, SyntaxKind, SyntaxText};

    #[test]
    fn borrow_expressions_store_mutability_and_operand() {
        let snapshot = test_snapshot("syntax-borrow-expression-test", "& mut value");

        let mut builder = BorrowExpressionSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_ampersand_token(keyword(SyntaxKind::AmpersandToken, 0, 1, true));
        builder.push_mut_keyword(keyword(SyntaxKind::MutKeyword, 2, 5, true));

        builder.push_expression(token_expression_from_token(
            snapshot,
            keyword(SyntaxKind::IdentifierToken, 6, 11, false),
        ));

        let expression = builder.build();

        assert_eq!(expression.full_text(), "& mut value");

        assert_eq!(
            expression
                .expression()
                .primary_expression()
                .and_then(|primary| primary.primary_token())
                .map(|token| token.kind()),
            Some(SyntaxKind::IdentifierToken)
        );
    }
}
