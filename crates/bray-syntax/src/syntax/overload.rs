use super::path::PathSyntax;
use crate::node::define_source_syntax_node;
use crate::{SyntaxKind, SyntaxToken};

define_source_syntax_node! {
    /// Optional overload declaration modifiers in source order.
    pub struct OverloadModifiersSyntax {
        builder: OverloadModifiersSyntaxBuilder,
        kind: SyntaxKind::OverloadModifiers,
        source_slot: "overload_modifiers.source",
        node_name: "overload modifiers",
        range_description: "overload-modifiers",
        debug_name: "OverloadModifiersSyntax",
        builder_debug_name: "OverloadModifiersSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [],
    }
}

impl OverloadModifiersSyntax {
    /// Returns the optional visibility modifier token.
    pub fn visibility_token(&self) -> Option<SyntaxToken> {
        self.tokens()
            .find(|token| token.kind().is_visibility_modifier())
    }
}

impl OverloadModifiersSyntaxBuilder {
    /// Appends the optional visibility modifier token.
    pub fn push_visibility_token(&mut self, token: SyntaxToken) {
        assert!(
            token.kind().is_visibility_modifier(),
            "overload_modifiers.visibility_token expected a visibility modifier"
        );

        self.node.push_token(token);
    }
}

define_source_syntax_node! {
    /// Single overload arm.
    pub struct OverloadArmSyntax {
        builder: OverloadArmSyntaxBuilder,
        kind: SyntaxKind::OverloadArm,
        source_slot: "overload_arm.source",
        node_name: "overload arm",
        range_description: "overload-arm",
        debug_name: "OverloadArmSyntax",
        builder_debug_name: "OverloadArmSyntaxBuilder",
        skipped_syntax: false,
        required_tokens: [],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the arm path child.
                path;
                /// Appends the arm path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Braced overload arm list.
    pub struct OverloadArmListSyntax {
        builder: OverloadArmListSyntaxBuilder,
        kind: SyntaxKind::OverloadArmList,
        source_slot: "overload_arm_list.source",
        node_name: "overload arm list",
        range_description: "overload-arm-list",
        debug_name: "OverloadArmListSyntax",
        builder_debug_name: "OverloadArmListSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required opening brace token.
                open_brace_token;
                /// Appends the opening brace token.
                push_open_brace_token;
                kind: SyntaxKind::OpenBraceToken;
                slot: "overload_arm_list.open_brace_token";
            },
            {
                /// Returns the required closing brace token.
                close_brace_token;
                /// Appends the closing brace token.
                push_close_brace_token;
                kind: SyntaxKind::CloseBraceToken;
                slot: "overload_arm_list.close_brace_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the first comma separator token.
                comma_token;
                /// Appends a comma separator token.
                push_separator_token;
                kind: SyntaxKind::CommaToken;
                slot: "overload_arm_list.comma_token";
            }
        ],
        required_children: [],
        repeated_children: [
            {
                /// Returns overload arms in source order.
                overload_arms;
                /// Appends an overload arm child.
                push_overload_arm;
                ty: OverloadArmSyntax;
                kind: SyntaxKind::OverloadArm;
            }
        ],
    }
}

impl OverloadArmListSyntax {
    /// Returns comma separator tokens in source order.
    pub fn separator_tokens(&self) -> impl Iterator<Item = SyntaxToken> + '_ {
        self.tokens()
            .filter(|token| token.kind() == SyntaxKind::CommaToken)
    }
}

define_source_syntax_node! {
    /// Subject named by an implementation overload declaration.
    pub struct ImplementationOverloadSubjectSyntax {
        builder: ImplementationOverloadSubjectSyntaxBuilder,
        kind: SyntaxKind::ImplementationOverloadSubject,
        source_slot: "implementation_overload_subject.source",
        node_name: "implementation overload subject",
        range_description: "implementation-overload-subject",
        debug_name: "ImplementationOverloadSubjectSyntax",
        builder_debug_name: "ImplementationOverloadSubjectSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [],
        optional_tokens: [
            {
                /// Returns the optional grouped-subject opening parenthesis token.
                open_paren_token;
                /// Appends a grouped-subject opening parenthesis token.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "implementation_overload_subject.open_paren_token";
            },
            {
                /// Returns the optional borrowed-subject ampersand token.
                ampersand_token;
                /// Appends a borrowed-subject ampersand token.
                push_ampersand_token;
                kind: SyntaxKind::AmpersandToken;
                slot: "implementation_overload_subject.ampersand_token";
            },
            {
                /// Returns the optional borrowed-subject `mut` token.
                mut_token;
                /// Appends a borrowed-subject `mut` token.
                push_mut_token;
                kind: SyntaxKind::MutKeyword;
                slot: "implementation_overload_subject.mut_token";
            },
            {
                /// Returns the optional grouped-subject closing parenthesis token.
                close_paren_token;
                /// Appends a grouped-subject closing parenthesis token.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "implementation_overload_subject.close_paren_token";
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
    /// Callable overload declaration.
    pub struct CallableOverloadDeclarationSyntax {
        builder: CallableOverloadDeclarationSyntaxBuilder,
        kind: SyntaxKind::CallableOverloadDeclaration,
        source_slot: "callable_overload_declaration.source",
        node_name: "callable overload declaration",
        range_description: "callable-overload-declaration",
        debug_name: "CallableOverloadDeclarationSyntax",
        builder_debug_name: "CallableOverloadDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `overload` keyword token.
                overload_keyword;
                /// Appends the `overload` keyword token.
                push_overload_keyword;
                kind: SyntaxKind::OverloadKeyword;
                slot: "callable_overload_declaration.overload_keyword";
            },
            {
                /// Returns the required overload name token.
                identifier_token;
                /// Appends the overload name token.
                push_identifier_token;
                kind: SyntaxKind::IdentifierToken;
                slot: "callable_overload_declaration.identifier_token";
            },
            {
                /// Returns the required equals token.
                equals_token;
                /// Appends the equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "callable_overload_declaration.equals_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the overload-modifiers child.
                overload_modifiers;
                /// Appends the overload-modifiers child.
                push_overload_modifiers;
                ty: OverloadModifiersSyntax;
                kind: SyntaxKind::OverloadModifiers;
            },
            {
                /// Returns the overload-arm-list child.
                overload_arm_list;
                /// Appends the overload-arm-list child.
                push_overload_arm_list;
                ty: OverloadArmListSyntax;
                kind: SyntaxKind::OverloadArmList;
            }
        ],
    }
}

define_source_syntax_node! {
    /// Implementation overload declaration.
    pub struct ImplementationOverloadDeclarationSyntax {
        builder: ImplementationOverloadDeclarationSyntaxBuilder,
        kind: SyntaxKind::ImplementationOverloadDeclaration,
        source_slot: "implementation_overload_declaration.source",
        node_name: "implementation overload declaration",
        range_description: "implementation-overload-declaration",
        debug_name: "ImplementationOverloadDeclarationSyntax",
        builder_debug_name: "ImplementationOverloadDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `overload` keyword token.
                overload_keyword;
                /// Appends the `overload` keyword token.
                push_overload_keyword;
                kind: SyntaxKind::OverloadKeyword;
                slot: "implementation_overload_declaration.overload_keyword";
            },
            {
                /// Returns the required opening parenthesis token around the trait path.
                open_paren_token;
                /// Appends the opening parenthesis token around the trait path.
                push_open_paren_token;
                kind: SyntaxKind::OpenParenToken;
                slot: "implementation_overload_declaration.open_paren_token";
            },
            {
                /// Returns the required closing parenthesis token around the trait path.
                close_paren_token;
                /// Appends the closing parenthesis token around the trait path.
                push_close_paren_token;
                kind: SyntaxKind::CloseParenToken;
                slot: "implementation_overload_declaration.close_paren_token";
            },
            {
                /// Returns the required equals token.
                equals_token;
                /// Appends the equals token.
                push_equals_token;
                kind: SyntaxKind::EqualsToken;
                slot: "implementation_overload_declaration.equals_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the overload-modifiers child.
                overload_modifiers;
                /// Appends the overload-modifiers child.
                push_overload_modifiers;
                ty: OverloadModifiersSyntax;
                kind: SyntaxKind::OverloadModifiers;
            },
            {
                /// Returns the implementation-overload-subject child.
                implementation_overload_subject;
                /// Appends the implementation-overload-subject child.
                push_implementation_overload_subject;
                ty: ImplementationOverloadSubjectSyntax;
                kind: SyntaxKind::ImplementationOverloadSubject;
            },
            {
                /// Returns the trait path child.
                trait_path;
                /// Appends the trait path child.
                push_trait_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            },
            {
                /// Returns the overload-arm-list child.
                overload_arm_list;
                /// Appends the overload-arm-list child.
                push_overload_arm_list;
                ty: OverloadArmListSyntax;
                kind: SyntaxKind::OverloadArmList;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceSnapshot, TextRange, TextSize};

    use crate::test_support::{identifier_path, snapshot as test_snapshot, token};
    use crate::{
        CallableOverloadDeclarationSyntax, ImplementationOverloadDeclarationSyntax,
        ImplementationOverloadSubjectSyntax, OverloadArmListSyntax, OverloadArmSyntax,
        OverloadModifiersSyntax, SyntaxKind, SyntaxText, SyntaxTrivia,
    };

    #[test]
    fn callable_overload_declarations_store_modifiers_name_and_arms() {
        let snapshot = test_snapshot(
            "syntax-overload-test",
            "public overload draw = {fast,slow,}",
        );

        let mut builder =
            CallableOverloadDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_overload_modifiers(overload_modifiers(snapshot.clone()));
        builder.push_overload_keyword(keyword(SyntaxKind::OverloadKeyword, 7, 15, true));
        builder.push_identifier_token(keyword(SyntaxKind::IdentifierToken, 16, 20, true));
        builder.push_equals_token(keyword(SyntaxKind::EqualsToken, 21, 22, true));
        builder.push_overload_arm_list(overload_arm_list(snapshot, 23));

        let declaration = builder.build();

        assert_eq!(
            declaration.full_text(),
            "public overload draw = {fast,slow,}"
        );

        assert_eq!(
            declaration
                .overload_modifiers()
                .visibility_token()
                .map(|token| token.kind()),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(declaration.overload_arm_list().overload_arms().count(), 2);

        assert_eq!(
            declaration.overload_arm_list().separator_tokens().count(),
            2
        );
    }

    #[test]
    fn implementation_overload_declarations_store_subject_trait_and_arms() {
        let snapshot = test_snapshot(
            "syntax-overload-test",
            "overload (&mut Point)(Shape) = {point_shape}",
        );

        let mut builder =
            ImplementationOverloadDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_overload_modifiers(
            OverloadModifiersSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
        );

        builder.push_overload_keyword(keyword(SyntaxKind::OverloadKeyword, 0, 8, true));
        builder.push_implementation_overload_subject(implementation_subject(snapshot.clone()));
        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 21, 22));
        builder.push_trait_path(identifier_path(snapshot.clone(), 22, 27));
        builder.push_close_paren_token(keyword(SyntaxKind::CloseParenToken, 27, 28, true));
        builder.push_equals_token(keyword(SyntaxKind::EqualsToken, 29, 30, true));
        builder.push_overload_arm_list(single_overload_arm_list(snapshot, 31, 32, 43, 43));

        let declaration = builder.build();
        let subject = declaration.implementation_overload_subject();

        assert_eq!(
            declaration.full_text(),
            "overload (&mut Point)(Shape) = {point_shape}"
        );

        assert!(subject.open_paren_token().is_some());
        assert!(subject.ampersand_token().is_some());
        assert!(subject.mut_token().is_some());
        assert_eq!(declaration.trait_path().full_text(), "Shape");
    }

    fn overload_modifiers(snapshot: SourceSnapshot) -> OverloadModifiersSyntax {
        let mut builder = OverloadModifiersSyntax::builder(snapshot, TextSize::ZERO);

        builder.push_visibility_token(keyword(SyntaxKind::PublicKeyword, 0, 6, true));

        builder.build()
    }

    fn implementation_subject(snapshot: SourceSnapshot) -> ImplementationOverloadSubjectSyntax {
        let mut builder =
            ImplementationOverloadSubjectSyntax::builder(snapshot.clone(), TextSize::new(9));

        builder.push_open_paren_token(token(SyntaxKind::OpenParenToken, 9, 10));
        builder.push_ampersand_token(token(SyntaxKind::AmpersandToken, 10, 11));
        builder.push_mut_token(keyword(SyntaxKind::MutKeyword, 11, 14, true));
        builder.push_path(identifier_path(snapshot, 15, 20));
        builder.push_close_paren_token(token(SyntaxKind::CloseParenToken, 20, 21));

        builder.build()
    }

    fn overload_arm_list(snapshot: SourceSnapshot, start: u32) -> OverloadArmListSyntax {
        let mut builder = OverloadArmListSyntax::builder(snapshot.clone(), TextSize::new(start));

        builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
        builder.push_overload_arm(overload_arm(snapshot.clone(), start + 1, start + 5));
        builder.push_separator_token(token(SyntaxKind::CommaToken, start + 5, start + 6));
        builder.push_overload_arm(overload_arm(snapshot, start + 6, start + 10));
        builder.push_separator_token(token(SyntaxKind::CommaToken, start + 10, start + 11));
        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, start + 11, start + 12));

        builder.build()
    }

    fn single_overload_arm_list(
        snapshot: SourceSnapshot,
        start: u32,
        arm_start: u32,
        arm_end: u32,
        close: u32,
    ) -> OverloadArmListSyntax {
        let mut builder = OverloadArmListSyntax::builder(snapshot.clone(), TextSize::new(start));

        builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
        builder.push_overload_arm(overload_arm(snapshot, arm_start, arm_end));
        builder.push_close_brace_token(token(SyntaxKind::CloseBraceToken, close, close + 1));

        builder.build()
    }

    fn overload_arm(snapshot: SourceSnapshot, start: u32, end: u32) -> OverloadArmSyntax {
        let mut builder = OverloadArmSyntax::builder(snapshot.clone(), TextSize::new(start));

        builder.push_path(identifier_path(snapshot, start, end));

        builder.build()
    }

    fn keyword(
        kind: SyntaxKind,
        start: u32,
        end: u32,
        has_trailing_space: bool,
    ) -> crate::SyntaxToken {
        let token = token(kind, start, end);

        if has_trailing_space {
            return token.with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(end),
                TextSize::new(end + 1),
            ))]);
        }

        token
    }
}
