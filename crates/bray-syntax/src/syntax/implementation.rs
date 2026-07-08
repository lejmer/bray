use super::member::{
    DestructorMemberDeclarationSyntax, FinalizerMemberDeclarationSyntax,
    ImplementationTypeMemberBindingSyntax, ScopeEnterMemberDeclarationSyntax,
    ScopeExitMemberDeclarationSyntax, TypeCallableMemberDeclarationSyntax,
    TypeConstructorMemberDeclarationSyntax,
};
use super::path::PathSyntax;
use crate::node::define_source_syntax_node;
use crate::{
    CallableOverloadDeclarationSyntax, ConstantDeclarationSyntax, GenericArgumentListSyntax,
    PredicateDeclarationSyntax, SyntaxKind,
};

define_source_syntax_node! {
    /// Type subject named by an implementation declaration.
    pub struct ImplementationSubjectSyntax {
        builder: ImplementationSubjectSyntaxBuilder,
        kind: SyntaxKind::ImplementationSubject,
        source_slot: "implementation_subject.source",
        node_name: "implementation subject",
        range_description: "implementation-subject",
        debug_name: "ImplementationSubjectSyntax",
        builder_debug_name: "ImplementationSubjectSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional borrowed-subject ampersand token.
                ampersand_token;
                /// Appends a borrowed-subject ampersand token.
                push_ampersand_token;
                kind: SyntaxKind::AmpersandToken;
                slot: "implementation_subject.ampersand_token";
            },
            {
                /// Returns the optional borrowed-subject `mut` token.
                mut_token;
                /// Appends a borrowed-subject `mut` token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "implementation_subject.mut_token";
            }
        ],
        required_children: [
            {
                /// Returns the subject path child.
                path;
                /// Appends the subject path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
        repeated_children: [
            {
                /// Returns generic argument lists in source order.
                generic_argument_lists;
                /// Appends a generic argument list child.
                push_generic_argument_list;
                ty: GenericArgumentListSyntax;
                kind: SyntaxKind::GenericArgumentList;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Trait application named by a trait implementation declaration.
    pub struct TraitApplicationSyntax {
        builder: TraitApplicationSyntaxBuilder,
        kind: SyntaxKind::TraitApplication,
        source_slot: "trait_application.source",
        node_name: "trait application",
        range_description: "trait-application",
        debug_name: "TraitApplicationSyntax",
        builder_debug_name: "TraitApplicationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the trait path child.
                path;
                /// Appends the trait path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
        repeated_children: [
            {
                /// Returns generic argument lists in source order.
                generic_argument_lists;
                /// Appends a generic argument list child.
                push_generic_argument_list;
                ty: GenericArgumentListSyntax;
                kind: SyntaxKind::GenericArgumentList;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Braced implementation body.
    pub struct ImplementationBodySyntax {
        builder: ImplementationBodySyntaxBuilder,
        kind: SyntaxKind::ImplementationBody,
        source_slot: "implementation_body.source",
        node_name: "implementation body",
        range_description: "implementation-body",
        debug_name: "ImplementationBodySyntax",
        builder_debug_name: "ImplementationBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "implementation_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "implementation_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
        repeated_children: [
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
                /// Returns implementation type member bindings in source order.
                implementation_type_member_bindings;
                /// Appends an implementation type member binding.
                push_implementation_type_member_binding;
                ty: ImplementationTypeMemberBindingSyntax;
                kind: SyntaxKind::ImplementationTypeMemberBinding;
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
    /// Module-level inherent implementation declaration.
    pub struct InherentImplementationDeclarationSyntax {
        builder: InherentImplementationDeclarationSyntaxBuilder,
        kind: SyntaxKind::InherentImplementationDeclaration,
        source_slot: "inherent_implementation_declaration.source",
        node_name: "inherent implementation declaration",
        range_description: "inherent-implementation-declaration",
        debug_name: "InherentImplementationDeclarationSyntax",
        builder_debug_name: "InherentImplementationDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `impl` keyword token.
                impl_keyword;
                /// Appends the `impl` keyword token.
                push_impl_keyword;
                kind: SyntaxKind::ImplKeyword;
                slot: "inherent_implementation_declaration.impl_keyword";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the implementation-subject child.
                implementation_subject;
                /// Appends the implementation-subject child.
                push_implementation_subject;
                ty: ImplementationSubjectSyntax;
                kind: SyntaxKind::ImplementationSubject;
            },
            {
                /// Returns the implementation-body child.
                implementation_body;
                /// Appends the implementation-body child.
                push_implementation_body;
                ty: ImplementationBodySyntax;
                kind: SyntaxKind::ImplementationBody;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Module-level unnamed trait implementation declaration.
    pub struct UnnamedTraitImplementationDeclarationSyntax {
        builder: UnnamedTraitImplementationDeclarationSyntaxBuilder,
        kind: SyntaxKind::UnnamedTraitImplementationDeclaration,
        source_slot: "unnamed_trait_implementation_declaration.source",
        node_name: "unnamed trait implementation declaration",
        range_description: "unnamed-trait-implementation-declaration",
        debug_name: "UnnamedTraitImplementationDeclarationSyntax",
        builder_debug_name: "UnnamedTraitImplementationDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `impl` keyword token.
                impl_keyword;
                /// Appends the `impl` keyword token.
                push_impl_keyword;
                kind: SyntaxKind::ImplKeyword;
                slot: "unnamed_trait_implementation_declaration.impl_keyword";
            },
            {
                /// Returns the required opening trait-application parenthesis token.
                open_paren_token;
                /// Appends the opening trait-application parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "unnamed_trait_implementation_declaration.open_paren_token";
            },
            {
                /// Returns the required closing trait-application parenthesis token.
                close_paren_token;
                /// Appends the closing trait-application parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "unnamed_trait_implementation_declaration.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the implementation-subject child.
                implementation_subject;
                /// Appends the implementation-subject child.
                push_implementation_subject;
                ty: ImplementationSubjectSyntax;
                kind: SyntaxKind::ImplementationSubject;
            },
            {
                /// Returns the trait-application child.
                trait_application;
                /// Appends the trait-application child.
                push_trait_application;
                ty: TraitApplicationSyntax;
                kind: SyntaxKind::TraitApplication;
            },
            {
                /// Returns the implementation-body child.
                implementation_body;
                /// Appends the implementation-body child.
                push_implementation_body;
                ty: ImplementationBodySyntax;
                kind: SyntaxKind::ImplementationBody;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Module-level named trait implementation declaration.
    pub struct NamedTraitImplementationDeclarationSyntax {
        builder: NamedTraitImplementationDeclarationSyntaxBuilder,
        kind: SyntaxKind::NamedTraitImplementationDeclaration,
        source_slot: "named_trait_implementation_declaration.source",
        node_name: "named trait implementation declaration",
        range_description: "named-trait-implementation-declaration",
        debug_name: "NamedTraitImplementationDeclarationSyntax",
        builder_debug_name: "NamedTraitImplementationDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `impl` keyword token.
                impl_keyword;
                /// Appends the `impl` keyword token.
                push_impl_keyword;
                kind: SyntaxKind::ImplKeyword;
                slot: "named_trait_implementation_declaration.impl_keyword";
            },
            {
                /// Returns the required implementation name token.
                identifier_token;
                /// Appends the implementation name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "named_trait_implementation_declaration.identifier_token";
            },
            {
                /// Returns the required equals token.
                equals_token;
                /// Appends the equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "named_trait_implementation_declaration.equals_token";
            },
            {
                /// Returns the required opening trait-application parenthesis token.
                open_paren_token;
                /// Appends the opening trait-application parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "named_trait_implementation_declaration.open_paren_token";
            },
            {
                /// Returns the required closing trait-application parenthesis token.
                close_paren_token;
                /// Appends the closing trait-application parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "named_trait_implementation_declaration.close_paren_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the implementation-subject child.
                implementation_subject;
                /// Appends the implementation-subject child.
                push_implementation_subject;
                ty: ImplementationSubjectSyntax;
                kind: SyntaxKind::ImplementationSubject;
            },
            {
                /// Returns the trait-application child.
                trait_application;
                /// Appends the trait-application child.
                push_trait_application;
                ty: TraitApplicationSyntax;
                kind: SyntaxKind::TraitApplication;
            },
            {
                /// Returns the implementation-body child.
                implementation_body;
                /// Appends the implementation-body child.
                push_implementation_body;
                ty: ImplementationBodySyntax;
                kind: SyntaxKind::ImplementationBody;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{
        identifier_path, identifier_type_expression, implementation_body, implementation_subject,
        implementation_type_member_binding, keyword, snapshot as test_snapshot, token,
        trait_application,
    };
    use crate::{
        GenericArgumentListSyntax, GenericArgumentSyntax, ImplementationBodySyntax,
        ImplementationSubjectSyntax, InherentImplementationDeclarationSyntax,
        NamedTraitImplementationDeclarationSyntax, SyntaxKind, SyntaxText,
        UnnamedTraitImplementationDeclarationSyntax,
    };

    #[test]
    fn inherent_implementation_declarations_store_subject_and_body() {
        let snapshot = test_snapshot("syntax-implementation-test", "impl Point {}");
        let mut builder =
            InherentImplementationDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_impl_keyword(keyword(SyntaxKind::ImplKeyword, 0, 4, true));
        builder.push_implementation_subject(implementation_subject(snapshot.clone(), 5, 10, true));

        builder.push_implementation_body(implementation_body(snapshot, 11, false));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "impl Point {}");
        assert_eq!(
            declaration.implementation_subject().path().full_text(),
            "Point "
        );
        assert_eq!(declaration.implementation_body().full_text(), "{}");
    }

    #[test]
    fn unnamed_trait_implementation_declarations_store_subject_trait_and_body() {
        let snapshot = test_snapshot("syntax-implementation-test", "impl Point(Equatable) {}");
        let mut builder =
            UnnamedTraitImplementationDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_impl_keyword(keyword(SyntaxKind::ImplKeyword, 0, 4, true));
        builder.push_implementation_subject(implementation_subject(snapshot.clone(), 5, 10, false));
        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 10, 11));
        builder.push_trait_application(trait_application(snapshot.clone(), 11, 20));
        builder.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 20, 21, true));
        builder.push_implementation_body(implementation_body(snapshot, 22, false));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "impl Point(Equatable) {}");
        assert_eq!(declaration.implementation_subject().full_text(), "Point");
        assert_eq!(declaration.trait_application().full_text(), "Equatable");
        assert_eq!(declaration.implementation_body().full_text(), "{}");
    }

    #[test]
    fn named_trait_implementation_declarations_store_name_subject_trait_and_body() {
        let snapshot = test_snapshot(
            "syntax-implementation-test",
            "impl PointEq = Point(Equatable) {}",
        );
        let mut builder =
            NamedTraitImplementationDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_impl_keyword(keyword(SyntaxKind::ImplKeyword, 0, 4, true));
        builder.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 5, 12, true));
        builder.push_equals_token(keyword(SyntaxKind::EqualsToken, 13, 14, true));

        builder.push_implementation_subject(implementation_subject(
            snapshot.clone(),
            15,
            20,
            false,
        ));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 20, 21));
        builder.push_trait_application(trait_application(snapshot.clone(), 21, 30));
        builder.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 30, 31, true));

        builder.push_implementation_body(implementation_body(snapshot, 32, false));

        let declaration = builder.build();

        assert_eq!(
            declaration.full_text(),
            "impl PointEq = Point(Equatable) {}"
        );

        assert_eq!(
            declaration.identifier_token().kind(),
            SyntaxKind::IdentifierToken
        );

        assert_eq!(declaration.implementation_subject().full_text(), "Point");
        assert_eq!(declaration.trait_application().full_text(), "Equatable");
    }

    #[test]
    fn implementation_bodies_store_type_member_bindings() {
        let snapshot = test_snapshot("syntax-implementation-test", "{ type Item = Element; }");
        let mut builder = ImplementationBodySyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_open_brace_token(keyword(SyntaxKind::OpenBraceToken, 0, 1, true));

        builder.push_implementation_type_member_binding(implementation_type_member_binding(
            snapshot, 2, true,
        ));

        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, 23, 24));

        let body = builder.build();

        let bindings = body
            .implementation_type_member_bindings()
            .collect::<Vec<_>>();

        let [binding] = bindings.as_slice() else {
            panic!("expected one implementation type member binding: {bindings:?}");
        };

        assert_eq!(body.full_text(), "{ type Item = Element; }");
        assert_eq!(binding.full_text(), "type Item = Element; ");
    }

    #[test]
    fn implementation_subjects_store_borrow_prefix_and_generic_arguments() {
        let snapshot = test_snapshot("syntax-implementation-test", "&mut Vec<T>");
        let mut builder = ImplementationSubjectSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_ampersand_token(token(SyntaxKind::AmpersandToken, 0, 1));
        builder.push_mut_token(keyword(SyntaxKind::MutKeyword, 1, 4, true));
        builder.push_path(identifier_path(snapshot.clone(), 5, 8));
        builder.push_generic_argument_list(generic_argument_list(snapshot));

        let subject = builder.build();

        assert_eq!(subject.full_text(), "&mut Vec<T>");
        assert_eq!(subject.path().full_text(), "Vec");
        assert_eq!(subject.generic_argument_lists().count(), 1);
        assert_eq!(subject.skipped_syntax().count(), 0);
    }

    fn generic_argument_list(snapshot: bray_source::SourceSnapshot) -> GenericArgumentListSyntax {
        let mut argument = GenericArgumentSyntax::builder(snapshot.clone(), TextSize::new(9));

        argument.push_type_expression(identifier_type_expression(snapshot.clone(), 9, 10));

        let mut list = GenericArgumentListSyntax::builder(snapshot, TextSize::new(8));

        list.push_less_token(token(SyntaxKind::LessToken, 8, 9));
        list.push_generic_argument(argument.build());
        list.push_greater_token(token(SyntaxKind::GreaterToken, 10, 11));

        list.build()
    }
}
