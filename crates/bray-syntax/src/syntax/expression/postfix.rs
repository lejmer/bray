use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind, TypeExpressionSyntax};

define_source_syntax_node! {
    /// Slice index postfix operation.
    pub struct SliceIndexOperationSyntax {
        builder: SliceIndexOperationSyntaxBuilder,
        kind: SyntaxKind::SliceIndexOperation,
        source_slot: "slice_index_operation.source",
        node_name: "slice index operation",
        range_description: "slice-index-operation",
        debug_name: "SliceIndexOperationSyntax",
        builder_debug_name: "SliceIndexOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "slice_index_operation.open_bracket_token";
            },
            {
                /// Returns the required range token.
                dot_dot_token;
                /// Appends the range token.
                push_dot_dot_token;
                kind: SyntaxKind::DotDotToken;
                slot: "slice_index_operation.dot_dot_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "slice_index_operation.close_bracket_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns slice bound expressions in source order.
                expressions;
                /// Appends a slice bound expression.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Nullable propagation postfix operation.
    pub struct NullablePropagationOperationSyntax {
        builder: NullablePropagationOperationSyntaxBuilder,
        kind: SyntaxKind::NullablePropagationOperation,
        source_slot: "nullable_propagation_operation.source",
        node_name: "nullable propagation operation",
        range_description: "nullable-propagation-operation",
        debug_name: "NullablePropagationOperationSyntax",
        builder_debug_name: "NullablePropagationOperationSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required question token.
                question_token;
                /// Appends the question token.
                push_question_token;
                kind: SyntaxKind::QuestionToken;
                slot: "nullable_propagation_operation.question_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Conversion postfix operation.
    pub struct ConversionOperationSyntax {
        builder: ConversionOperationSyntaxBuilder,
        kind: SyntaxKind::ConversionOperation,
        source_slot: "conversion_operation.source",
        node_name: "conversion operation",
        range_description: "conversion-operation",
        debug_name: "ConversionOperationSyntax",
        builder_debug_name: "ConversionOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `as` keyword token.
                as_keyword;
                /// Appends the `as` keyword token.
                push_as_keyword;
                kind: SyntaxKind::AsKeyword;
                slot: "conversion_operation.as_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the target type-expression child.
                type_expression;
                /// Appends the target type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            }
        ],
    }
}
