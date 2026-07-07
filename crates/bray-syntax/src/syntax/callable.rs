use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken};

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
        required_tokens: [
            {
                /// Returns the required parameter name token.
                identifier_token;
                /// Appends the parameter name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "parameter.identifier_token";
            },
            {
                /// Returns the required colon token.
                colon_token;
                /// Appends the colon token.
                push_colon_token;
                kind: SyntaxKind::ColonToken;
                slot: "parameter.colon_token";
            }
        ],
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
            }
        ],
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
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Callable body block expression with skipped expression contents.
    pub struct CallableBodyBlockExpressionSyntax {
        builder: CallableBodyBlockExpressionSyntaxBuilder,
        kind: SyntaxKind::CallableBodyBlockExpression,
        source_slot: "callable_body_block_expression.source",
        node_name: "callable body block expression",
        range_description: "callable-body-block-expression",
        debug_name: "CallableBodyBlockExpressionSyntax",
        builder_debug_name: "CallableBodyBlockExpressionSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "callable_body_block_expression.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "callable_body_block_expression.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextRange, TextSize};

    use crate::test_support::{snapshot as test_snapshot, token};
    use crate::{
        CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
        ParameterModifiersSyntax, ParameterSyntax, SyntaxKind, SyntaxText, SyntaxToken,
        SyntaxTrivia,
    };

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

    // TODO(syntax): Update this when callable result and block contents are typed syntax.
    #[test]
    fn callable_result_clauses_and_body_blocks_store_skipped_contents() {
        let snapshot = test_snapshot("syntax-callable-test", "-> Int { return }");
        let mut result_builder =
            CallableResultClauseSyntax::builder(snapshot.clone(), TextSize::ZERO);

        result_builder.push_arrow_token(token(SyntaxKind::ArrowToken, 0, 2).with_trailing_trivia(
            [SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(2),
                TextSize::new(3),
            ))],
        ));
        result_builder.push_skipped_tokens([token(SyntaxKind::IdentifierToken, 3, 6)]);

        let result = result_builder.build();

        let mut body_builder =
            CallableBodyBlockExpressionSyntax::builder(snapshot, TextSize::new(7));

        body_builder.push_open_brace_token(
            token(SyntaxKind::OpenBraceToken, 7, 8).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(8), TextSize::new(9))),
            ]),
        );
        body_builder.push_skipped_tokens([token(SyntaxKind::ReturnKeyword, 9, 15)
            .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(15),
                TextSize::new(16),
            ))])]);
        body_builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 16, 17));

        let body = body_builder.build();

        assert_eq!(result.full_text(), "-> Int");
        assert_eq!(result.skipped_syntax().count(), 1);
        assert_eq!(body.full_text(), "{ return }");
        assert_eq!(body.skipped_syntax().count(), 1);
    }

    fn parameter(snapshot: SourceSnapshot, start: u32, has_default: bool) -> ParameterSyntax {
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
            builder.push_identifier_token(token(SyntaxKind::IdentifierToken, start + 4, start + 9));
            builder.push_colon_token(
                token(SyntaxKind::ColonToken, start + 9, start + 10).with_trailing_trivia([
                    SyntaxTrivia::whitespace(TextRange::new(
                        TextSize::new(start + 10),
                        TextSize::new(start + 11),
                    )),
                ]),
            );
            builder.push_skipped_tokens([token(
                SyntaxKind::IdentifierToken,
                start + 11,
                start + 14,
            )
            .with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(start + 14),
                TextSize::new(start + 15),
            ))])]);
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
            builder.push_identifier_token(token(SyntaxKind::IdentifierToken, start, start + 4));
            builder.push_colon_token(
                token(SyntaxKind::ColonToken, start + 4, start + 5).with_trailing_trivia([
                    SyntaxTrivia::whitespace(TextRange::new(
                        TextSize::new(start + 5),
                        TextSize::new(start + 6),
                    )),
                ]),
            );
            builder.push_skipped_tokens([token(
                SyntaxKind::IdentifierToken,
                start + 6,
                start + 10,
            )]);
        }

        builder.build()
    }
}
