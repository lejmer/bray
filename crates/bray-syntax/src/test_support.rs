use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
};

use crate::{
    ImplementationBodySyntax, ImplementationSubjectSyntax, PathSyntax, SyntaxKind, SyntaxToken,
    SyntaxTrivia, TraitApplicationSyntax,
};

pub(crate) fn func_keyword_with_trailing_space() -> SyntaxToken {
    keyword(SyntaxKind::FuncKeyword, 0, 4, true)
}

pub(crate) fn func_keyword() -> SyntaxToken {
    token(SyntaxKind::FuncKeyword, 0, 4)
}

pub(crate) fn keyword(
    kind: SyntaxKind,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> SyntaxToken {
    let token = token(kind, start, end);

    if has_trailing_space {
        return token.with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
            TextSize::new(end),
            TextSize::new(end + 1),
        ))]);
    }

    token
}

pub(crate) fn token(kind: SyntaxKind, start: u32, end: u32) -> SyntaxToken {
    SyntaxToken::new(
        kind,
        TextRange::new(TextSize::new(start), TextSize::new(end)),
    )
}

pub(crate) fn identifier_path(snapshot: SourceSnapshot, start: u32, end: u32) -> PathSyntax {
    identifier_path_with_optional_trailing_space(snapshot, start, end, false)
}

pub(crate) fn identifier_path_with_trailing_space(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
) -> PathSyntax {
    identifier_path_with_optional_trailing_space(snapshot, start, end, true)
}

pub(crate) fn implementation_subject(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> ImplementationSubjectSyntax {
    let mut builder = ImplementationSubjectSyntax::builder(snapshot.clone(), TextSize::new(start));

    if has_trailing_space {
        builder.push_path(identifier_path_with_trailing_space(snapshot, start, end));
    } else {
        builder.push_path(identifier_path(snapshot, start, end));
    }

    builder.build()
}

pub(crate) fn trait_application(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
) -> TraitApplicationSyntax {
    let mut builder = TraitApplicationSyntax::builder(snapshot.clone(), TextSize::new(start));

    builder.push_path(identifier_path(snapshot, start, end));

    builder.build()
}

pub(crate) fn implementation_body(
    snapshot: SourceSnapshot,
    start: u32,
    has_trailing_space: bool,
) -> ImplementationBodySyntax {
    let mut builder = ImplementationBodySyntax::builder(snapshot, TextSize::new(start));

    builder.push_open_brace_token(token(SyntaxKind::OpenBraceToken, start, start + 1));
    builder.push_close_brace_token(close_brace_token(start, has_trailing_space));

    builder.build()
}

fn identifier_path_with_optional_trailing_space(
    snapshot: SourceSnapshot,
    start: u32,
    end: u32,
    has_trailing_space: bool,
) -> PathSyntax {
    let mut builder = PathSyntax::builder(snapshot);

    builder.push_identifier_token(keyword(
        SyntaxKind::IdentifierToken,
        start,
        end,
        has_trailing_space,
    ));

    builder.build()
}

fn close_brace_token(start: u32, has_trailing_space: bool) -> SyntaxToken {
    if has_trailing_space {
        return keyword(SyntaxKind::CloseBraceToken, start + 1, start + 2, true);
    }

    token(SyntaxKind::CloseBraceToken, start + 1, start + 2)
}

pub(crate) fn snapshot(origin_name: &'static str, text: &str) -> SourceSnapshot {
    match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source(origin_name),
        SourceVersion::new(0),
        text,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source should fit in TextSize: {error:?}"),
    }
}
