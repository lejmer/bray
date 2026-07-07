use super::super::callable::{
    CallableBodyBlockExpressionSyntax, CallableResultClauseSyntax, ParameterListSyntax,
};
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Optional type callable member modifiers in source order.
    pub struct TypeCallableMemberModifiersSyntax {
        builder: TypeCallableMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::TypeCallableMemberModifiers,
        source_slot: "type_callable_member_modifiers.source",
        node_name: "type callable member modifiers",
        range_description: "type-callable-member-modifiers",
        debug_name: "TypeCallableMemberModifiersSyntax",
        builder_debug_name: "TypeCallableMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `async` modifier token.
                async_token;
                /// Appends an `async` modifier token.
                push_async_token;
                kind: SyntaxKind::AsyncKeyword;
                slot: "type_callable_member_modifiers.async_token";
            },
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "type_callable_member_modifiers.trusted_token";
            },
            {
                /// Returns the first optional `const` modifier token.
                const_token;
                /// Appends a `const` modifier token.
                push_const_token;
                kind: SyntaxKind::ConstKeyword;
                slot: "type_callable_member_modifiers.const_token";
            },
            {
                /// Returns the first optional `static` modifier token.
                static_token;
                /// Appends a `static` modifier token.
                push_static_token;
                kind: SyntaxKind::StaticKeyword;
                slot: "type_callable_member_modifiers.static_token";
            },
            {
                /// Returns the first optional `consume` modifier token.
                consume_token;
                /// Appends a `consume` modifier token.
                push_consume_token;
                kind: SyntaxKind::ConsumeKeyword;
                slot: "type_callable_member_modifiers.consume_token";
            },
            {
                /// Returns the first optional `mut` modifier token.
                mut_token;
                /// Appends a `mut` modifier token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "type_callable_member_modifiers.mut_token";
            }
        ],
        required_children: [],
    }
}

impl TypeCallableMemberModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl TypeCallableMemberModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "type_callable_member_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Type callable member declaration.
    pub struct TypeCallableMemberDeclarationSyntax {
        builder: TypeCallableMemberDeclarationSyntaxBuilder,
        kind: SyntaxKind::TypeCallableMemberDeclaration,
        source_slot: "type_callable_member_declaration.source",
        node_name: "type callable member declaration",
        range_description: "type-callable-member-declaration",
        debug_name: "TypeCallableMemberDeclarationSyntax",
        builder_debug_name: "TypeCallableMemberDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `func` keyword token.
                func_keyword;
                /// Appends the `func` keyword token.
                push_func_keyword;
                kind: SyntaxKind::FuncKeyword;
                slot: "type_callable_member_declaration.func_keyword";
            },
            {
                /// Returns the required callable member name token.
                identifier_token;
                /// Appends the callable member name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "type_callable_member_declaration.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the member-modifiers child.
                type_callable_member_modifiers;
                /// Appends the member-modifiers child.
                push_type_callable_member_modifiers;
                ty: TypeCallableMemberModifiersSyntax;
                kind: SyntaxKind::TypeCallableMemberModifiers;
            },
            {
                /// Returns the parameter-list child.
                parameter_list;
                /// Appends the parameter-list child.
                push_parameter_list;
                ty: ParameterListSyntax;
                kind: SyntaxKind::ParameterList;
            }
        ],
        repeated_children: [
            {
                /// Returns callable result clauses in source order.
                callable_result_clauses;
                /// Appends a callable result clause child.
                push_callable_result_clause;
                ty: CallableResultClauseSyntax;
                kind: SyntaxKind::CallableResultClause;
            },
            {
                /// Returns callable body block expressions in source order.
                callable_body_block_expressions;
                /// Appends a callable body block expression child.
                push_callable_body_block_expression;
                ty: CallableBodyBlockExpressionSyntax;
                kind: SyntaxKind::CallableBodyBlockExpression;
            }
        ],
    }
}

impl TypeCallableMemberDeclarationSyntax {
    /// Returns the callable result clause child when present.
    pub fn callable_result_clause(&self) -> Option<CallableResultClauseSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableResultClause,
            CallableResultClauseSyntax::from_green,
        )
        .next()
    }

    /// Returns the callable body block expression child when present.
    pub fn callable_body_block_expression(&self) -> Option<CallableBodyBlockExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableBodyBlockExpression,
            CallableBodyBlockExpressionSyntax::from_green,
        )
        .next()
    }
}

define_source_syntax_node! {
    /// Optional trait callable member modifiers in source order.
    pub struct TraitCallableMemberModifiersSyntax {
        builder: TraitCallableMemberModifiersSyntaxBuilder,
        kind: SyntaxKind::TraitCallableMemberModifiers,
        source_slot: "trait_callable_member_modifiers.source",
        node_name: "trait callable member modifiers",
        range_description: "trait-callable-member-modifiers",
        debug_name: "TraitCallableMemberModifiersSyntax",
        builder_debug_name: "TraitCallableMemberModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the first optional `async` modifier token.
                async_token;
                /// Appends an `async` modifier token.
                push_async_token;
                kind: SyntaxKind::AsyncKeyword;
                slot: "trait_callable_member_modifiers.async_token";
            },
            {
                /// Returns the first optional `trusted` modifier token.
                trusted_token;
                /// Appends a `trusted` modifier token.
                push_trusted_token;
                kind: SyntaxKind::TrustedKeyword;
                slot: "trait_callable_member_modifiers.trusted_token";
            },
            {
                /// Returns the first optional `const` modifier token.
                const_token;
                /// Appends a `const` modifier token.
                push_const_token;
                kind: SyntaxKind::ConstKeyword;
                slot: "trait_callable_member_modifiers.const_token";
            },
            {
                /// Returns the first optional `static` modifier token.
                static_token;
                /// Appends a `static` modifier token.
                push_static_token;
                kind: SyntaxKind::StaticKeyword;
                slot: "trait_callable_member_modifiers.static_token";
            },
            {
                /// Returns the first optional `consume` modifier token.
                consume_token;
                /// Appends a `consume` modifier token.
                push_consume_token;
                kind: SyntaxKind::ConsumeKeyword;
                slot: "trait_callable_member_modifiers.consume_token";
            },
            {
                /// Returns the first optional `mut` modifier token.
                mut_token;
                /// Appends a `mut` modifier token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "trait_callable_member_modifiers.mut_token";
            }
        ],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Trait callable member declaration.
    pub struct TraitCallableMemberDeclarationSyntax {
        builder: TraitCallableMemberDeclarationSyntaxBuilder,
        kind: SyntaxKind::TraitCallableMemberDeclaration,
        source_slot: "trait_callable_member_declaration.source",
        node_name: "trait callable member declaration",
        range_description: "trait-callable-member-declaration",
        debug_name: "TraitCallableMemberDeclarationSyntax",
        builder_debug_name: "TraitCallableMemberDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `func` keyword token.
                func_keyword;
                /// Appends the `func` keyword token.
                push_func_keyword;
                kind: SyntaxKind::FuncKeyword;
                slot: "trait_callable_member_declaration.func_keyword";
            },
            {
                /// Returns the required callable member name token.
                identifier_token;
                /// Appends the callable member name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "trait_callable_member_declaration.identifier_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional semicolon tail token.
                semicolon_token;
                /// Appends the semicolon tail token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "trait_callable_member_declaration.semicolon_token";
            }
        ],
        required_children: [
            {
                /// Returns the member-modifiers child.
                trait_callable_member_modifiers;
                /// Appends the member-modifiers child.
                push_trait_callable_member_modifiers;
                ty: TraitCallableMemberModifiersSyntax;
                kind: SyntaxKind::TraitCallableMemberModifiers;
            },
            {
                /// Returns the parameter-list child.
                parameter_list;
                /// Appends the parameter-list child.
                push_parameter_list;
                ty: ParameterListSyntax;
                kind: SyntaxKind::ParameterList;
            }
        ],
        repeated_children: [
            {
                /// Returns callable result clauses in source order.
                callable_result_clauses;
                /// Appends a callable result clause child.
                push_callable_result_clause;
                ty: CallableResultClauseSyntax;
                kind: SyntaxKind::CallableResultClause;
            },
            {
                /// Returns callable body block expressions in source order.
                callable_body_block_expressions;
                /// Appends a callable body block expression child.
                push_callable_body_block_expression;
                ty: CallableBodyBlockExpressionSyntax;
                kind: SyntaxKind::CallableBodyBlockExpression;
            }
        ],
    }
}

impl TraitCallableMemberDeclarationSyntax {
    /// Returns the callable result clause child when present.
    pub fn callable_result_clause(&self) -> Option<CallableResultClauseSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableResultClause,
            CallableResultClauseSyntax::from_green,
        )
        .next()
    }

    /// Returns the callable body block expression child when present.
    pub fn callable_body_block_expression(&self) -> Option<CallableBodyBlockExpressionSyntax> {
        child_nodes(
            &self.source,
            &self.node,
            self.start,
            SyntaxKind::CallableBodyBlockExpression,
            CallableBodyBlockExpressionSyntax::from_green,
        )
        .next()
    }
}
