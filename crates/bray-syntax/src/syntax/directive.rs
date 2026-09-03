use crate::node::define_source_syntax_node;
use crate::{ExpressionSyntax, SyntaxKind};

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
    /// Single directive argument.
    pub struct DirectiveArgumentSyntax {
        builder: DirectiveArgumentSyntaxBuilder,
        kind: SyntaxKind::DirectiveArgument,
        source_slot: "directive_argument.source",
        node_name: "directive argument",
        range_description: "directive-argument",
        debug_name: "DirectiveArgumentSyntax",
        builder_debug_name: "DirectiveArgumentSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional argument name token.
                name_token;
                /// Appends an argument name token.
                push_name_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "directive_argument.name_token";
            },
            {
                /// Returns the optional named-argument equals token.
                equals_token;
                /// Appends a named-argument equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "directive_argument.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the argument value expression child.
                expression;
                /// Appends the argument value expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
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
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "directive_argument_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns directive arguments in source order.
                directive_arguments;
                /// Appends a directive argument.
                push_directive_argument;
                ty: DirectiveArgumentSyntax;
                kind: SyntaxKind::DirectiveArgument;
            }
        ],
    }
}

impl crate::node::GreenSeparatedSyntaxNode for DirectiveArgumentListSyntax {}

define_argument_list_directive_syntax! {
    /// `@target(...)` directive.
    TargetDirectiveSyntax {
        builder: TargetDirectiveSyntaxBuilder,
        kind: SyntaxKind::TargetDirective,
        source_slot: "target_directive.source",
        node_name: "target directive",
        range_description: "target-directive",
        debug_name: "TargetDirectiveSyntax",
        builder_debug_name: "TargetDirectiveSyntaxBuilder",
        marker_slot: "target_directive.directive_marker_token",
        name_slot: "target_directive.name_token",
    }
}

define_bare_directive_syntax! {
    /// `@copy` directive.
    CopyDirectiveSyntax {
        builder: CopyDirectiveSyntaxBuilder,
        kind: SyntaxKind::CopyDirective,
        source_slot: "copy_directive.source",
        node_name: "copy directive",
        range_description: "copy-directive",
        debug_name: "CopyDirectiveSyntax",
        builder_debug_name: "CopyDirectiveSyntaxBuilder",
        marker_slot: "copy_directive.directive_marker_token",
        name_slot: "copy_directive.name_token",
    }
}

define_bare_directive_syntax! {
    /// `@thread_local` directive.
    ThreadLocalDirectiveSyntax {
        builder: ThreadLocalDirectiveSyntaxBuilder,
        kind: SyntaxKind::ThreadLocalDirective,
        source_slot: "thread_local_directive.source",
        node_name: "thread-local directive",
        range_description: "thread-local-directive",
        debug_name: "ThreadLocalDirectiveSyntax",
        builder_debug_name: "ThreadLocalDirectiveSyntaxBuilder",
        marker_slot: "thread_local_directive.directive_marker_token",
        name_slot: "thread_local_directive.name_token",
    }
}

define_source_syntax_node! {
    /// `@test` or `@test(...)` directive.
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
        repeated_children: [
            {
                /// Returns the optional directive argument list.
                directive_argument_lists;
                /// Appends the directive argument list.
                push_directive_argument_list;
                ty: DirectiveArgumentListSyntax;
                kind: SyntaxKind::DirectiveArgumentList;
            }
        ],
    }
}

impl TestDirectiveSyntax {
    /// Returns the optional directive argument list.
    pub fn directive_argument_list(&self) -> Option<DirectiveArgumentListSyntax> {
        self.directive_argument_lists().next()
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
    /// `@tag(...)` directive.
    TagDirectiveSyntax {
        builder: TagDirectiveSyntaxBuilder,
        kind: SyntaxKind::TagDirective,
        source_slot: "tag_directive.source",
        node_name: "tag directive",
        range_description: "tag-directive",
        debug_name: "TagDirectiveSyntax",
        builder_debug_name: "TagDirectiveSyntaxBuilder",
        marker_slot: "tag_directive.directive_marker_token",
        name_slot: "tag_directive.name_token",
    }
}

define_argument_list_directive_syntax! {
    /// `@layout(...)` directive.
    LayoutDirectiveSyntax {
        builder: LayoutDirectiveSyntaxBuilder,
        kind: SyntaxKind::LayoutDirective,
        source_slot: "layout_directive.source",
        node_name: "layout directive",
        range_description: "layout-directive",
        debug_name: "LayoutDirectiveSyntax",
        builder_debug_name: "LayoutDirectiveSyntaxBuilder",
        marker_slot: "layout_directive.directive_marker_token",
        name_slot: "layout_directive.name_token",
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
