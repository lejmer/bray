use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Optional module modifiers in source order.
    pub struct ModuleModifiersSyntax {
        builder: ModuleModifiersSyntaxBuilder,
        kind: SyntaxKind::ModuleModifiers,
        source_slot: "module_modifiers.source",
        node_name: "module modifiers",
        range_description: "module-modifiers",
        debug_name: "ModuleModifiersSyntax",
        builder_debug_name: "ModuleModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional `trusted` modifier token.
                trusted_token;
                /// Appends the optional trusted modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "module_modifiers.trusted_token";
            }
        ],
        required_children: [],
    }
}

impl ModuleModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl ModuleModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "module_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}
