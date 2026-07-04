use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

pub(super) fn make_token(
    snapshot: &SourceSnapshot,
    kind: SyntaxKind,
    start: TextSize,
    end: TextSize,
) -> SyntaxToken {
    let range = TextRange::new(start, end);
    let text = token_text(snapshot, range);

    SyntaxToken::new(kind, range, text)
}

pub(super) fn make_scalar_token(
    snapshot: &SourceSnapshot,
    kind: SyntaxKind,
    start: TextSize,
    character: char,
) -> SyntaxToken {
    make_token(
        snapshot,
        kind,
        start,
        offset_after_character(start, character),
    )
}

pub(super) fn token_text(snapshot: &SourceSnapshot, range: TextRange) -> &str {
    match snapshot.text_slice(range) {
        Some(text) => text,
        None => panic!("lexer token range is not on UTF-8 boundaries"),
    }
}

pub(super) fn first_character(snapshot: &SourceSnapshot, start: TextSize) -> Option<char> {
    let source_text = snapshot.text();
    let start_index = text_size_to_usize(start);

    let remainder = match source_text.get(start_index..) {
        Some(remainder) => remainder,
        None => panic!("lexer cursor is not on a UTF-8 boundary"),
    };

    remainder.chars().next()
}

pub(super) fn character_len_at(snapshot: &SourceSnapshot, index: usize) -> Option<usize> {
    snapshot
        .text()
        .get(index..)
        .and_then(|remainder| remainder.chars().next())
        .map(char::len_utf8)
}

pub(super) fn offset_after_character(start: TextSize, character: char) -> TextSize {
    let character_len = text_size_from_usize(character.len_utf8());

    match start.checked_add(character_len) {
        Some(end) => end,
        None => panic!("lexer token range overflowed TextSize"),
    }
}

pub(super) fn text_size_to_usize(size: TextSize) -> usize {
    match usize::try_from(size.bytes()) {
        Ok(size) => size,
        Err(_) => panic!("TextSize did not fit in usize on this target"),
    }
}

pub(super) fn text_size_from_usize(size: usize) -> TextSize {
    match TextSize::try_from(size) {
        Ok(size) => size,
        Err(error) => panic!("source offset should fit in TextSize: {error:?}"),
    }
}
