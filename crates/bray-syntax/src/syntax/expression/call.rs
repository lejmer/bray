use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Call postfix operation.
    pub struct CallOperationSyntax {
        builder: CallOperationSyntaxBuilder,
        kind: SyntaxKind::CallOperation,
        source_slot: "call_operation.source",
        node_name: "call operation",
        range_description: "call-operation",
        debug_name: "CallOperationSyntax",
        builder_debug_name: "CallOperationSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the argument-list child.
                argument_list;
                /// Appends the argument-list child.
                push_argument_list;
                ty: ArgumentListSyntax;
                kind: SyntaxKind::ArgumentList;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Runtime argument entry.
    pub struct ArgumentSyntax {
        builder: ArgumentSyntaxBuilder,
        kind: SyntaxKind::Argument,
        source_slot: "argument.source",
        node_name: "argument",
        range_description: "argument",
        debug_name: "ArgumentSyntax",
        builder_debug_name: "ArgumentSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the named argument identifier token.
                identifier_token;
                /// Appends the named argument identifier token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "argument.identifier_token";
            },
            {
                /// Returns the named argument equals token.
                equals_token;
                /// Appends the named argument equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "argument.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the argument expression child.
                expression;
                /// Appends the argument expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Runtime argument list including delimiters.
    pub struct ArgumentListSyntax {
        builder: ArgumentListSyntaxBuilder,
        kind: SyntaxKind::ArgumentList,
        source_slot: "argument_list.source",
        node_name: "argument list",
        range_description: "argument-list",
        debug_name: "ArgumentListSyntax",
        builder_debug_name: "ArgumentListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "argument_list.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "argument_list.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "argument_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns argument entries in source order.
                arguments;
                /// Appends an argument entry.
                push_argument;
                ty: ArgumentSyntax;
                kind: SyntaxKind::Argument;
            }
        ],
    }
}

impl ArgumentListSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}
