use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
};

use crate::{PathSyntax, SyntaxKind, SyntaxToken, SyntaxTrivia};

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
