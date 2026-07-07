use crate::SyntaxKind;
use crate::node::define_source_syntax_node;

define_source_syntax_node! {
    /// Parenthesized directive arguments.
    pub struct DirectiveArgumentListSyntax {
        builder: DirectiveArgumentListSyntaxBuilder,
        kind: SyntaxKind::DirectiveArgumentList,
        source_slot: "directive_argument_list.source",
        node_name: "directive argument list",
        range_description: "directive-argument-list",
        debug_name: "DirectiveArgumentListSyntax",
        builder_debug_name: "DirectiveArgumentListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "directive_argument_list.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "directive_argument_list.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// `@target(...)` directive.
    pub struct TargetDirectiveSyntax {
        builder: TargetDirectiveSyntaxBuilder,
        kind: SyntaxKind::TargetDirective,
        source_slot: "target_directive.source",
        node_name: "target directive",
        range_description: "target-directive",
        debug_name: "TargetDirectiveSyntax",
        builder_debug_name: "TargetDirectiveSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required directive marker token.
                directive_marker_token;
                /// Appends the directive marker token.
                push_directive_marker_token;
                kind: SyntaxKind::AtToken;
                slot: "target_directive.directive_marker_token";
            },
            {
                /// Returns the required directive name token.
                name_token;
                /// Appends the directive name token.
                push_name_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "target_directive.name_token";
            },
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "target_directive.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "target_directive.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// `@test` directive.
    pub struct TestDirectiveSyntax {
        builder: TestDirectiveSyntaxBuilder,
        kind: SyntaxKind::TestDirective,
        source_slot: "test_directive.source",
        node_name: "test directive",
        range_description: "test-directive",
        debug_name: "TestDirectiveSyntax",
        builder_debug_name: "TestDirectiveSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required directive marker token.
                directive_marker_token;
                /// Appends the directive marker token.
                push_directive_marker_token;
                kind: SyntaxKind::AtToken;
                slot: "test_directive.directive_marker_token";
            },
            {
                /// Returns the required directive name token.
                name_token;
                /// Appends the directive name token.
                push_name_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "test_directive.name_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// `@link(...)` directive.
    pub struct LinkDirectiveSyntax {
        builder: LinkDirectiveSyntaxBuilder,
        kind: SyntaxKind::LinkDirective,
        source_slot: "link_directive.source",
        node_name: "link directive",
        range_description: "link-directive",
        debug_name: "LinkDirectiveSyntax",
        builder_debug_name: "LinkDirectiveSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [
            {
                /// Returns the required directive marker token.
                directive_marker_token;
                /// Appends the directive marker token.
                push_directive_marker_token;
                kind: SyntaxKind::AtToken;
                slot: "link_directive.directive_marker_token";
            },
            {
                /// Returns the required directive name token.
                name_token;
                /// Appends the directive name token.
                push_name_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "link_directive.name_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the directive argument list child.
                directive_argument_list;
                /// Appends the directive argument list child.
                push_directive_argument_list;
                ty: DirectiveArgumentListSyntax;
                kind: SyntaxKind::DirectiveArgumentList;
            }
        ],
    }
}
