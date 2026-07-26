use super::expression::first_expression;
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    ExpressionSyntax, GenericParameterListSyntax, SyntaxKind, SyntaxToken, TypeExpressionSyntax,
    TypedIdentifierSyntax,
};

define_source_syntax_node! {
    /// Optional predicate modifiers in source order.
    pub struct PredicateModifiersSyntax {
        builder: PredicateModifiersSyntaxBuilder,
        kind: SyntaxKind::PredicateModifiers,
        source_slot: "predicate_modifiers.source",
        node_name: "predicate modifiers",
        range_description: "predicate-modifiers",
        debug_name: "PredicateModifiersSyntax",
        builder_debug_name: "PredicateModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "predicate_modifiers.trusted_token";
            }
        ],
        required_children: [],
    }
}

impl PredicateModifiersSyntax {
    /// Returns the first visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl PredicateModifiersSyntaxBuilder {
    /// Appends a visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "predicate_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Predicate parameter item.
    pub struct PredicateParameterSyntax {
        builder: PredicateParameterSyntaxBuilder,
        kind: SyntaxKind::PredicateParameter,
        source_slot: "predicate_parameter.source",
        node_name: "predicate parameter",
        range_description: "predicate-parameter",
        debug_name: "PredicateParameterSyntax",
        builder_debug_name: "PredicateParameterSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the typed-identifier child.
                typed_identifier;
                /// Appends the typed-identifier child.
                push_typed_identifier;
                ty: TypedIdentifierSyntax;
                kind: SyntaxKind::TypedIdentifier;
            }
        ],
    }
}

impl PredicateParameterSyntax {
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
}

define_source_syntax_node! {
    /// Predicate parameter list including delimiters.
    pub struct PredicateParameterListSyntax {
        builder: PredicateParameterListSyntaxBuilder,
        kind: SyntaxKind::PredicateParameterList,
        source_slot: "predicate_parameter_list.source",
        node_name: "predicate parameter list",
        range_description: "predicate-parameter-list",
        debug_name: "PredicateParameterListSyntax",
        builder_debug_name: "PredicateParameterListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "predicate_parameter_list.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "predicate_parameter_list.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "predicate_parameter_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns predicate parameter children in source order.
                predicate_parameters;
                /// Appends a predicate parameter child in source order.
                push_predicate_parameter;
                ty: PredicateParameterSyntax;
                kind: SyntaxKind::PredicateParameter;
            }
        ],
    }
}

impl PredicateParameterListSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Module-level predicate declaration.
    pub struct PredicateDeclarationSyntax {
        builder: PredicateDeclarationSyntaxBuilder,
        kind: SyntaxKind::PredicateDeclaration,
        source_slot: "predicate_declaration.source",
        node_name: "predicate declaration",
        range_description: "predicate-declaration",
        debug_name: "PredicateDeclarationSyntax",
        builder_debug_name: "PredicateDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `predicate` keyword token.
                predicate_keyword;
                /// Appends the `predicate` keyword token.
                push_predicate_keyword;
                kind: SyntaxKind::PredicateKeyword;
                slot: "predicate_declaration.predicate_keyword";
            },
            {
                /// Returns the required predicate name token.
                identifier_token;
                /// Appends the predicate name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "predicate_declaration.identifier_token";
            },
            {
                /// Returns the required semicolon tail token.
                semicolon_token;
                /// Appends the semicolon tail token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "predicate_declaration.semicolon_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional predicate-body equals token.
                equals_token;
                /// Appends the predicate-body equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "predicate_declaration.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the predicate-modifiers child.
                predicate_modifiers;
                /// Appends the predicate-modifiers child.
                push_predicate_modifiers;
                ty: PredicateModifiersSyntax;
                kind: SyntaxKind::PredicateModifiers;
            },
            {
                /// Returns the predicate-parameter-list child.
                predicate_parameter_list;
                /// Appends the predicate-parameter-list child.
                push_predicate_parameter_list;
                ty: PredicateParameterListSyntax;
                kind: SyntaxKind::PredicateParameterList;
            }
        ],
        repeated_children: [
            {
                /// Returns generic parameter lists in source order.
                generic_parameter_lists;
                /// Appends a generic parameter list child.
                push_generic_parameter_list;
                ty: GenericParameterListSyntax;
                kind: SyntaxKind::GenericParameterList;
            },
            {
                /// Returns body expression children in source order.
                expressions;
                /// Appends a body expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl PredicateDeclarationSyntax {
    /// Returns the generic parameter list child when present.
    pub fn generic_parameter_list(&self) -> Option<GenericParameterListSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::GenericParameterList,
            GenericParameterListSyntax::from_green,
        )
        .next()
    }

    /// Returns the body expression child when present.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

define_source_syntax_node! {
    /// Optional trait predicate member modifiers in source order.
    pub struct TraitPredicateMemberModifiersSyntax {
        builder: TraitPredicateMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::TraitPredicateMemberModifiers,
        source_slot: "trait_predicate_member_modifiers.source",
        node_name: "trait predicate member modifiers",
        range_description: "trait-predicate-member-modifiers",
        debug_name: "TraitPredicateMemberModifiersSyntax",
        builder_debug_name: "TraitPredicateMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "trait_predicate_member_modifiers.trusted_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Trait predicate member declaration.
    pub struct TraitPredicateMemberDeclarationSyntax {
        builder: TraitPredicateMemberDeclarationSyntaxBuilder,
        kind: SyntaxKind::TraitPredicateMemberDeclaration,
        source_slot: "trait_predicate_member_declaration.source",
        node_name: "trait predicate member declaration",
        range_description: "trait-predicate-member-declaration",
        debug_name: "TraitPredicateMemberDeclarationSyntax",
        builder_debug_name: "TraitPredicateMemberDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `predicate` keyword token.
                predicate_keyword;
                /// Appends the `predicate` keyword token.
                push_predicate_keyword;
                kind: SyntaxKind::PredicateKeyword;
                slot: "trait_predicate_member_declaration.predicate_keyword";
            },
            {
                /// Returns the required predicate member name token.
                identifier_token;
                /// Appends the predicate member name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "trait_predicate_member_declaration.identifier_token";
            },
            {
                /// Returns the required semicolon tail token.
                semicolon_token;
                /// Appends the semicolon tail token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "trait_predicate_member_declaration.semicolon_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional default predicate-body equals token.
                equals_token;
                /// Appends the default predicate-body equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "trait_predicate_member_declaration.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the trait predicate member modifiers child.
                trait_predicate_member_modifiers;
                /// Appends the trait predicate member modifiers child.
                push_trait_predicate_member_modifiers;
                ty: TraitPredicateMemberModifiersSyntax;
                kind: SyntaxKind::TraitPredicateMemberModifiers;
            },
            {
                /// Returns the predicate-parameter-list child.
                predicate_parameter_list;
                /// Appends the predicate-parameter-list child.
                push_predicate_parameter_list;
                ty: PredicateParameterListSyntax;
                kind: SyntaxKind::PredicateParameterList;
            }
        ],
        repeated_children: [
            {
                /// Returns body expression children in source order.
                expressions;
                /// Appends a body expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl TraitPredicateMemberDeclarationSyntax {
    /// Returns the body expression child when present.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use crate::test_support::{
        snapshot as test_snapshot, token, token_expression_from_token, typed_identifier,
    };
    use crate::{
        ExpressionSyntax, PredicateDeclarationSyntax, PredicateModifiersSyntax,
        PredicateParameterListSyntax, PredicateParameterSyntax, SyntaxKind, SyntaxText,
        SyntaxTrivia, TraitPredicateMemberDeclarationSyntax, TraitPredicateMemberModifiersSyntax,
    };

    #[test]
    fn predicate_declarations_store_modifiers_parameters_and_tail() {
        let snapshot = test_snapshot(
            "syntax-predicate-test",
            "trusted predicate valid(value: Int) = value > 0;",
        );

        let mut builder = PredicateDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_predicate_modifiers(predicate_modifiers(snapshot.clone()));

        builder.push_predicate_keyword(
            token(SyntaxKind::PredicateKeyword, 8, 17).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(17), TextSize::new(18))),
            ]),
        );

        builder.push_identifier_token(token(SyntaxKind::IdentifierToken, 18, 23));
        builder.push_predicate_parameter_list(predicate_parameter_list(snapshot.clone()));

        builder.push_equals_token(
            token(SyntaxKind::EqualsToken, 35, 36).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(36), TextSize::new(37))),
            ]),
        );

        builder.push_expression(predicate_body_expression(snapshot.clone()));
        builder.push_semicolon_token(token(SyntaxKind::SemicolonToken, 46, 47));

        let declaration = builder.build();

        assert_eq!(
            declaration.full_text(),
            "trusted predicate valid(value: Int) = value > 0;"
        );

        assert_eq!(
            declaration
                .predicate_modifiers()
                .trusted_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert_eq!(
            declaration
                .predicate_parameter_list()
                .predicate_parameters()
                .count(),
            1
        );

        assert!(declaration.equals_token().is_some());

        assert_eq!(
            declaration
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value > 0"))
        );

        assert_eq!(declaration.skipped_syntax().count(), 0);
    }

    #[test]
    fn trait_predicate_member_declarations_store_modifiers_parameters_and_tail() {
        let snapshot = test_snapshot(
            "syntax-trait-predicate-test",
            "trusted predicate valid(value: Int) = value > 0;",
        );

        let mut builder =
            TraitPredicateMemberDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_trait_predicate_member_modifiers(trait_predicate_member_modifiers(
            snapshot.clone(),
        ));

        builder.push_predicate_keyword(
            token(SyntaxKind::PredicateKeyword, 8, 17).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(17), TextSize::new(18))),
            ]),
        );

        builder.push_identifier_token(token(SyntaxKind::IdentifierToken, 18, 23));
        builder.push_predicate_parameter_list(predicate_parameter_list(snapshot.clone()));

        builder.push_equals_token(
            token(SyntaxKind::EqualsToken, 35, 36).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(36), TextSize::new(37))),
            ]),
        );

        builder.push_expression(predicate_body_expression(snapshot.clone()));
        builder.push_semicolon_token(token(SyntaxKind::SemicolonToken, 46, 47));

        let declaration = builder.build();

        assert_eq!(
            declaration.full_text(),
            "trusted predicate valid(value: Int) = value > 0;"
        );

        assert_eq!(
            declaration
                .trait_predicate_member_modifiers()
                .trusted_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::TrustedKeyword)
        );

        assert_eq!(
            declaration
                .predicate_parameter_list()
                .predicate_parameters()
                .count(),
            1
        );

        assert!(declaration.equals_token().is_some());

        assert_eq!(
            declaration
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value > 0"))
        );

        assert_eq!(declaration.skipped_syntax().count(), 0);
    }

    fn predicate_modifiers(snapshot: bray_source::SourceSnapshot) -> PredicateModifiersSyntax {
        let mut builder = PredicateModifiersSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_trusted_token(
            token(SyntaxKind::TrustedKeyword, 0, 7).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(7), TextSize::new(8))),
            ]),
        );

        builder.build()
    }

    fn trait_predicate_member_modifiers(
        snapshot: bray_source::SourceSnapshot,
    ) -> TraitPredicateMemberModifiersSyntax {
        let mut builder = TraitPredicateMemberModifiersSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_trusted_token(
            token(SyntaxKind::TrustedKeyword, 0, 7).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(7), TextSize::new(8))),
            ]),
        );

        builder.build()
    }

    fn predicate_parameter_list(
        snapshot: bray_source::SourceSnapshot,
    ) -> PredicateParameterListSyntax {
        let mut builder =
            PredicateParameterListSyntax::builder(snapshot.clone(), TextSize::new(23));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 23, 24));
        builder.push_predicate_parameter(predicate_parameter(snapshot));

        builder.push_close_paren_token(
            token(SyntaxKind::CloseParenToken, 34, 35).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(35), TextSize::new(36))),
            ]),
        );

        builder.build()
    }

    fn predicate_parameter(snapshot: bray_source::SourceSnapshot) -> PredicateParameterSyntax {
        let mut builder = PredicateParameterSyntax::builder(snapshot.clone(), TextSize::new(24));

        builder.push_typed_identifier(typed_identifier(snapshot, 24, 29, 31, 34, false));

        builder.build()
    }

    fn predicate_body_expression(snapshot: bray_source::SourceSnapshot) -> ExpressionSyntax {
        let mut builder = ExpressionSyntax::builder(snapshot.clone(), TextSize::new(37));

        builder.push_expression(token_expression_from_token(
            snapshot.clone(),
            token(SyntaxKind::IdentifierToken, 37, 42).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(42), TextSize::new(43))),
            ]),
        ));

        builder.push_operator_token(
            token(SyntaxKind::GreaterToken, 43, 44).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(44), TextSize::new(45))),
            ]),
        );

        builder.push_expression(token_expression_from_token(
            snapshot,
            token(SyntaxKind::DecimalIntegerLiteralToken, 45, 46),
        ));

        builder.build()
    }
}
