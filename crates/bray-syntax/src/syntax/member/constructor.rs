use super::super::callable::{
    CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
};
use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Optional constructor member modifiers in source order.
    pub struct ConstructorMemberModifiersSyntax {
        builder: ConstructorMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::ConstructorMemberModifiers,
        source_slot: "constructor_member_modifiers.source",
        node_name: "constructor member modifiers",
        range_description: "constructor-member-modifiers",
        debug_name: "ConstructorMemberModifiersSyntax",
        builder_debug_name: "ConstructorMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "constructor_member_modifiers.trusted_token";
            }
        ],
        required_children: [],
    }
}

impl ConstructorMemberModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl ConstructorMemberModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "constructor_member_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Type constructor member declaration.
    pub struct TypeConstructorMemberDeclarationSyntax {
        builder: TypeConstructorMemberDeclarationSyntaxBuilder,
        kind: SyntaxKind::TypeConstructorMemberDeclaration,
        source_slot: "type_constructor_member_declaration.source",
        node_name: "type constructor member declaration",
        range_description: "type-constructor-member-declaration",
        debug_name: "TypeConstructorMemberDeclarationSyntax",
        builder_debug_name: "TypeConstructorMemberDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `construct` keyword token.
                construct_keyword;
                /// Appends the `construct` keyword token.
                push_construct_keyword;
                kind: SyntaxKind::ConstructKeyword;
                slot: "type_constructor_member_declaration.construct_keyword";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional constructor name token.
                identifier_token;
                /// Appends the constructor name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "type_constructor_member_declaration.identifier_token";
            }
        ],
        required_children: [
            {
                /// Returns the constructor modifiers child.
                constructor_member_modifiers;
                /// Appends the constructor modifiers child.
                push_constructor_member_modifiers;
                ty: ConstructorMemberModifiersSyntax;
                kind: SyntaxKind::ConstructorMemberModifiers;
            },
            {
                /// Returns the parameter-list child.
                parameter_list;
                /// Appends the parameter-list child.
                push_parameter_list;
                ty: ParameterListSyntax;
                kind: SyntaxKind::ParameterList;
            },
            {
                /// Returns the required callable result clause.
                callable_result_clause;
                /// Appends the callable result clause child.
                push_callable_result_clause;
                ty: CallableResultClauseSyntax;
                kind: SyntaxKind::CallableResultClause;
            },
            {
                /// Returns the required callable body block expression.
                callable_body_block_expression;
                /// Appends the callable body block expression child.
                push_callable_body_block_expression;
                ty: CallableBodyBlockExpressionSyntax;
                kind: SyntaxKind::CallableBodyBlockExpression;
            }
        ],
    }
}
