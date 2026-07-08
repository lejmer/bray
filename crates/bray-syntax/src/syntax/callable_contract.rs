use crate::node::{child_nodes, define_source_syntax_node};
use crate::{GenericParameterListSyntax, SyntaxKind, SyntaxToken, TypeExpressionSyntax};

define_source_syntax_node! {
    /// Optional callable contract modifiers in source order.
    pub struct CallableContractModifiersSyntax {
        builder: CallableContractModifiersSyntaxBuilder,
        kind: SyntaxKind::CallableContractModifiers,
        source_slot: "callable_contract_modifiers.source",
        node_name: "callable contract modifiers",
        range_description: "callable-contract-modifiers",
        debug_name: "CallableContractModifiersSyntax",
        builder_debug_name: "CallableContractModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl CallableContractModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl CallableContractModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "callable_contract_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Module-level named callable contract declaration.
    pub struct CallableContractDeclarationSyntax {
        builder: CallableContractDeclarationSyntaxBuilder,
        kind: SyntaxKind::CallableContractDeclaration,
        source_slot: "callable_contract_declaration.source",
        node_name: "callable contract declaration",
        range_description: "callable-contract-declaration",
        debug_name: "CallableContractDeclarationSyntax",
        builder_debug_name: "CallableContractDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `callable` keyword token.
                callable_keyword;
                /// Appends the `callable` keyword token.
                push_callable_keyword;
                kind: SyntaxKind::CallableKeyword;
                slot: "callable_contract_declaration.callable_keyword";
            },
            {
                /// Returns the required callable contract name token.
                identifier_token;
                /// Appends the callable contract name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "callable_contract_declaration.identifier_token";
            },
            {
                /// Returns the required equals token before the callable type.
                equals_token;
                /// Appends the equals token before the callable type.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "callable_contract_declaration.equals_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "callable_contract_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the callable-contract-modifiers child.
                callable_contract_modifiers;
                /// Appends the callable-contract-modifiers child.
                push_callable_contract_modifiers;
                ty: CallableContractModifiersSyntax;
                kind: SyntaxKind::CallableContractModifiers;
            },
            {
                /// Returns the callable type-expression child.
                type_expression;
                /// Appends the callable type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
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
            }
        ],
    }
}

impl CallableContractDeclarationSyntax {
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
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use crate::test_support::{snapshot as test_snapshot, token};
    use crate::{
        CallableContractDeclarationSyntax, CallableContractModifiersSyntax, SyntaxKind, SyntaxText,
        SyntaxTrivia, TypeExpressionSyntax,
    };

    // TODO(syntax): Update this when callable contract generics and constraints are typed syntax.
    #[test]
    fn callable_contract_declarations_store_modifier_name_and_type_form() {
        let snapshot = test_snapshot(
            "syntax-callable-contract-test",
            "public callable Mapper = func(value: Int) -> Bool;",
        );

        let mut builder =
            CallableContractDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_callable_contract_modifiers(callable_contract_modifiers(snapshot.clone()));

        builder.push_callable_keyword(
            token(SyntaxKind::CallableKeyword, 7, 15).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(15), TextSize::new(16))),
            ]),
        );

        builder.push_identifier_token(
            token(SyntaxKind::IdentifierToken, 16, 22).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(22), TextSize::new(23))),
            ]),
        );

        builder.push_equals_token(
            token(SyntaxKind::EqualsToken, 23, 24).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(24), TextSize::new(25))),
            ]),
        );

        builder.push_type_expression(callable_type_expression(snapshot.clone()));
        builder.push_semicolon_token(token(SyntaxKind::SemicolonToken, 49, 50));

        let declaration = builder.build();

        assert_eq!(
            declaration.full_text(),
            "public callable Mapper = func(value: Int) -> Bool;"
        );

        assert_eq!(
            declaration
                .callable_contract_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            declaration.type_expression().full_text(),
            "func(value: Int) -> Bool"
        );

        assert_eq!(declaration.skipped_syntax().count(), 1);
    }

    fn callable_contract_modifiers(
        snapshot: bray_source::SourceSnapshot,
    ) -> CallableContractModifiersSyntax {
        let mut builder = CallableContractModifiersSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_visibility_token(token(SyntaxKind::PublicKeyword, 0, 6).with_trailing_trivia(
            [SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(6),
                TextSize::new(7),
            ))],
        ));

        builder.build()
    }

    fn callable_type_expression(snapshot: bray_source::SourceSnapshot) -> TypeExpressionSyntax {
        let mut builder = TypeExpressionSyntax::builder(snapshot, TextSize::new(25));

        builder.push_skipped_tokens([
            token(SyntaxKind::FuncKeyword, 25, 29),
            token(SyntaxKind::OpenParenToken, 29, 30),
            token(SyntaxKind::IdentifierToken, 30, 35),
            token(SyntaxKind::ColonToken, 35, 36).with_trailing_trivia([SyntaxTrivia::whitespace(
                TextRange::new(TextSize::new(36), TextSize::new(37)),
            )]),
            token(SyntaxKind::IdentifierToken, 37, 40),
            token(SyntaxKind::CloseParenToken, 40, 41).with_trailing_trivia([
                SyntaxTrivia::whitespace(TextRange::new(TextSize::new(41), TextSize::new(42))),
            ]),
            token(SyntaxKind::ArrowToken, 42, 44).with_trailing_trivia([SyntaxTrivia::whitespace(
                TextRange::new(TextSize::new(44), TextSize::new(45)),
            )]),
            token(SyntaxKind::IdentifierToken, 45, 49),
        ]);

        builder.build()
    }
}
