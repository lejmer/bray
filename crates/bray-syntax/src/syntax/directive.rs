use crate::SyntaxKind;
use crate::node::define_source_syntax_node;

macro_rules! define_bare_directive_syntax {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            marker_slot: $marker_slot:literal,
            name_slot: $name_slot:literal $(,)?
        }
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: false,
                required_tokens: [
                    {
                        /// Returns the required directive marker token.
                        directive_marker_token;
                        /// Appends the directive marker token.
                        push_directive_marker_token;
                        kind: SyntaxKind::AtToken;
                        slot: $marker_slot;
                    },
                    {
                        /// Returns the required directive name token.
                        name_token;
                        /// Appends the directive name token.
                        push_name_token;
                        kind: SyntaxKind::IdentifierToken;
                        slot: $name_slot;
                    }
                ],
                optional_tokens: [],
                required_children: [],
            }
        }
    };
}

macro_rules! define_argument_list_directive_syntax {
    (
        $(#[$node_meta:meta])*
        $node_syntax:ident {
            builder: $builder_syntax:ident,
            kind: $node_kind:path,
            source_slot: $source_slot:literal,
            node_name: $node_name:literal,
            range_description: $range_description:literal,
            debug_name: $debug_name:literal,
            builder_debug_name: $builder_debug_name:literal,
            marker_slot: $marker_slot:literal,
            name_slot: $name_slot:literal $(,)?
        }
    ) => {
        define_source_syntax_node! {
            $(#[$node_meta])*
            pub struct $node_syntax {
                builder: $builder_syntax,
                kind: $node_kind,
                source_slot: $source_slot,
                node_name: $node_name,
                range_description: $range_description,
                debug_name: $debug_name,
                builder_debug_name: $builder_debug_name,
                skipped_syntax: false,
                required_tokens: [
                    {
                        /// Returns the required directive marker token.
                        directive_marker_token;
                        /// Appends the directive marker token.
                        push_directive_marker_token;
                        kind: SyntaxKind::AtToken;
                        slot: $marker_slot;
                    },
                    {
                        /// Returns the required directive name token.
                        name_token;
                        /// Appends the directive name token.
                        push_name_token;
                        kind: SyntaxKind::IdentifierToken;
                        slot: $name_slot;
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
    };
}

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
        // TODO(syntax): Replace skipped syntax with typed directive argument items.
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
        // TODO(syntax): Replace skipped syntax with typed target directive arguments.
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

define_bare_directive_syntax! {
    /// `@test` directive.
    TestDirectiveSyntax {
        builder: TestDirectiveSyntaxBuilder,
        kind: SyntaxKind::TestDirective,
        source_slot: "test_directive.source",
        node_name: "test directive",
        range_description: "test-directive",
        debug_name: "TestDirectiveSyntax",
        builder_debug_name: "TestDirectiveSyntaxBuilder",
        marker_slot: "test_directive.directive_marker_token",
        name_slot: "test_directive.name_token",
    }
}

define_bare_directive_syntax! {
    /// `@entrypoint` directive.
    EntrypointDirectiveSyntax {
        builder: EntrypointDirectiveSyntaxBuilder,
        kind: SyntaxKind::EntrypointDirective,
        source_slot: "entrypoint_directive.source",
        node_name: "entrypoint directive",
        range_description: "entrypoint-directive",
        debug_name: "EntrypointDirectiveSyntax",
        builder_debug_name: "EntrypointDirectiveSyntaxBuilder",
        marker_slot: "entrypoint_directive.directive_marker_token",
        name_slot: "entrypoint_directive.name_token",
    }
}

define_argument_list_directive_syntax! {
    /// `@link(...)` directive.
    LinkDirectiveSyntax {
        builder: LinkDirectiveSyntaxBuilder,
        kind: SyntaxKind::LinkDirective,
        source_slot: "link_directive.source",
        node_name: "link directive",
        range_description: "link-directive",
        debug_name: "LinkDirectiveSyntax",
        builder_debug_name: "LinkDirectiveSyntaxBuilder",
        marker_slot: "link_directive.directive_marker_token",
        name_slot: "link_directive.name_token",
    }
}

define_argument_list_directive_syntax! {
    /// `@abi(...)` directive.
    AbiDirectiveSyntax {
        builder: AbiDirectiveSyntaxBuilder,
        kind: SyntaxKind::AbiDirective,
        source_slot: "abi_directive.source",
        node_name: "abi directive",
        range_description: "abi-directive",
        debug_name: "AbiDirectiveSyntax",
        builder_debug_name: "AbiDirectiveSyntaxBuilder",
        marker_slot: "abi_directive.directive_marker_token",
        name_slot: "abi_directive.name_token",
    }
}

define_argument_list_directive_syntax! {
    /// `@symbol(...)` directive.
    SymbolDirectiveSyntax {
        builder: SymbolDirectiveSyntaxBuilder,
        kind: SyntaxKind::SymbolDirective,
        source_slot: "symbol_directive.source",
        node_name: "symbol directive",
        range_description: "symbol-directive",
        debug_name: "SymbolDirectiveSyntax",
        builder_debug_name: "SymbolDirectiveSyntaxBuilder",
        marker_slot: "symbol_directive.directive_marker_token",
        name_slot: "symbol_directive.name_token",
    }
}
