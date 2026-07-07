use super::directive::TagDirectiveSyntax;
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Union variant directives in source order.
    pub struct VariantDirectivesSyntax {
        builder: VariantDirectivesSyntaxBuilder,
        kind: SyntaxKind::VariantDirectives,
        source_slot: "variant_directives.source",
        node_name: "variant directives",
        range_description: "variant-directives",
        debug_name: "VariantDirectivesSyntax",
        builder_debug_name: "VariantDirectivesSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns `@tag(...)` directives in source order.
                tag_directives;
                /// Appends a `@tag(...)` directive.
                push_tag_directive;
                ty: TagDirectiveSyntax;
                kind: SyntaxKind::TagDirective;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Union variant declaration.
    pub struct UnionVariantDeclarationSyntax {
        builder: UnionVariantDeclarationSyntaxBuilder,
        kind: SyntaxKind::UnionVariantDeclaration,
        source_slot: "union_variant_declaration.source",
        node_name: "union variant declaration",
        range_description: "union-variant-declaration",
        debug_name: "UnionVariantDeclarationSyntax",
        builder_debug_name: "UnionVariantDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required variant name token.
                identifier_token;
                /// Appends the variant name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "union_variant_declaration.identifier_token";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "union_variant_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the variant-directives child.
                variant_directives;
                /// Appends the variant-directives child.
                push_variant_directives;
                ty: VariantDirectivesSyntax;
                kind: SyntaxKind::VariantDirectives;
            }
        ],
        repeated_children: [
            {
                /// Returns payload children in source order.
                union_variant_payloads;
                /// Appends a payload child.
                push_union_variant_payload;
                ty: UnionVariantPayloadSyntax;
                kind: SyntaxKind::UnionVariantPayload;
            }
        ],
    }
}

impl UnionVariantDeclarationSyntax {
    /// Returns the union variant payload child when present.
    pub fn union_variant_payload(&self) -> Option<UnionVariantPayloadSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::UnionVariantPayload,
            UnionVariantPayloadSyntax::from_green,
        )
        .next()
    }
}

define_source_syntax_node! {
    /// Parenthesized union variant payload.
    pub struct UnionVariantPayloadSyntax {
        builder: UnionVariantPayloadSyntaxBuilder,
        kind: SyntaxKind::UnionVariantPayload,
        source_slot: "union_variant_payload.source",
        node_name: "union variant payload",
        range_description: "union-variant-payload",
        debug_name: "UnionVariantPayloadSyntax",
        builder_debug_name: "UnionVariantPayloadSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening parenthesis token.
                open_paren_token;
                /// Appends the opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "union_variant_payload.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token.
                close_paren_token;
                /// Appends the closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "union_variant_payload.close_paren_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "union_variant_payload.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns payload field children in source order.
                union_payload_fields;
                /// Appends a payload field child in source order.
                push_union_payload_field;
                ty: UnionPayloadFieldSyntax;
                kind: SyntaxKind::UnionPayloadField;
            }
        ],
    }
}

impl UnionVariantPayloadSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Optional union payload field modifiers in source order.
    pub struct PayloadFieldModifiersSyntax {
        builder: PayloadFieldModifiersSyntaxBuilder,
        kind: SyntaxKind::PayloadFieldModifiers,
        source_slot: "payload_field_modifiers.source",
        node_name: "payload field modifiers",
        range_description: "payload-field-modifiers",
        debug_name: "PayloadFieldModifiersSyntax",
        builder_debug_name: "PayloadFieldModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `pos` modifier token.
                pos_token;
                /// Appends a `pos` modifier token.
                push_pos_token;
                kind: SyntaxKind::PosKeyword;
                slot: "payload_field_modifiers.pos_token";
            },
            {
                /// Returns the first optional `mut` modifier token.
                mut_token;
                /// Appends a `mut` modifier token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "payload_field_modifiers.mut_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Union variant payload field.
    pub struct UnionPayloadFieldSyntax {
        builder: UnionPayloadFieldSyntaxBuilder,
        kind: SyntaxKind::UnionPayloadField,
        source_slot: "union_payload_field.source",
        node_name: "union payload field",
        range_description: "union-payload-field",
        debug_name: "UnionPayloadFieldSyntax",
        builder_debug_name: "UnionPayloadFieldSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required payload field name token.
                identifier_token;
                /// Appends the payload field name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "union_payload_field.identifier_token";
            },
            {
                /// Returns the required colon token.
                colon_token;
                /// Appends the colon token.
                push_colon_token;
                kind: SyntaxKind::ColonToken;
                slot: "union_payload_field.colon_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional default-value equals token.
                equals_token;
                /// Appends the default-value equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "union_payload_field.equals_token";
            }
        ],
        required_children: [
            {
                /// Returns the payload-field-modifiers child.
                payload_field_modifiers;
                /// Appends the payload-field-modifiers child.
                push_payload_field_modifiers;
                ty: PayloadFieldModifiersSyntax;
                kind: SyntaxKind::PayloadFieldModifiers;
            }
        ],
    }
}
