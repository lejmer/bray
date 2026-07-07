use crate::SyntaxKind;
use crate::node::define_source_syntax_node;
use crate::syntax::path::PathSyntax;

define_source_syntax_node! {
    /// `using` declaration module item.
    pub struct UsingDeclarationSyntax {
        builder: UsingDeclarationSyntaxBuilder,
        kind: SyntaxKind::UsingDeclaration,
        source_slot: "using_declaration.source",
        node_name: "using declaration",
        range_description: "using-declaration",
        debug_name: "UsingDeclarationSyntax",
        builder_debug_name: "UsingDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `using` keyword token.
                using_keyword;
                /// Appends the `using` keyword token.
                push_using_keyword;
                kind: SyntaxKind::UsingKeyword;
                slot: "using_declaration.using_keyword";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "using_declaration.semicolon_token";
            }
        ],
        optional_tokens: [
            {
                /// Returns the optional `internal` keyword token.
                internal_keyword;
                /// Appends the `internal` keyword token.
                push_internal_keyword;
                kind: SyntaxKind::InternalKeyword;
                slot: "using_declaration.internal_keyword";
            }
        ],
        required_children: [
            {
                /// Returns the imported path child.
                path;
                /// Appends the imported path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
    }
}

define_source_syntax_node! {
    /// `export` declaration module item.
    pub struct ExportDeclarationSyntax {
        builder: ExportDeclarationSyntaxBuilder,
        kind: SyntaxKind::ExportDeclaration,
        source_slot: "export_declaration.source",
        node_name: "export declaration",
        range_description: "export-declaration",
        debug_name: "ExportDeclarationSyntax",
        builder_debug_name: "ExportDeclarationSyntaxBuilder",
        skipped_syntax: true,
        required_tokens: [
            {
                /// Returns the required `export` keyword token.
                export_keyword;
                /// Appends the `export` keyword token.
                push_export_keyword;
                kind: SyntaxKind::ExportKeyword;
                slot: "export_declaration.export_keyword";
            },
            {
                /// Returns the required semicolon token.
                semicolon_token;
                /// Appends the semicolon token.
                push_semicolon_token;
                kind: SyntaxKind::SemicolonToken;
                slot: "export_declaration.semicolon_token";
            }
        ],
        optional_tokens: [],
        required_children: [
            {
                /// Returns the exported path child.
                path;
                /// Appends the exported path child.
                push_path;
                ty: PathSyntax;
                kind: SyntaxKind::Path;
            }
        ],
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{
        SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
    };

    use crate::{
        ExportDeclarationSyntax, PathSyntax, SyntaxKind, SyntaxText, SyntaxToken, SyntaxTrivia,
        UsingDeclarationSyntax,
    };

    #[test]
    fn using_declarations_store_internal_path_and_semicolon() {
        let snapshot = snapshot("using internal core;");
        let mut builder = UsingDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_using_keyword(keyword(SyntaxKind::UsingKeyword, 0, 5, true));
        builder.push_internal_keyword(keyword(SyntaxKind::InternalKeyword, 6, 14, true));
        builder.push_path(path(snapshot, 15, 19));
        builder.push_semicolon_token(token(SyntaxKind::SemicolonToken, 19, 20));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "using internal core;");

        assert_eq!(
            declaration.internal_keyword().map(|token| token.kind()),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(declaration.path().full_text(), "core");

        assert_eq!(
            declaration.semicolon_token().kind(),
            SyntaxKind::SemicolonToken
        );
    }

    #[test]
    fn export_declarations_store_path_and_semicolon() {
        let snapshot = snapshot("export api;");
        let mut builder = ExportDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

        builder.push_export_keyword(keyword(SyntaxKind::ExportKeyword, 0, 6, true));
        builder.push_path(path(snapshot, 7, 10));
        builder.push_semicolon_token(token(SyntaxKind::SemicolonToken, 10, 11));

        let declaration = builder.build();

        assert_eq!(declaration.full_text(), "export api;");
        assert_eq!(declaration.path().full_text(), "api");

        assert_eq!(
            declaration.semicolon_token().kind(),
            SyntaxKind::SemicolonToken
        );
    }

    fn keyword(kind: SyntaxKind, start: u32, end: u32, has_trailing_space: bool) -> SyntaxToken {
        let token = token(kind, start, end);

        if has_trailing_space {
            return token.with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
                TextSize::new(end),
                TextSize::new(end + 1),
            ))]);
        }

        token
    }

    fn path(snapshot: SourceSnapshot, start: u32, end: u32) -> PathSyntax {
        let mut builder = PathSyntax::builder(snapshot);

        builder.push_identifier_token(token(SyntaxKind::IdentifierToken, start, end));

        builder.build()
    }

    fn token(kind: SyntaxKind, start: u32, end: u32) -> SyntaxToken {
        SyntaxToken::new(
            kind,
            TextRange::new(TextSize::new(start), TextSize::new(end)),
        )
    }

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("syntax-module-item-test"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
