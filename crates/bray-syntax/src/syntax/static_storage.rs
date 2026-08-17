use super::expression::first_expression;
use crate::node::define_source_syntax_node;
use crate::{
    ExpressionSyntax, GenericParameterListSyntax, SyntaxKind, SyntaxToken,
    ThreadLocalDirectiveSyntax, TypeExpressionSyntax, WithClauseSyntax,
};

define_source_syntax_node! {
    /// Static declaration directives in source order.
    pub struct StaticDirectivesSyntax {
        builder: StaticDirectivesSyntaxBuilder,
        kind: SyntaxKind::StaticDirectives,
        source_slot: "static_directives.source",
        node_name: "static directives",
        range_description: "static-directives",
        debug_name: "StaticDirectivesSyntax",
        builder_debug_name: "StaticDirectivesSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns `@thread_local` directives in source order.
                thread_local_directives;
                /// Appends an `@thread_local` directive.
                push_thread_local_directive;
                ty: ThreadLocalDirectiveSyntax;
                kind: SyntaxKind::ThreadLocalDirective;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Optional static declaration modifiers in source order.
    pub struct StaticDeclarationModifiersSyntax {
        builder: StaticDeclarationModifiersSyntaxBuilder,
        kind: SyntaxKind::StaticDeclarationModifiers,
        source_slot: "static_declaration_modifiers.source",
        node_name: "static declaration modifiers",
        range_description: "static-declaration-modifiers",
        debug_name: "StaticDeclarationModifiersSyntax",
        builder_debug_name: "StaticDeclarationModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl StaticDeclarationModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl StaticDeclarationModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "static_declaration_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Module-level static storage declaration.
    pub struct StaticDeclarationSyntax {
        builder: StaticDeclarationSyntaxBuilder,
        kind: SyntaxKind::StaticDeclaration,
        source_slot: "static_declaration.source",
        node_name: "static declaration",
        range_description: "static-declaration",
        debug_name: "StaticDeclarationSyntax",
        builder_debug_name: "StaticDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `static` keyword token.
                static_keyword;
                /// Appends the `static` keyword token.
                push_static_keyword;
                kind: SyntaxKind::StaticKeyword;
                slot: "static_declaration.static_keyword";
            },
            {
                /// Returns the required static name token.
                identifier_token;
                /// Appends the static name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "static_declaration.identifier_token";
            },
            {
                /// Returns the required type separator token.
                colon_token;
                /// Appends the type separator token.
                push_colon_token;
                kind: SyntaxKind::ColonToken;
                slot: "static_declaration.colon_token";
            },
            {
                /// Returns the required initializer equals token.
                equals_token;
                /// Appends the initializer equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "static_declaration.equals_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "static_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the static directives child.
                static_directives;
                /// Appends the static directives child.
                push_static_directives;
                ty: StaticDirectivesSyntax;
                kind: SyntaxKind::StaticDirectives;
            },
            {
                /// Returns the static declaration modifiers child.
                static_declaration_modifiers;
                /// Appends the static declaration modifiers child.
                push_static_declaration_modifiers;
                ty: StaticDeclarationModifiersSyntax;
                kind: SyntaxKind::StaticDeclarationModifiers;
            },
            {
                /// Returns the declared static type.
                type_expression;
                /// Appends the declared static type.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            }
        ],
        repeated_children: [
            {
                /// Returns generic parameter lists in source order.
                generic_parameter_lists;
                /// Appends a generic parameter list.
                push_generic_parameter_list;
                ty: GenericParameterListSyntax;
                kind: SyntaxKind::GenericParameterList;
            },
            {
                /// Returns constraint clauses in source order.
                with_clauses;
                /// Appends a constraint clause.
                push_with_clause;
                ty: WithClauseSyntax;
                kind: SyntaxKind::WithClause;
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

impl StaticDeclarationSyntax {
    /// Returns the optional generic parameter list.
    pub fn generic_parameter_list(&self) -> Option<GenericParameterListSyntax> {
        self.generic_parameter_lists().next()
    }

    /// Returns the initializer expression when present.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}
