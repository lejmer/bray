use crate::node::define_source_syntax_node;
use crate::{EnsuresClauseSyntax, ExpressionSyntax, SyntaxKind};

define_source_syntax_node! {
    /// One execution property identifier.
    pub struct ExecutionPropertySyntax {
        builder: ExecutionPropertySyntaxBuilder,
        kind: SyntaxKind::ExecutionProperty,
        source_slot: "execution_property.source",
        node_name: "execution property",
        range_description: "execution-property",
        debug_name: "ExecutionPropertySyntax",
        builder_debug_name: "ExecutionPropertySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the identifier token.
                identifier_token;
                /// Appends the identifier token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [],
    }
}

define_source_syntax_node! {
    /// Execution property names, without executable expressions.
    pub struct ExecutesClauseSyntax {
        builder: ExecutesClauseSyntaxBuilder,
        kind: SyntaxKind::ExecutesClause,
        source_slot: "executes_clause.source",
        node_name: "executes clause",
        range_description: "executes-clause",
        debug_name: "ExecutesClauseSyntax",
        builder_debug_name: "ExecutesClauseSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the executes keyword.
                executes_keyword;
                /// Appends the executes keyword.
                push_executes_keyword;
                kind: SyntaxKind::ExecutesKeyword;
                slot: "executes_keyword";
            },
            {
                /// Returns the open paren token.
                open_paren_token;
                /// Appends the open paren token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "open_paren_token";
            },
            {
                /// Returns the close paren token.
                close_paren_token;
                /// Appends the close paren token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the separator token.
                separator_token;
                /// Appends the separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "separator_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns properties.
                properties;
                /// Appends an execution property.
                push_property;
                ty: ExecutionPropertySyntax;
                kind: SyntaxKind::ExecutionProperty;
            }
        ],
    }
}
impl crate::node::GreenSeparatedSyntaxNode for ExecutesClauseSyntax {}

define_source_syntax_node! {
    /// Guarantees conditional on one execution-entry predicate.
    pub struct WhenClauseSyntax {
        builder: WhenClauseSyntaxBuilder,
        kind: SyntaxKind::WhenClause,
        source_slot: "when_clause.source",
        node_name: "when clause",
        range_description: "when-clause",
        debug_name: "WhenClauseSyntax",
        builder_debug_name: "WhenClauseSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the when keyword.
                when_keyword;
                /// Appends the when keyword.
                push_when_keyword;
                kind: SyntaxKind::WhenKeyword;
                slot: "when_keyword";
            },
            {
                /// Returns the open paren token.
                open_paren_token;
                /// Appends the open paren token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "open_paren_token";
            },
            {
                /// Returns the close paren token.
                close_paren_token;
                /// Appends the close paren token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "close_paren_token";
            },
            {
                /// Returns the open brace token.
                open_brace_token;
                /// Appends the open brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "open_brace_token";
            },
            {
                /// Returns the close brace token.
                close_brace_token;
                /// Appends the close brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the execution-entry condition.
                condition;
                /// Appends the execution-entry condition.
                push_condition;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
        repeated_children: [
            {
                /// Returns ensures clauses.
                ensures_clauses;
                /// Appends a postcondition clause.
                push_ensures_clause;
                ty: EnsuresClauseSyntax;
                kind: SyntaxKind::EnsuresClause;
            },
            {
                /// Returns executes clauses.
                executes_clauses;
                /// Appends an execution-property clause.
                push_executes_clause;
                ty: ExecutesClauseSyntax;
                kind: SyntaxKind::ExecutesClause;
            },
            {
                /// Returns when clauses.
                when_clauses;
                /// Appends a nested guarded guarantee group.
                push_when_clause;
                ty: WhenClauseSyntax;
                kind: SyntaxKind::WhenClause;
            }
        ],
    }
}
