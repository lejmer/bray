use super::path::PathSyntax;
use crate::SyntaxKind;
use crate::node::define_source_syntax_node;

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
    }
}

define_source_syntax_node! {
    /// Braced inherent implementation body.
    pub struct InherentImplementationBodySyntax {
        builder: InherentImplementationBodySyntaxBuilder,
        kind: SyntaxKind::InherentImplementationBody,
        source_slot: "inherent_implementation_body.source",
        node_name: "inherent implementation body",
        range_description: "inherent-implementation-body",
        debug_name: "InherentImplementationBodySyntax",
        builder_debug_name: "InherentImplementationBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "inherent_implementation_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "inherent_implementation_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
    }
}

define_source_syntax_node! {
    /// Braced trait implementation body.
    pub struct TraitImplementationBodySyntax {
        builder: TraitImplementationBodySyntaxBuilder,
        kind: SyntaxKind::TraitImplementationBody,
        source_slot: "trait_implementation_body.source",
        node_name: "trait implementation body",
        range_description: "trait-implementation-body",
        debug_name: "TraitImplementationBodySyntax",
        builder_debug_name: "TraitImplementationBodySyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "trait_implementation_body.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "trait_implementation_body.close_brace_token";
            }
        ],
        optional_tokens: [],
        required_children: [],
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
                inherent_implementation_body;
                /// Appends the implementation-body child.
                push_inherent_implementation_body;
                ty: InherentImplementationBodySyntax;
                kind: SyntaxKind::InherentImplementationBody;
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
                trait_implementation_body;
                /// Appends the implementation-body child.
                push_trait_implementation_body;
                ty: TraitImplementationBodySyntax;
                kind: SyntaxKind::TraitImplementationBody;
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
                trait_implementation_body;
                /// Appends the implementation-body child.
                push_trait_implementation_body;
                ty: TraitImplementationBodySyntax;
                kind: SyntaxKind::TraitImplementationBody;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;

    use crate::test_support::{
        identifier_path, implementation_subject, inherent_implementation_body, keyword,
        snapshot as test_snapshot, token, trait_application, trait_implementation_body,
    };
    use crate::{
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

        builder
            .push_inherent_implementation_body(inherent_implementation_body(snapshot, 11, false));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "impl Point {}");
        assert_eq!(
            declaration.implementation_subject().path().full_text(),
            "Point "
        );
        assert_eq!(declaration.inherent_implementation_body().full_text(), "{}");
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
        builder.push_trait_implementation_body(trait_implementation_body(snapshot, 22, false));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "impl Point(Equatable) {}");
        assert_eq!(declaration.implementation_subject().full_text(), "Point");
        assert_eq!(declaration.trait_application().full_text(), "Equatable");
        assert_eq!(declaration.trait_implementation_body().full_text(), "{}");
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

        builder.push_trait_implementation_body(trait_implementation_body(snapshot, 32, false));

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

    // TODO(syntax): Update this when generic argument lists are typed syntax.
    #[test]
    fn implementation_subjects_store_borrow_prefix_and_skipped_generics() {
        let snapshot = test_snapshot("syntax-implementation-test", "&mut Vec<T>");
        let mut builder = ImplementationSubjectSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_ampersand_token(token(SyntaxKind::AmpersandToken, 0, 1));
        builder.push_mut_token(keyword(SyntaxKind::MutKeyword, 1, 4, true));
        builder.push_path(identifier_path(snapshot.clone(), 5, 8));

        builder.push_skipped_tokens([
            token(SyntaxKind::LessToken, 8, 9),
            token(SyntaxKind::IdentifierToken, 9, 10),
            token(SyntaxKind::GreaterToken, 10, 11),
        ]);

        let subject = builder.build();

        assert_eq!(subject.full_text(), "&mut Vec<T>");
        assert_eq!(subject.path().full_text(), "Vec");
        assert_eq!(subject.skipped_syntax().count(), 1);
    }
}
