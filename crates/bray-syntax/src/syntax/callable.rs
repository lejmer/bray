use super::expression::first_expression;
use crate::node::define_source_syntax_node;
use crate::{
    AbiDirectiveSyntax, BlockExpressionSyntax, ExpressionSyntax, SyntaxKind, SyntaxToken,
    TypeExpressionSyntax, TypedIdentifierSyntax,
};

define_source_syntax_node! {
    /// Callable directives in source order.
    pub struct CallableDirectivesSyntax {
        builder: CallableDirectivesSyntaxBuilder,
        kind: SyntaxKind::CallableDirectives,
        source_slot: "callable_directives.source",
        node_name: "callable directives",
        range_description: "callable-directives",
        debug_name: "CallableDirectivesSyntax",
        builder_debug_name: "CallableDirectivesSyntaxBuilder",
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
            }
        ],
    }
}

define_source_syntax_node! {
    /// Optional callable modifiers in source order.
    pub struct CallableModifiersSyntax {
        builder: CallableModifiersSyntaxBuilder,
        kind: SyntaxKind::CallableModifiers,
        source_slot: "callable_modifiers.source",
        node_name: "callable modifiers",
        range_description: "callable-modifiers",
        debug_name: "CallableModifiersSyntax",
        builder_debug_name: "CallableModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `async` modifier token.
                async_token;
                /// Appends an `async` modifier token.
                push_async_token;
                kind: SyntaxKind::AsyncKeyword;
                slot: "callable_modifiers.async_token";
            },
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "callable_modifiers.trusted_token";
            },
            {
                /// Returns the first optional `const` modifier token.
                const_token;
                /// Appends a `const` modifier token.
                push_const_token;
                kind: SyntaxKind::ConstKeyword;
                slot: "callable_modifiers.const_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Optional callable parameter modifiers in source order.
    pub struct ParameterModifiersSyntax {
        builder: ParameterModifiersSyntaxBuilder,
        kind: SyntaxKind::ParameterModifiers,
        source_slot: "parameter_modifiers.source",
        node_name: "parameter modifiers",
        range_description: "parameter-modifiers",
        debug_name: "ParameterModifiersSyntax",
        builder_debug_name: "ParameterModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `pos` modifier token.
                pos_token;
                /// Appends a `pos` modifier token.
                push_pos_token;
                kind: SyntaxKind::PosKeyword;
                slot: "parameter_modifiers.pos_token";
            },
            {
                /// Returns the first optional `mut` modifier token.
                mut_token;
                /// Appends a `mut` modifier token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "parameter_modifiers.mut_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Callable parameter.
    pub struct ParameterSyntax {
        builder: ParameterSyntaxBuilder,
        kind: SyntaxKind::Parameter,
        source_slot: "parameter.source",
        node_name: "parameter",
        range_description: "parameter",
        debug_name: "ParameterSyntax",
        builder_debug_name: "ParameterSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional default-value equals token.
                equals_token;
                /// Appends the default-value equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "parameter.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the parameter-modifiers child.
                parameter_modifiers;
                /// Appends the parameter-modifiers child.
                push_parameter_modifiers;
                ty: ParameterModifiersSyntax;
                kind: SyntaxKind::ParameterModifiers;
            },
            {
                /// Returns the typed-identifier child.
                typed_identifier;
                /// Appends the typed-identifier child.
                push_typed_identifier;
                ty: TypedIdentifierSyntax;
                kind: SyntaxKind::TypedIdentifier;
            }
        ],
        repeated_children: [
            {
                /// Returns default expression children in source order.
                expressions;
                /// Appends a default expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl ParameterSyntax {
    /// Returns the required parameter name token.
    pub fn identifier_token(&self) -> SyntaxToken {
        self.typed_identifier().identifier_token()
    }

    /// Returns the required colon token.
    pub fn colon_token(&self) -> SyntaxToken {
        self.typed_identifier().colon_token()
    }

    /// Returns the parameter type-expression child.
    pub fn type_expression(&self) -> TypeExpressionSyntax {
        self.typed_identifier().type_expression()
    }

    /// Returns the default expression child when present.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

define_source_syntax_node! {
    /// Callable parameter list including delimiters.
    pub struct ParameterListSyntax {
        builder: ParameterListSyntaxBuilder,
        kind: SyntaxKind::ParameterList,
        source_slot: "parameter_list.source",
        node_name: "parameter list",
        range_description: "parameter-list",
        debug_name: "ParameterListSyntax",
        builder_debug_name: "ParameterListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "parameter_list.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "parameter_list.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "parameter_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns parameter children in source order.
                parameters;
                /// Appends a parameter child in source order.
                push_parameter;
                ty: ParameterSyntax;
                kind: SyntaxKind::Parameter;
            }
        ],
    }
}

impl ParameterListSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Callable result clause.
    pub struct CallableResultClauseSyntax {
        builder: CallableResultClauseSyntaxBuilder,
        kind: SyntaxKind::CallableResultClause,
        source_slot: "callable_result_clause.source",
        node_name: "callable result clause",
        range_description: "callable-result-clause",
        debug_name: "CallableResultClauseSyntax",
        builder_debug_name: "CallableResultClauseSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required arrow token.
                arrow_token;
                /// Appends the arrow token.
                push_arrow_token;
                kind: SyntaxKind::ArrowToken;
                slot: "callable_result_clause.arrow_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the result type-expression child.
                type_expression;
                /// Appends the result type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Callable body block expression.
    pub struct CallableBodyBlockExpressionSyntax {
        builder: CallableBodyBlockExpressionSyntaxBuilder,
        kind: SyntaxKind::CallableBodyBlockExpression,
        source_slot: "callable_body_block_expression.source",
        node_name: "callable body block expression",
        range_description: "callable-body-block-expression",
        debug_name: "CallableBodyBlockExpressionSyntax",
        builder_debug_name: "CallableBodyBlockExpressionSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the block-expression child.
                block_expression;
                /// Appends the block-expression child.
                push_block_expression;
                ty: BlockExpressionSyntax;
                kind: SyntaxKind::BlockExpression;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextRange, TextSize};

    use crate::test_support::{
        block_expression, identifier_type_expression, keyword, snapshot as test_snapshot, token,
        typed_identifier,
    };
    use crate::{
        AbiDirectiveSyntax, CallableBodyBlockExpressionSyntax, CallableDirectivesSyntax,
        CallableModifiersSyntax, CallableResultClauseSyntax, DirectiveArgumentListSyntax,
        ParameterListSyntax, ParameterModifiersSyntax, ParameterSyntax, SyntaxKind, SyntaxText,
        SyntaxToken, SyntaxTrivia,
    };

    #[test]
    fn callable_roots_store_directives_and_modifiers() {
        let snapshot = test_snapshot("syntax-callable-test", "@abi() async trusted const");

        let mut arguments =
            DirectiveArgumentListSyntax::builder(snapshot.clone(), TextSize::new(4));

        arguments.push_open_paren_token(token(SyntaxKind::OpenParenToken, 4, 5));
        arguments.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 5, 6, true));

        let mut abi = AbiDirectiveSyntax::builder(snapshot.clone(), TextSize::ZERO);

        abi.push_directive_marker_token(token(SyntaxKind::AtToken, 0, 1));
        abi.push_name_token(token(SyntaxKind::IdentifierToken, 1, 4));
        abi.push_directive_argument_list(arguments.build());

        let mut directives = CallableDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO);
        let mut modifiers = CallableModifiersSyntax::builder(snapshot, TextSize::new(7));

        directives.push_abi_directive(abi.build());

        modifiers.push_async_token(keyword(SyntaxKind::AsyncKeyword, 7, 12, true));
        modifiers.push_trusted_token(keyword(SyntaxKind::TrustedKeyword, 13, 20, true));
        modifiers.push_const_token(token(SyntaxKind::ConstKeyword, 21, 26));

        let directives = directives.build();
        let modifiers = modifiers.build();

        assert_eq!(directives.full_text(), "@abi() ");
        assert_eq!(directives.abi_directives().count(), 1);
        assert_eq!(modifiers.full_text(), "async trusted const");

        assert!(modifiers.async_token().is_some());
        assert!(modifiers.trusted_token().is_some());
        assert!(modifiers.const_token().is_some());
    }

    #[test]
    fn parameter_lists_store_parameters_separators_and_defaults() {
        let snapshot = test_snapshot("syntax-callable-test", "pos value: Int = 1, tail: Bool");

        let first = parameter(snapshot.clone(), 0, true);
        let second = parameter(snapshot.clone(), 20, false);

        let comma =
            token(SyntaxKind::CommaToken, 18, 19).with_trailing_trivia([SyntaxTrivia::whitespace(
                TextRange::new(TextSize::new(19), TextSize::new(20)),
            )]);

        let mut builder = ParameterListSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_open_paren_token(SyntaxToken::missing(
            SyntaxKind::OpenParenToken,
            TextSize::ZERO,
        ));

        builder.push_parameter(first);
        builder.push_separator_token(comma);
        builder.push_parameter(second);

        builder.push_close_paren_token(SyntaxToken::missing(
            SyntaxKind::CloseParenToken,
            TextSize::new(30),
        ));

        let list = builder.build();

        assert_eq!(list.parameters().count(), 2);
        assert_eq!(list.separator_tokens().count(), 1);
        assert_eq!(list.full_text(), "pos value: Int = 1, tail: Bool");
    }

    #[test]
    fn callable_result_clauses_store_type_and_body_blocks() {
        let snapshot = test_snapshot("syntax-callable-test", "-> Int {}");

        let mut result_builder =
            CallableResultClauseSyntax::builder(snapshot.clone(), TextSize::ZERO);

        result_builder.push_arrow_token(token(SyntaxKind::ArrowToken, 0, 2).with_trailing_trivia(
            [SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(2),
                TextSize::new(3),
            ))],
        ));

        result_builder.push_type_expression(identifier_type_expression(snapshot.clone(), 3, 6));

        let result = result_builder.build();

        let mut body_builder =
            CallableBodyBlockExpressionSyntax::builder(snapshot.clone(), TextSize::new(7));

        body_builder.push_block_expression(block_expression(snapshot, 7, false));

        let body = body_builder.build();

        assert_eq!(result.full_text(), "-> Int");
        assert_eq!(result.type_expression().full_text(), "Int");
        assert_eq!(result.skipped_syntax().count(), 0);
        assert_eq!(body.full_text(), "{}");
        assert_eq!(body.block_expression().full_text(), "{}");
    }

    fn parameter(snapshot: SourceSnapshot, start: u32, has_default: bool) -> ParameterSyntax {
        let builder_source = snapshot.clone();

        let mut builder = ParameterSyntax::builder(snapshot.clone(), TextSize::new(start));
        let mut modifiers = ParameterModifiersSyntax::builder(snapshot, TextSize::new(start));

        if has_default {
            modifiers.push_pos_token(
                token(SyntaxKind::PosKeyword, start, start + 3).with_trailing_trivia([
                    SyntaxTrivia::whitespace(TextRange::new(
                        TextSize::new(start + 3),
                        TextSize::new(start + 4),
                    )),
                ]),
            );

            builder.push_parameter_modifiers(modifiers.build());
            builder.push_typed_identifier(typed_identifier(
                builder_source.clone(),
                start + 4,
                start + 9,
                start + 11,
                start + 14,
                true,
            ));

            builder.push_equals_token(
                token(SyntaxKind::EqualsToken, start + 15, start + 16).with_trailing_trivia([
                    SyntaxTrivia::whitespace(TextRange::new(
                        TextSize::new(start + 16),
                        TextSize::new(start + 17),
                    )),
                ]),
            );

            builder.push_skipped_tokens([token(
                SyntaxKind::DecimalIntegerLiteralToken,
                start + 17,
                start + 18,
            )]);
        } else {
            builder.push_parameter_modifiers(modifiers.build());
            builder.push_typed_identifier(typed_identifier(
                builder_source,
                start,
                start + 4,
                start + 6,
                start + 10,
                false,
            ));
        }

        builder.build()
    }
}
