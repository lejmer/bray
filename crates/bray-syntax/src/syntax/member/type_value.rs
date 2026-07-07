use crate::SyntaxKind;
use crate::node::define_source_syntax_node;

define_source_syntax_node! {
    /// Type-valued member binding inside an implementation body.
    pub struct ImplementationTypeMemberBindingSyntax {
        builder: ImplementationTypeMemberBindingSyntaxBuilder,
        kind: SyntaxKind::ImplementationTypeMemberBinding,
        source_slot: "implementation_type_member_binding.source",
        node_name: "implementation type member binding",
        range_description: "implementation-type-member-binding",
        debug_name: "ImplementationTypeMemberBindingSyntax",
        builder_debug_name: "ImplementationTypeMemberBindingSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `type` keyword token.
                type_keyword;
                /// Appends the `type` keyword token.
                push_type_keyword;
                kind: SyntaxKind::TypeKeyword;
                slot: "implementation_type_member_binding.type_keyword";
            },
            {
                /// Returns the required type member name token.
                identifier_token;
                /// Appends the type member name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "implementation_type_member_binding.identifier_token";
            },
            {
                /// Returns the required equals token.
                equals_token;
                /// Appends the equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "implementation_type_member_binding.equals_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "implementation_type_member_binding.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Trait type-valued member declaration.
    pub struct TraitTypeMemberDeclarationSyntax {
        builder: TraitTypeMemberDeclarationSyntaxBuilder,
        kind: SyntaxKind::TraitTypeMemberDeclaration,
        source_slot: "trait_type_member_declaration.source",
        node_name: "trait type member declaration",
        range_description: "trait-type-member-declaration",
        debug_name: "TraitTypeMemberDeclarationSyntax",
        builder_debug_name: "TraitTypeMemberDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `type` keyword token.
                type_keyword;
                /// Appends the `type` keyword token.
                push_type_keyword;
                kind: SyntaxKind::TypeKeyword;
                slot: "trait_type_member_declaration.type_keyword";
            },
            {
                /// Returns the required type member name token.
                identifier_token;
                /// Appends the type member name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "trait_type_member_declaration.identifier_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "trait_type_member_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{
        implementation_type_member_binding, keyword, snapshot as test_snapshot, token,
    };
    use crate::{SyntaxKind, SyntaxText};

    #[test]
    fn implementation_type_member_bindings_store_name_equals_type_and_semicolon() {
        let snapshot = test_snapshot(
            "syntax-implementation-type-member-test",
            "type Item = Element;",
        );

        let binding = implementation_type_member_binding(snapshot, 0, false);
        let skipped_syntax = binding.skipped_syntax().collect::<Vec<_>>();

        let [type_value] = skipped_syntax.as_slice() else {
            panic!("expected skipped implementation type member value: {skipped_syntax:?}");
        };

        assert_eq!(binding.full_text(), "type Item = Element;");
        assert_eq!(binding.type_keyword().kind(), SyntaxKind::TypeKeyword);
        assert_eq!(binding.equals_token().kind(), SyntaxKind::EqualsToken);
        assert_eq!(binding.semicolon_token().kind(), SyntaxKind::SemicolonToken);
        assert_eq!(type_value.full_text(), "Element");
    }

    #[test]
    fn trait_type_member_declarations_store_name_and_semicolon() {
        let snapshot = test_snapshot("syntax-trait-type-member-test", "type Item;");
        let mut builder =
            super::TraitTypeMemberDeclarationSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_type_keyword(keyword(SyntaxKind::TypeKeyword, 0, 4, true));
        builder.push_identifier_token(token(SyntaxKind::IdentifierToken, 5, 9));
        builder.push_semicolon_token(token(SyntaxKind::SemicolonToken, 9, 10));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "type Item;");
        assert_eq!(declaration.type_keyword().kind(), SyntaxKind::TypeKeyword);

        assert_eq!(
            declaration.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(
            declaration.semicolon_token().kind(),
            SyntaxKind::SemicolonToken
        );
    }
}
