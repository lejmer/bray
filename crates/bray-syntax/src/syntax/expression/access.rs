use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind};

define_source_syntax_node! {
    /// Access expression.
    pub struct AccessExpressionSyntax {
        builder: AccessExpressionSyntaxBuilder,
        kind: SyntaxKind::AccessExpression,
        source_slot: "access_expression.source",
        node_name: "access expression",
        range_description: "access-expression",
        debug_name: "AccessExpressionSyntax",
        builder_debug_name: "AccessExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the identifier root token.
                identifier_token;
                /// Appends the identifier root token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "access_expression.identifier_token";
            },
            {
                /// Returns the `self` root token.
                self_token;
                /// Appends the `self` root token.
                push_self_token;
                kind: SyntaxKind::SelfValueKeyword;
                slot: "access_expression.self_token";
            },
            {
                /// Returns the `internal` root token.
                internal_token;
                /// Appends the `internal` root token.
                push_internal_token;
                kind: SyntaxKind::InternalKeyword;
                slot: "access_expression.internal_token";
            },
            {
                /// Returns the grouped-access opening parenthesis token.
                open_paren_token;
                /// Appends the grouped-access opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "access_expression.open_paren_token";
            },
            {
                /// Returns the grouped-access closing parenthesis token.
                close_paren_token;
                /// Appends the grouped-access closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "access_expression.close_paren_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns nested access-expression children in source order.
                access_expressions;
                /// Appends a nested access-expression child.
                push_access_expression;
                ty: AccessExpressionSyntax;
                kind: SyntaxKind::AccessExpression;
            },
            {
                /// Returns member-access operations in source order.
                member_access_operations;
                /// Appends a member-access operation.
                push_member_access_operation;
                ty: MemberAccessOperationSyntax;
                kind: SyntaxKind::MemberAccessOperation;
            },
            {
                /// Returns element-index operations in source order.
                element_index_operations;
                /// Appends an element-index operation.
                push_element_index_operation;
                ty: ElementIndexOperationSyntax;
                kind: SyntaxKind::ElementIndexOperation;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Member access postfix operation.
    pub struct MemberAccessOperationSyntax {
        builder: MemberAccessOperationSyntaxBuilder,
        kind: SyntaxKind::MemberAccessOperation,
        source_slot: "member_access_operation.source",
        node_name: "member access operation",
        range_description: "member-access-operation",
        debug_name: "MemberAccessOperationSyntax",
        builder_debug_name: "MemberAccessOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required dot token.
                dot_token;
                /// Appends the dot token.
                push_dot_token;
                kind: SyntaxKind::DotToken;
                slot: "member_access_operation.dot_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the member identifier token.
                identifier_token;
                /// Appends the member identifier token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "member_access_operation.identifier_token";
            },
            {
                /// Returns the tuple-element index token.
                tuple_element_index_token;
                /// Appends the tuple-element index token.
                push_tuple_element_index_token;
                kind: SyntaxKind::TupleElementIndexToken;
                slot: "member_access_operation.tuple_element_index_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Element index postfix operation.
    pub struct ElementIndexOperationSyntax {
        builder: ElementIndexOperationSyntaxBuilder,
        kind: SyntaxKind::ElementIndexOperation,
        source_slot: "element_index_operation.source",
        node_name: "element index operation",
        range_description: "element-index-operation",
        debug_name: "ElementIndexOperationSyntax",
        builder_debug_name: "ElementIndexOperationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening bracket token.
                open_bracket_token;
                /// Appends the opening bracket token.
                push_open_bracket_token;
                kind: SyntaxKind::OpenBracketToken;
                slot: "element_index_operation.open_bracket_token";
            },
            {
                /// Returns the required closing bracket token.
                close_bracket_token;
                /// Appends the closing bracket token.
                push_close_bracket_token;
                kind: SyntaxKind::CloseBracketToken;
                slot: "element_index_operation.close_bracket_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the index expression child.
                expression;
                /// Appends the index expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}
