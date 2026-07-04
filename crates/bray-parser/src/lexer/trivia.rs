use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxTrivia};

use super::text::{text_size_from_usize, text_size_to_usize};

pub(super) struct TriviaScan {
    trivia: Vec<SyntaxTrivia>,
    end: TextSize,
    reached_eof: bool,
}

impl TriviaScan {
    pub(super) fn into_trivia(self) -> Vec<SyntaxTrivia> {
        self.trivia
    }

    pub(super) const fn end(&self) -> TextSize {
        self.end
    }

    pub(super) const fn reached_eof(&self) -> bool {
        self.reached_eof
    }
}

pub(super) fn scan_leading_trivia(snapshot: &SourceSnapshot, start: TextSize) -> TriviaScan {
    let mut trivia = Vec::new();
    let mut cursor = start;

    while let Some(item) = scan_trivia_at(snapshot, cursor, TriviaPosition::Leading) {
        cursor = item.trivia.end();
        trivia.push(item.trivia);
    }

    TriviaScan {
        trivia,
        end: cursor,
        reached_eof: cursor == snapshot.text_len(),
    }
}

pub(super) fn scan_trailing_trivia(snapshot: &SourceSnapshot, start: TextSize) -> TriviaScan {
    let mut trivia = Vec::new();
    let mut cursor = start;

    while let Some(item) = scan_trivia_at(snapshot, cursor, TriviaPosition::Trailing) {
        cursor = item.trivia.end();

        let contains_line_break = item.contains_line_break;

        trivia.push(item.trivia);

        if contains_line_break {
            break;
        }
    }

    TriviaScan {
        trivia,
        end: cursor,
        reached_eof: cursor == snapshot.text_len(),
    }
}

struct TriviaItem {
    trivia: SyntaxTrivia,
    contains_line_break: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TriviaPosition {
    Leading,
    Trailing,
}

fn scan_trivia_at(
    snapshot: &SourceSnapshot,
    start: TextSize,
    position: TriviaPosition,
) -> Option<TriviaItem> {
    let bytes = snapshot.bytes();
    let start_index = text_size_to_usize(start);

    match bytes.get(start_index).copied()? {
        b' ' | b'\t' | b'\n' => Some(scan_whitespace_trivia(snapshot, start, position)),
        b'\r' if bytes.get(start_index + 1).copied() == Some(b'\n') => {
            Some(scan_whitespace_trivia(snapshot, start, position))
        }
        b'/' if bytes.get(start_index + 1).copied() == Some(b'/') => {
            Some(scan_line_comment_trivia(snapshot, start))
        }
        b'/' if bytes.get(start_index + 1).copied() == Some(b'*') => {
            Some(scan_block_comment_trivia(snapshot, start))
        }
        _ => None,
    }
}

fn scan_whitespace_trivia(
    snapshot: &SourceSnapshot,
    start: TextSize,
    position: TriviaPosition,
) -> TriviaItem {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start);
    let mut contains_line_break = false;

    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b' ' | b'\t' => index += 1,
            b'\n' => {
                index += 1;
                contains_line_break = true;

                if position == TriviaPosition::Trailing {
                    break;
                }
            }
            b'\r' if bytes.get(index + 1).copied() == Some(b'\n') => {
                index += 2;
                contains_line_break = true;

                if position == TriviaPosition::Trailing {
                    break;
                }
            }
            _ => break,
        }
    }

    make_trivia_item(
        snapshot,
        SyntaxKind::WhitespaceTrivia,
        start,
        text_size_from_usize(index),
        contains_line_break,
    )
}

fn scan_line_comment_trivia(snapshot: &SourceSnapshot, start: TextSize) -> TriviaItem {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start) + 2;

    while let Some(byte) = bytes.get(index).copied() {
        match byte {
            b'\n' | b'\r' => break,
            _ => index += 1,
        }
    }

    let kind = if starts_with(snapshot, start, "///") {
        SyntaxKind::DocumentationLineCommentTrivia
    } else {
        SyntaxKind::LineCommentTrivia
    };

    make_trivia_item(snapshot, kind, start, text_size_from_usize(index), false)
}

fn scan_block_comment_trivia(snapshot: &SourceSnapshot, start: TextSize) -> TriviaItem {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start) + 2;
    let mut depth = 1_u32;
    let mut contains_line_break = false;

    while index < bytes.len() {
        if bytes.get(index).copied() == Some(b'\n') {
            contains_line_break = true;
            index += 1;

            continue;
        }

        if bytes.get(index).copied() == Some(b'\r') && bytes.get(index + 1).copied() == Some(b'\n')
        {
            contains_line_break = true;
            index += 2;

            continue;
        }

        if bytes.get(index).copied() == Some(b'/') && bytes.get(index + 1).copied() == Some(b'*') {
            depth += 1;
            index += 2;

            continue;
        }

        if bytes.get(index).copied() == Some(b'*') && bytes.get(index + 1).copied() == Some(b'/') {
            depth -= 1;
            index += 2;

            if depth == 0 {
                break;
            }

            continue;
        }

        index += next_utf8_character_len(snapshot, text_size_from_usize(index));
    }

    let kind = if starts_with(snapshot, start, "/**") {
        SyntaxKind::DocumentationBlockCommentTrivia
    } else {
        SyntaxKind::BlockCommentTrivia
    };

    make_trivia_item(
        snapshot,
        kind,
        start,
        text_size_from_usize(index),
        contains_line_break,
    )
}

fn make_trivia_item(
    snapshot: &SourceSnapshot,
    kind: SyntaxKind,
    start: TextSize,
    end: TextSize,
    contains_line_break: bool,
) -> TriviaItem {
    let range = TextRange::new(start, end);

    let text = match snapshot.text_slice(range) {
        Some(text) => text,
        None => panic!("lexer trivia range is not on UTF-8 boundaries"),
    };

    TriviaItem {
        trivia: SyntaxTrivia::new(kind, range, text),
        contains_line_break,
    }
}

fn starts_with(snapshot: &SourceSnapshot, start: TextSize, text: &str) -> bool {
    let start_index = text_size_to_usize(start);

    match snapshot.text().get(start_index..) {
        Some(source_text) => source_text.starts_with(text),
        None => panic!("lexer cursor is not on a UTF-8 boundary"),
    }
}

fn next_utf8_character_len(snapshot: &SourceSnapshot, start: TextSize) -> usize {
    let start_index = text_size_to_usize(start);

    let source_text = match snapshot.text().get(start_index..) {
        Some(source_text) => source_text,
        None => panic!("lexer cursor is not on a UTF-8 boundary"),
    };

    match source_text.chars().next() {
        Some(character) => character.len_utf8(),
        None => 0,
    }
}
