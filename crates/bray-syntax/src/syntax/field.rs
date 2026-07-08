use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken, TypeExpressionSyntax};

define_source_syntax_node! {
    /// Optional struct field modifiers in source order.
    pub struct FieldModifiersSyntax {
        builder: FieldModifiersSyntaxBuilder,
        kind: SyntaxKind::FieldModifiers,
        source_slot: "field_modifiers.source",
        node_name: "field modifiers",
        range_description: "field-modifiers",
        debug_name: "FieldModifiersSyntax",
        builder_debug_name: "FieldModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `mut` modifier token.
                mut_token;
                /// Appends a `mut` modifier token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "field_modifiers.mut_token";
            }
        ],
        required_children: [],
    }
}

impl FieldModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl FieldModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "field_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Struct field declaration.
    pub struct StructFieldDeclarationSyntax {
        builder: StructFieldDeclarationSyntaxBuilder,
        kind: SyntaxKind::StructFieldDeclaration,
        source_slot: "struct_field_declaration.source",
        node_name: "struct field declaration",
        range_description: "struct-field-declaration",
        debug_name: "StructFieldDeclarationSyntax",
        builder_debug_name: "StructFieldDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required field name token.
                identifier_token;
                /// Appends the field name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "struct_field_declaration.identifier_token";
            },
            {
                /// Returns the required colon token.
                colon_token;
                /// Appends the colon token.
                push_colon_token;
                kind: SyntaxKind::ColonToken;
                slot: "struct_field_declaration.colon_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "struct_field_declaration.semicolon_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional default-value equals token.
                equals_token;
                /// Appends the default-value equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "struct_field_declaration.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the field-modifiers child.
                field_modifiers;
                /// Appends the field-modifiers child.
                push_field_modifiers;
                ty: FieldModifiersSyntax;
                kind: SyntaxKind::FieldModifiers;
            },
            {
                /// Returns the field type-expression child.
                type_expression;
                /// Appends the field type-expression child.
                push_type_expression;
                ty: TypeExpressionSyntax;
                kind: SyntaxKind::TypeExpression;
            }
        ],
    }
}
