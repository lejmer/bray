use super::expression::first_expression;
use crate::node::define_source_syntax_node;
use crate::{
    ExpressionSyntax, SyntaxKind, SyntaxToken, TypeExpressionSyntax, TypedIdentifierSyntax,
};

define_source_syntax_node! {
    /// Optional constant declaration modifiers in source order.
    pub struct ConstantModifiersSyntax {
        builder: ConstantModifiersSyntaxBuilder,
        kind: SyntaxKind::ConstantModifiers,
        source_slot: "constant_modifiers.source",
        node_name: "constant modifiers",
        range_description: "constant-modifiers",
        debug_name: "ConstantModifiersSyntax",
        builder_debug_name: "ConstantModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl ConstantModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl ConstantModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "constant_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Ordinary constant declaration.
    pub struct ConstantDeclarationSyntax {
        builder: ConstantDeclarationSyntaxBuilder,
        kind: SyntaxKind::ConstantDeclaration,
        source_slot: "constant_declaration.source",
        node_name: "constant declaration",
        range_description: "constant-declaration",
        debug_name: "ConstantDeclarationSyntax",
        builder_debug_name: "ConstantDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `const` keyword token.
                const_keyword;
                /// Appends the `const` keyword token.
                push_const_keyword;
                kind: SyntaxKind::ConstKeyword;
                slot: "constant_declaration.const_keyword";
            },
            {
                /// Returns the required initializer equals token.
                equals_token;
                /// Appends the initializer equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "constant_declaration.equals_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "constant_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the constant-modifiers child.
                constant_modifiers;
                /// Appends the constant-modifiers child.
                push_constant_modifiers;
                ty: ConstantModifiersSyntax;
                kind: SyntaxKind::ConstantModifiers;
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
                /// Returns value expression children in source order.
                expressions;
                /// Appends a value expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl ConstantDeclarationSyntax {
    /// Returns the required constant name token.
    pub fn identifier_token(&self) -> SyntaxToken {
        self.typed_identifier().identifier_token()
    }

    /// Returns the required colon token.
    pub fn colon_token(&self) -> SyntaxToken {
        self.typed_identifier().colon_token()
    }

    /// Returns the constant type-expression child.
    pub fn type_expression(&self) -> TypeExpressionSyntax {
        self.typed_identifier().type_expression()
    }

    /// Returns the value expression child.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}

define_source_syntax_node! {
    /// Trait constant member declaration.
    pub struct TraitConstantMemberDeclarationSyntax {
        builder: TraitConstantMemberDeclarationSyntaxBuilder,
        kind: SyntaxKind::TraitConstantMemberDeclaration,
        source_slot: "trait_constant_member_declaration.source",
        node_name: "trait constant member declaration",
        range_description: "trait-constant-member-declaration",
        debug_name: "TraitConstantMemberDeclarationSyntax",
        builder_debug_name: "TraitConstantMemberDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `const` keyword token.
                const_keyword;
                /// Appends the `const` keyword token.
                push_const_keyword;
                kind: SyntaxKind::ConstKeyword;
                slot: "trait_constant_member_declaration.const_keyword";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "trait_constant_member_declaration.semicolon_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional default-value equals token.
                equals_token;
                /// Appends the default-value equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "trait_constant_member_declaration.equals_token";
            }
        ],
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
        repeated_children: [
            {
                /// Returns default value expression children in source order.
                expressions;
                /// Appends a default value expression child.
                push_expression;
                ty: ExpressionSyntax;
                kind: SyntaxKind::Expression;
            }
        ],
    }
}

impl TraitConstantMemberDeclarationSyntax {
    /// Returns the required constant name token.
    pub fn identifier_token(&self) -> SyntaxToken {
        self.typed_identifier().identifier_token()
    }

    /// Returns the required colon token.
    pub fn colon_token(&self) -> SyntaxToken {
        self.typed_identifier().colon_token()
    }

    /// Returns the constant type-expression child.
    pub fn type_expression(&self) -> TypeExpressionSyntax {
        self.typed_identifier().type_expression()
    }

    /// Returns the default value expression child when present.
    pub fn expression(&self) -> Option<ExpressionSyntax> {
        first_expression(&self.source, &self.node, self.start)
    }
}
