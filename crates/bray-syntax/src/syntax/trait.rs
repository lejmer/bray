use super::member::TraitCallableMemberDeclarationSyntax;
use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Optional trait modifiers in source order.
    pub struct TraitModifiersSyntax {
        builder: TraitModifiersSyntaxBuilder,
        kind: SyntaxKind::TraitModifiers,
        source_slot: "trait_modifiers.source",
        node_name: "trait modifiers",
        range_description: "trait-modifiers",
        debug_name: "TraitModifiersSyntax",
        builder_debug_name: "TraitModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl TraitModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl TraitModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "trait_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Braced trait declaration body.
    pub struct TraitBodySyntax {
        builder: TraitBodySyntaxBuilder,
        kind: SyntaxKind::TraitBody,
        source_slot: "trait_body.source",
        node_name: "trait body",
        range_description: "trait-body",
        debug_name: "TraitBodySyntax",
        builder_debug_name: "TraitBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "trait_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "trait_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns trait callable member declarations in source order.
                trait_callable_member_declarations;
                /// Appends a trait callable member declaration.
                push_trait_callable_member_declaration;
                ty: TraitCallableMemberDeclarationSyntax;
                kind: SyntaxKind::TraitCallableMemberDeclaration;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Module-level trait declaration.
    pub struct TraitDeclarationSyntax {
        builder: TraitDeclarationSyntaxBuilder,
        kind: SyntaxKind::TraitDeclaration,
        source_slot: "trait_declaration.source",
        node_name: "trait declaration",
        range_description: "trait-declaration",
        debug_name: "TraitDeclarationSyntax",
        builder_debug_name: "TraitDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `trait` keyword token.
                trait_keyword;
                /// Appends the `trait` keyword token.
                push_trait_keyword;
                kind: SyntaxKind::TraitKeyword;
                slot: "trait_declaration.trait_keyword";
            },
            {
                /// Returns the required trait name token.
                identifier_token;
                /// Appends the trait name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "trait_declaration.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the trait-modifiers child.
                trait_modifiers;
                /// Appends the trait-modifiers child.
                push_trait_modifiers;
                ty: TraitModifiersSyntax;
                kind: SyntaxKind::TraitModifiers;
            },
            {
                /// Returns the trait-body child.
                trait_body;
                /// Appends the trait-body child.
                push_trait_body;
                ty: TraitBodySyntax;
                kind: SyntaxKind::TraitBody;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextSize};

    use crate::test_support::{keyword, snapshot as test_snapshot, token};
    use crate::{
        SyntaxKind, SyntaxText, TraitBodySyntax, TraitDeclarationSyntax, TraitModifiersSyntax,
    };

    #[test]
    fn trait_declarations_store_modifiers_name_and_body() {
        let snapshot = test_snapshot("syntax-trait-declaration-test", "public trait Display {}");
        let mut builder = TraitDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_trait_modifiers(trait_modifiers(snapshot.clone()));
        builder.push_trait_keyword(keyword(SyntaxKind::TraitKeyword, 7, 12, true));
        builder.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 13, 20, true));
        builder.push_trait_body(trait_body(snapshot));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "public trait Display {}");

        assert_eq!(
            declaration
                .trait_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(declaration.trait_keyword().kind(), SyntaxKind::TraitKeyword);

        assert_eq!(
            declaration.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(declaration.trait_body().full_text(), "{}");
    }

    fn trait_modifiers(snapshot: SourceSnapshot) -> TraitModifiersSyntax {
        let mut builder = TraitModifiersSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_visibility_token(keyword(SyntaxKind::PublicKeyword, 0, 6, true));

        builder.build()
    }

    fn trait_body(snapshot: SourceSnapshot) -> TraitBodySyntax {
        let mut builder = TraitBodySyntax::builder(snapshot, TextSize::new(21));

        builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, 21, 22));
        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 22, 23));

        builder.build()
    }
}
