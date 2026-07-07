use super::callable::{
    CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
};
use super::directive::{
    AbiDirectiveSyntax, EntrypointDirectiveSyntax, LinkDirectiveSyntax, SymbolDirectiveSyntax,
    TestDirectiveSyntax,
};
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Function directives in source order.
    pub struct FunctionDirectivesSyntax {
        builder: FunctionDirectivesSyntaxBuilder,
        kind: SyntaxKind::FunctionDirectives,
        source_slot: "function_directives.source",
        node_name: "function directives",
        range_description: "function-directives",
        debug_name: "FunctionDirectivesSyntax",
        builder_debug_name: "FunctionDirectivesSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns `@abi(...)` directives in source order.
                abi_directives;
                /// Appends an `@abi(...)` directive.
                push_abi_directive;
                ty: AbiDirectiveSyntax;
                kind: SyntaxKind::AbiDirective;
            },
            {
                /// Returns `@link(...)` directives in source order.
                link_directives;
                /// Appends a `@link(...)` directive.
                push_link_directive;
                ty: LinkDirectiveSyntax;
                kind: SyntaxKind::LinkDirective;
            },
            {
                /// Returns `@symbol(...)` directives in source order.
                symbol_directives;
                /// Appends a `@symbol(...)` directive.
                push_symbol_directive;
                ty: SymbolDirectiveSyntax;
                kind: SyntaxKind::SymbolDirective;
            },
            {
                /// Returns `@entrypoint` directives in source order.
                entrypoint_directives;
                /// Appends an `@entrypoint` directive.
                push_entrypoint_directive;
                ty: EntrypointDirectiveSyntax;
                kind: SyntaxKind::EntrypointDirective;
            },
            {
                /// Returns `@test` directives in source order.
                test_directives;
                /// Appends a `@test` directive.
                push_test_directive;
                ty: TestDirectiveSyntax;
                kind: SyntaxKind::TestDirective;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Optional function declaration modifiers in source order.
    pub struct FunctionModifiersSyntax {
        builder: FunctionModifiersSyntaxBuilder,
        kind: SyntaxKind::FunctionModifiers,
        source_slot: "function_modifiers.source",
        node_name: "function modifiers",
        range_description: "function-modifiers",
        debug_name: "FunctionModifiersSyntax",
        builder_debug_name: "FunctionModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `extern` modifier token.
                extern_token;
                /// Appends an `extern` modifier token.
                push_extern_token;
                kind: SyntaxKind::ExternKeyword;
                slot: "function_modifiers.extern_token";
            },
            {
                /// Returns the first optional `async` modifier token.
                async_token;
                /// Appends an `async` modifier token.
                push_async_token;
                kind: SyntaxKind::AsyncKeyword;
                slot: "function_modifiers.async_token";
            },
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "function_modifiers.trusted_token";
            },
            {
                /// Returns the first optional `const` modifier token.
                const_token;
                /// Appends a `const` modifier token.
                push_const_token;
                kind: SyntaxKind::ConstKeyword;
                slot: "function_modifiers.const_token";
            }
        ],
        required_children: [],
    }
}

impl FunctionModifiersSyntax {
    /// Returns the first visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl FunctionModifiersSyntaxBuilder {
    /// Appends a visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "function_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Module-level function declaration.
    pub struct FunctionDeclarationSyntax {
        builder: FunctionDeclarationSyntaxBuilder,
        kind: SyntaxKind::FunctionDeclaration,
        source_slot: "function_declaration.source",
        node_name: "function declaration",
        range_description: "function-declaration",
        debug_name: "FunctionDeclarationSyntax",
        builder_debug_name: "FunctionDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `func` keyword token.
                func_keyword;
                /// Appends the `func` keyword token.
                push_func_keyword;
                kind: SyntaxKind::FuncKeyword;
                slot: "function_declaration.func_keyword";
            },
            {
                /// Returns the required function name token.
                identifier_token;
                /// Appends the function name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "function_declaration.identifier_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional semicolon tail token.
                semicolon_token;
                /// Appends the semicolon tail token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "function_declaration.semicolon_token";
            }
        ],
        required_children: [
            {
                /// Returns the function-directives child.
                function_directives;
                /// Appends the function-directives child.
                push_function_directives;
                ty: FunctionDirectivesSyntax;
                kind: SyntaxKind::FunctionDirectives;
            },
            {
                /// Returns the function-modifiers child.
                function_modifiers;
                /// Appends the function-modifiers child.
                push_function_modifiers;
                ty: FunctionModifiersSyntax;
                kind: SyntaxKind::FunctionModifiers;
            },
            {
                /// Returns the parameter-list child.
                parameter_list;
                /// Appends the parameter-list child.
                push_parameter_list;
                ty: ParameterListSyntax;
                kind: SyntaxKind::ParameterList;
            }
        ],
        repeated_children: [
            {
                /// Returns callable result clauses in source order.
                callable_result_clauses;
                /// Appends a callable result clause child.
                push_callable_result_clause;
                ty: CallableResultClauseSyntax;
                kind: SyntaxKind::CallableResultClause;
            },
            {
                /// Returns callable body block expressions in source order.
                callable_body_block_expressions;
                /// Appends a callable body block expression child.
                push_callable_body_block_expression;
                ty: CallableBodyBlockExpressionSyntax;
                kind: SyntaxKind::CallableBodyBlockExpression;
            }
        ],
    }
}

impl FunctionDeclarationSyntax {
    /// Returns the callable result clause child when present.
    pub fn callable_result_clause(&self) -> Option<CallableResultClauseSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableResultClause,
            CallableResultClauseSyntax::from_green,
        )
        .next()
    }

    /// Returns the callable body block expression child when present.
    pub fn callable_body_block_expression(&self) -> Option<CallableBodyBlockExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableBodyBlockExpression,
            CallableBodyBlockExpressionSyntax::from_green,
        )
        .next()
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextRange, TextSize};

    use crate::test_support::{snapshot as test_snapshot, token};
    use crate::{
        CallableBodyBlockExpressionSyntax, DirectiveArgumentListSyntax, FunctionDeclarationSyntax,
        FunctionDirectivesSyntax, FunctionModifiersSyntax, LinkDirectiveSyntax,
        ParameterListSyntax, SyntaxKind, SyntaxText, SyntaxTrivia,
    };

    #[test]
    fn function_declarations_store_modifiers_parameters_and_body() {
        let snapshot = test_snapshot("syntax-function-test", "public func main() {}");
        let mut builder = FunctionDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_function_directives(
            FunctionDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );
        builder.push_function_modifiers(function_modifiers(snapshot.clone()));
        builder.push_func_keyword(token(SyntaxKind::FuncKeyword, 7, 11).with_trailing_trivia([
            SyntaxTrivia::whitespace(TextRange::new(TextSize::new(11), TextSize::new(12))),
        ]));
        builder.push_identifier_token(token(SyntaxKind::IdentifierToken, 12, 16));
        builder.push_parameter_list(parameter_list(snapshot.clone()));
        builder.push_callable_body_block_expression(body(snapshot));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "public func main() {}");

        assert_eq!(
            declaration
                .function_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(declaration.parameter_list().full_text(), "() ");
        assert!(declaration.semicolon_token().is_none());
        assert!(declaration.callable_body_block_expression().is_some());
    }

    // TODO(syntax): Update this when directive arguments are typed syntax.
    #[test]
    fn function_directives_store_directives_in_source_order() {
        let snapshot = test_snapshot("syntax-function-test", "@link(\"m\")");
        let mut directives = FunctionDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut link = LinkDirectiveSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut arguments =
            DirectiveArgumentListSyntax::builder(snapshot.clone(), TextSize::new(5));

        arguments.push_open_paren_token(token(SyntaxKind::OpenParenToken, 5, 6));
        arguments.push_skipped_tokens([token(SyntaxKind::StringLiteralToken, 6, 9)]);
        arguments.push_close_paren_token(token(SyntaxKind::CloseParenToken, 9, 10));

        link.push_directive_marker_token(token(SyntaxKind::AtToken, 0, 1));
        link.push_name_token(token(SyntaxKind::IdentifierToken, 1, 5));
        link.push_directive_argument_list(arguments.build());

        directives.push_link_directive(link.build());

        let directives = directives.build();

        assert_eq!(directives.full_text(), "@link(\"m\")");
        assert_eq!(directives.link_directives().count(), 1);
        assert_eq!(directives.skipped_syntax().count(), 1);
    }

    fn function_modifiers(snapshot: SourceSnapshot) -> FunctionModifiersSyntax {
        let mut builder = FunctionModifiersSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_visibility_token(token(SyntaxKind::PublicKeyword, 0, 6).with_trailing_trivia(
            [SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(6),
                TextSize::new(7),
            ))],
        ));

        builder.build()
    }

    fn parameter_list(snapshot: SourceSnapshot) -> ParameterListSyntax {
        let mut builder = ParameterListSyntax::builder(snapshot, TextSize::new(16));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 16, 17));
        builder.push_close_paren_token(
            token(SyntaxKind::CloseParenToken, 17, 18).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(18), TextSize::new(19))),
            ]),
        );

        builder.build()
    }

    fn body(snapshot: SourceSnapshot) -> CallableBodyBlockExpressionSyntax {
        let mut builder = CallableBodyBlockExpressionSyntax::builder(snapshot, TextSize::new(19));

        builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, 19, 20));
        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 20, 21));

        builder.build()
    }
}
