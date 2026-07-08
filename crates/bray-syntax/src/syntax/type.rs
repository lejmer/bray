use super::directive::{CopyDirectiveSyntax, LayoutDirectiveSyntax};
use super::field::StructFieldDeclarationSyntax;
use super::member::{
    DestructorMemberDeclarationSyntax, FinalizerMemberDeclarationSyntax,
    ScopeEnterMemberDeclarationSyntax, ScopeExitMemberDeclarationSyntax,
    TypeCallableMemberDeclarationSyntax, TypeConstructorMemberDeclarationSyntax,
};
use super::variant::UnionVariantDeclarationSyntax;
use crate::node::{child_nodes, define_source_syntax_node};
use crate::{
    CallableOverloadDeclarationSyntax, ConstantDeclarationSyntax, GenericParameterListSyntax,
    PredicateDeclarationSyntax, SyntaxKind, SyntaxToken,
};

define_source_syntax_node! {
    /// Type directives in source order.
    pub struct TypeDirectivesSyntax {
        builder: TypeDirectivesSyntaxBuilder,
        kind: SyntaxKind::TypeDirectives,
        source_slot: "type_directives.source",
        node_name: "type directives",
        range_description: "type-directives",
        debug_name: "TypeDirectivesSyntax",
        builder_debug_name: "TypeDirectivesSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns `@layout(...)` directives in source order.
                layout_directives;
                /// Appends a `@layout(...)` directive.
                push_layout_directive;
                ty: LayoutDirectiveSyntax;
                kind: SyntaxKind::LayoutDirective;
            },
            {
                /// Returns `@copy` directives in source order.
                copy_directives;
                /// Appends a `@copy` directive.
                push_copy_directive;
                ty: CopyDirectiveSyntax;
                kind: SyntaxKind::CopyDirective;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextSize};

    use crate::test_support::{directive_argument, keyword, snapshot as test_snapshot, token};
    use crate::{
        CopyDirectiveSyntax, DirectiveArgumentListSyntax, LayoutDirectiveSyntax, StructBodySyntax,
        StructDeclarationSyntax, SyntaxKind, SyntaxText, TypeDirectivesSyntax, TypeModifiersSyntax,
        UnionBodySyntax, UnionDeclarationSyntax,
    };

    #[test]
    fn struct_declarations_store_directives_modifiers_name_and_body() {
        let snapshot = test_snapshot(
            "syntax-type-declaration-test",
            "@copy public struct Point {}",
        );
        let mut builder = StructDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_type_directives(copy_type_directives(snapshot.clone()));
        builder.push_type_modifiers(public_type_modifiers(snapshot.clone(), 6));
        builder.push_struct_keyword(keyword(SyntaxKind::StructKeyword, 13, 19, true));
        builder.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 20, 25, true));
        builder.push_struct_body(struct_body(snapshot, 26));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "@copy public struct Point {}");
        assert_eq!(declaration.type_directives().copy_directives().count(), 1);

        assert_eq!(
            declaration
                .type_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            declaration.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(declaration.struct_body().full_text(), "{}");
    }

    #[test]
    fn union_declarations_store_layout_directives_name_and_body() {
        let snapshot = test_snapshot("syntax-type-declaration-test", "@layout(c) union Maybe {}");
        let mut builder = UnionDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_type_directives(layout_type_directives(snapshot.clone()));

        builder.push_type_modifiers(
            TypeModifiersSyntax::builder(snapshot.clone(), TextSize::new(11)).build(),
        );

        builder.push_union_keyword(keyword(SyntaxKind::UnionKeyword, 11, 16, true));
        builder.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 17, 22, true));

        builder.push_union_body(union_body(snapshot, 23));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "@layout(c) union Maybe {}");
        assert_eq!(declaration.type_directives().layout_directives().count(), 1);
        assert_eq!(declaration.type_directives().skipped_syntax().count(), 0);
        assert_eq!(declaration.union_body().full_text(), "{}");
    }

    fn copy_type_directives(snapshot: SourceSnapshot) -> TypeDirectivesSyntax {
        let mut builder = TypeDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_copy_directive(copy_directive(snapshot));

        builder.build()
    }

    fn layout_type_directives(snapshot: SourceSnapshot) -> TypeDirectivesSyntax {
        let mut builder = TypeDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_layout_directive(layout_directive(snapshot));

        builder.build()
    }

    fn copy_directive(snapshot: SourceSnapshot) -> CopyDirectiveSyntax {
        let mut builder = CopyDirectiveSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_directive_marker_token(token(SyntaxKind::AtToken, 0, 1));
        builder.push_name_token(keyword(SyntaxKind::IdentifierToken, 1, 5, true));

        builder.build()
    }

    fn layout_directive(snapshot: SourceSnapshot) -> LayoutDirectiveSyntax {
        let mut builder = LayoutDirectiveSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_directive_marker_token(token(SyntaxKind::AtToken, 0, 1));
        builder.push_name_token(token(SyntaxKind::IdentifierToken, 1, 7));
        builder.push_directive_argument_list(layout_argument_list(snapshot));

        builder.build()
    }

    fn layout_argument_list(snapshot: SourceSnapshot) -> DirectiveArgumentListSyntax {
        let mut builder = DirectiveArgumentListSyntax::builder(snapshot.clone(), TextSize::new(7));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 7, 8));

        builder.push_directive_argument(directive_argument(
            snapshot,
            SyntaxKind::IdentifierToken,
            8,
            9,
        ));

        builder.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 9, 10, true));

        builder.build()
    }

    fn public_type_modifiers(snapshot: SourceSnapshot, start: u32) -> TypeModifiersSyntax {
        let mut builder = TypeModifiersSyntax::builder(snapshot, TextSize::new(start));

        builder.push_visibility_token(keyword(SyntaxKind::PublicKeyword, start, start + 6, true));

        builder.build()
    }

    fn struct_body(snapshot: SourceSnapshot, start: u32) -> StructBodySyntax {
        let mut builder = StructBodySyntax::builder(snapshot, TextSize::new(start));

        builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, start + 1, start + 2));

        builder.build()
    }

    fn union_body(snapshot: SourceSnapshot, start: u32) -> UnionBodySyntax {
        let mut builder = UnionBodySyntax::builder(snapshot, TextSize::new(start));

        builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, start + 1, start + 2));

        builder.build()
    }
}

define_source_syntax_node! {
    /// Optional type declaration modifiers in source order.
    pub struct TypeModifiersSyntax {
        builder: TypeModifiersSyntaxBuilder,
        kind: SyntaxKind::TypeModifiers,
        source_slot: "type_modifiers.source",
        node_name: "type modifiers",
        range_description: "type-modifiers",
        debug_name: "TypeModifiersSyntax",
        builder_debug_name: "TypeModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl TypeModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl TypeModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "type_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Braced struct declaration body.
    pub struct StructBodySyntax {
        builder: StructBodySyntaxBuilder,
        kind: SyntaxKind::StructBody,
        source_slot: "struct_body.source",
        node_name: "struct body",
        range_description: "struct-body",
        debug_name: "StructBodySyntax",
        builder_debug_name: "StructBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "struct_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "struct_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns struct field declarations in source order.
                struct_field_declarations;
                /// Appends a struct field declaration.
                push_struct_field_declaration;
                ty: StructFieldDeclarationSyntax;
                kind: SyntaxKind::StructFieldDeclaration;
            },
            {
                /// Returns constant declarations in source order.
                constant_declarations;
                /// Appends a constant declaration.
                push_constant_declaration;
                ty: ConstantDeclarationSyntax;
                kind: SyntaxKind::ConstantDeclaration;
            },
            {
                /// Returns predicate declarations in source order.
                predicate_declarations;
                /// Appends a predicate declaration.
                push_predicate_declaration;
                ty: PredicateDeclarationSyntax;
                kind: SyntaxKind::PredicateDeclaration;
            },
            {
                /// Returns callable overload declarations in source order.
                callable_overload_declarations;
                /// Appends a callable overload declaration.
                push_callable_overload_declaration;
                ty: CallableOverloadDeclarationSyntax;
                kind: SyntaxKind::CallableOverloadDeclaration;
            },
            {
                /// Returns type constructor member declarations in source order.
                type_constructor_member_declarations;
                /// Appends a type constructor member declaration.
                push_type_constructor_member_declaration;
                ty: TypeConstructorMemberDeclarationSyntax;
                kind: SyntaxKind::TypeConstructorMemberDeclaration;
            },
            {
                /// Returns finalizer member declarations in source order.
                finalizer_member_declarations;
                /// Appends a finalizer member declaration.
                push_finalizer_member_declaration;
                ty: FinalizerMemberDeclarationSyntax;
                kind: SyntaxKind::FinalizerMemberDeclaration;
            },
            {
                /// Returns destructor member declarations in source order.
                destructor_member_declarations;
                /// Appends a destructor member declaration.
                push_destructor_member_declaration;
                ty: DestructorMemberDeclarationSyntax;
                kind: SyntaxKind::DestructorMemberDeclaration;
            },
            {
                /// Returns scope-enter member declarations in source order.
                scope_enter_member_declarations;
                /// Appends a scope-enter member declaration.
                push_scope_enter_member_declaration;
                ty: ScopeEnterMemberDeclarationSyntax;
                kind: SyntaxKind::ScopeEnterMemberDeclaration;
            },
            {
                /// Returns scope-exit member declarations in source order.
                scope_exit_member_declarations;
                /// Appends a scope-exit member declaration.
                push_scope_exit_member_declaration;
                ty: ScopeExitMemberDeclarationSyntax;
                kind: SyntaxKind::ScopeExitMemberDeclaration;
            },
            {
                /// Returns type callable member declarations in source order.
                type_callable_member_declarations;
                /// Appends a type callable member declaration.
                push_type_callable_member_declaration;
                ty: TypeCallableMemberDeclarationSyntax;
                kind: SyntaxKind::TypeCallableMemberDeclaration;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Braced union declaration body.
    pub struct UnionBodySyntax {
        builder: UnionBodySyntaxBuilder,
        kind: SyntaxKind::UnionBody,
        source_slot: "union_body.source",
        node_name: "union body",
        range_description: "union-body",
        debug_name: "UnionBodySyntax",
        builder_debug_name: "UnionBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "union_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "union_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
            {
                /// Returns union variant declarations in source order.
                union_variant_declarations;
                /// Appends a union variant declaration.
                push_union_variant_declaration;
                ty: UnionVariantDeclarationSyntax;
                kind: SyntaxKind::UnionVariantDeclaration;
            },
            {
                /// Returns constant declarations in source order.
                constant_declarations;
                /// Appends a constant declaration.
                push_constant_declaration;
                ty: ConstantDeclarationSyntax;
                kind: SyntaxKind::ConstantDeclaration;
            },
            {
                /// Returns predicate declarations in source order.
                predicate_declarations;
                /// Appends a predicate declaration.
                push_predicate_declaration;
                ty: PredicateDeclarationSyntax;
                kind: SyntaxKind::PredicateDeclaration;
            },
            {
                /// Returns callable overload declarations in source order.
                callable_overload_declarations;
                /// Appends a callable overload declaration.
                push_callable_overload_declaration;
                ty: CallableOverloadDeclarationSyntax;
                kind: SyntaxKind::CallableOverloadDeclaration;
            },
            {
                /// Returns type constructor member declarations in source order.
                type_constructor_member_declarations;
                /// Appends a type constructor member declaration.
                push_type_constructor_member_declaration;
                ty: TypeConstructorMemberDeclarationSyntax;
                kind: SyntaxKind::TypeConstructorMemberDeclaration;
            },
            {
                /// Returns finalizer member declarations in source order.
                finalizer_member_declarations;
                /// Appends a finalizer member declaration.
                push_finalizer_member_declaration;
                ty: FinalizerMemberDeclarationSyntax;
                kind: SyntaxKind::FinalizerMemberDeclaration;
            },
            {
                /// Returns destructor member declarations in source order.
                destructor_member_declarations;
                /// Appends a destructor member declaration.
                push_destructor_member_declaration;
                ty: DestructorMemberDeclarationSyntax;
                kind: SyntaxKind::DestructorMemberDeclaration;
            },
            {
                /// Returns scope-enter member declarations in source order.
                scope_enter_member_declarations;
                /// Appends a scope-enter member declaration.
                push_scope_enter_member_declaration;
                ty: ScopeEnterMemberDeclarationSyntax;
                kind: SyntaxKind::ScopeEnterMemberDeclaration;
            },
            {
                /// Returns scope-exit member declarations in source order.
                scope_exit_member_declarations;
                /// Appends a scope-exit member declaration.
                push_scope_exit_member_declaration;
                ty: ScopeExitMemberDeclarationSyntax;
                kind: SyntaxKind::ScopeExitMemberDeclaration;
            },
            {
                /// Returns type callable member declarations in source order.
                type_callable_member_declarations;
                /// Appends a type callable member declaration.
                push_type_callable_member_declaration;
                ty: TypeCallableMemberDeclarationSyntax;
                kind: SyntaxKind::TypeCallableMemberDeclaration;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Module-level struct declaration.
    pub struct StructDeclarationSyntax {
        builder: StructDeclarationSyntaxBuilder,
        kind: SyntaxKind::StructDeclaration,
        source_slot: "struct_declaration.source",
        node_name: "struct declaration",
        range_description: "struct-declaration",
        debug_name: "StructDeclarationSyntax",
        builder_debug_name: "StructDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `struct` keyword token.
                struct_keyword;
                /// Appends the `struct` keyword token.
                push_struct_keyword;
                kind: SyntaxKind::StructKeyword;
                slot: "struct_declaration.struct_keyword";
            },
            {
                /// Returns the required struct name token.
                identifier_token;
                /// Appends the struct name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "struct_declaration.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the type-directives child.
                type_directives;
                /// Appends the type-directives child.
                push_type_directives;
                ty: TypeDirectivesSyntax;
                kind: SyntaxKind::TypeDirectives;
            },
            {
                /// Returns the type-modifiers child.
                type_modifiers;
                /// Appends the type-modifiers child.
                push_type_modifiers;
                ty: TypeModifiersSyntax;
                kind: SyntaxKind::TypeModifiers;
            },
            {
                /// Returns the struct-body child.
                struct_body;
                /// Appends the struct-body child.
                push_struct_body;
                ty: StructBodySyntax;
                kind: SyntaxKind::StructBody;
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

impl StructDeclarationSyntax {
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

define_source_syntax_node! {
    /// Module-level union declaration.
    pub struct UnionDeclarationSyntax {
        builder: UnionDeclarationSyntaxBuilder,
        kind: SyntaxKind::UnionDeclaration,
        source_slot: "union_declaration.source",
        node_name: "union declaration",
        range_description: "union-declaration",
        debug_name: "UnionDeclarationSyntax",
        builder_debug_name: "UnionDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `union` keyword token.
                union_keyword;
                /// Appends the `union` keyword token.
                push_union_keyword;
                kind: SyntaxKind::UnionKeyword;
                slot: "union_declaration.union_keyword";
            },
            {
                /// Returns the required union name token.
                identifier_token;
                /// Appends the union name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "union_declaration.identifier_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the type-directives child.
                type_directives;
                /// Appends the type-directives child.
                push_type_directives;
                ty: TypeDirectivesSyntax;
                kind: SyntaxKind::TypeDirectives;
            },
            {
                /// Returns the type-modifiers child.
                type_modifiers;
                /// Appends the type-modifiers child.
                push_type_modifiers;
                ty: TypeModifiersSyntax;
                kind: SyntaxKind::TypeModifiers;
            },
            {
                /// Returns the union-body child.
                union_body;
                /// Appends the union-body child.
                push_union_body;
                ty: UnionBodySyntax;
                kind: SyntaxKind::UnionBody;
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

impl UnionDeclarationSyntax {
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
