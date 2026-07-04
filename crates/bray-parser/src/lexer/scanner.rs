use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LexerScanMode {
    Normal,
    TupleElementIndexAfterDot,
}

pub(super) fn scan_token_at(
    snapshot: &SourceSnapshot,
    start: TextSize,
    mode: LexerScanMode,
) -> SyntaxToken {
    let start = skip_whitespace(snapshot, start);

    if start == snapshot.text_len() {
        return SyntaxToken::end_of_file(start);
    }

    if snapshot.text_len() < start {
        panic!("lexer cursor moved past source text");
    }

    match mode {
        LexerScanMode::Normal => scan_normal_token(snapshot, start),
        LexerScanMode::TupleElementIndexAfterDot => {
            scan_tuple_element_index_or_normal(snapshot, start)
        }
    }
}

fn skip_whitespace(snapshot: &SourceSnapshot, start: TextSize) -> TextSize {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start);

    loop {
        match bytes.get(index).copied() {
            Some(b' ' | b'\t' | b'\n') => index += 1,
            Some(b'\r') if bytes.get(index + 1).copied() == Some(b'\n') => index += 2,
            _ => return text_size_from_usize(index),
        }
    }
}

fn scan_normal_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let character = match first_character(snapshot, start) {
        Some(character) => character,
        None => return SyntaxToken::end_of_file(start),
    };

    if character.is_ascii_alphabetic() {
        return scan_identifier_or_keyword(snapshot, start);
    }

    if character == '_' {
        return scan_underscore_or_invalid_identifier(snapshot, start);
    }

    if character.is_ascii_digit() {
        return scan_invalid_numeric_like_token(snapshot, start);
    }

    if let Some(kind) = delimiter_or_separator_kind(character) {
        return make_scalar_token(snapshot, kind, start, character);
    }

    if is_operator_cluster_character(character) {
        return scan_operator_or_punctuation_token(snapshot, start);
    }

    if is_non_ascii_identifier_character(character) {
        return scan_invalid_identifier_like_token(snapshot, start);
    }

    scan_invalid_scalar_token(snapshot, start)
}

fn scan_tuple_element_index_or_normal(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let character = match first_character(snapshot, start) {
        Some(character) => character,
        None => return SyntaxToken::end_of_file(start),
    };

    if character.is_ascii_digit() {
        return scan_tuple_element_index_token(snapshot, start);
    }

    scan_normal_token(snapshot, start)
}

fn scan_identifier_or_keyword(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    match identifier_like_end(snapshot, start) {
        IdentifierScanEnd::Valid(end) => {
            let text = token_text(snapshot, TextRange::new(start, end));
            let kind = keyword_kind(text).unwrap_or(SyntaxKind::IdentifierToken);

            make_token(snapshot, kind, start, end)
        }
        IdentifierScanEnd::Invalid(end) => {
            make_token(snapshot, SyntaxKind::InvalidToken, start, end)
        }
    }
}

fn scan_underscore_or_invalid_identifier(
    snapshot: &SourceSnapshot,
    start: TextSize,
) -> SyntaxToken {
    let end = offset_after_character(start, '_');

    match first_character(snapshot, end) {
        Some(character)
            if character.is_ascii_alphanumeric()
                || character == '_'
                || is_non_ascii_identifier_character(character) =>
        {
            scan_invalid_identifier_like_token(snapshot, start)
        }
        _ => make_token(snapshot, SyntaxKind::UnderscoreToken, start, end),
    }
}

fn scan_tuple_element_index_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let end = decimal_digit_sequence_end(snapshot, start);
    let text = token_text(snapshot, TextRange::new(start, end));

    if tuple_index_text_is_valid(text) {
        make_token(snapshot, SyntaxKind::TupleElementIndexToken, start, end)
    } else {
        scan_invalid_numeric_like_token(snapshot, start)
    }
}

fn scan_operator_or_punctuation_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let end = operator_cluster_end(snapshot, start);
    let text = token_text(snapshot, TextRange::new(start, end));

    match operator_or_punctuation_kind(text) {
        Some(kind) => make_token(snapshot, kind, start, end),
        None => make_token(snapshot, SyntaxKind::InvalidToken, start, end),
    }
}

fn scan_invalid_identifier_like_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let end = invalid_identifier_like_end(snapshot, start);

    make_token(snapshot, SyntaxKind::InvalidToken, start, end)
}

fn scan_invalid_numeric_like_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let end = invalid_numeric_like_end(snapshot, start);

    make_token(snapshot, SyntaxKind::InvalidToken, start, end)
}

fn scan_invalid_scalar_token(snapshot: &SourceSnapshot, start: TextSize) -> SyntaxToken {
    let character = match first_character(snapshot, start) {
        Some(character) => character,
        None => return SyntaxToken::end_of_file(start),
    };

    make_scalar_token(snapshot, SyntaxKind::InvalidToken, start, character)
}

fn make_scalar_token(
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

fn make_token(
    snapshot: &SourceSnapshot,
    kind: SyntaxKind,
    start: TextSize,
    end: TextSize,
) -> SyntaxToken {
    let range = TextRange::new(start, end);
    let text = token_text(snapshot, range);

    SyntaxToken::new(kind, range, text)
}

fn first_character(snapshot: &SourceSnapshot, start: TextSize) -> Option<char> {
    let source_text = snapshot.text();
    let start_index = text_size_to_usize(start);

    let remainder = match source_text.get(start_index..) {
        Some(remainder) => remainder,
        None => panic!("lexer cursor is not on a UTF-8 boundary"),
    };

    remainder.chars().next()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum IdentifierScanEnd {
    Valid(TextSize),
    Invalid(TextSize),
}

fn identifier_like_end(snapshot: &SourceSnapshot, start: TextSize) -> IdentifierScanEnd {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start);

    while let Some(byte) = bytes.get(index).copied() {
        if is_ascii_identifier_continue(byte) {
            index += 1;
            continue;
        }

        break;
    }

    let end = text_size_from_usize(index);

    match first_character(snapshot, end) {
        Some(character) if is_non_ascii_identifier_character(character) => {
            IdentifierScanEnd::Invalid(invalid_identifier_like_end(snapshot, start))
        }
        _ => IdentifierScanEnd::Valid(end),
    }
}

fn invalid_identifier_like_end(snapshot: &SourceSnapshot, start: TextSize) -> TextSize {
    let mut end = start;

    while let Some(character) = first_character(snapshot, end) {
        if character.is_ascii() {
            if character.is_ascii_alphanumeric() || character == '_' {
                end = offset_after_character(end, character);
                continue;
            }

            break;
        }

        if is_non_ascii_identifier_character(character) {
            end = offset_after_character(end, character);
            continue;
        }

        break;
    }

    if end == start {
        match first_character(snapshot, start) {
            Some(character) => offset_after_character(start, character),
            None => start,
        }
    } else {
        end
    }
}

fn invalid_numeric_like_end(snapshot: &SourceSnapshot, start: TextSize) -> TextSize {
    let mut end = start;

    while let Some(character) = first_character(snapshot, end) {
        if character.is_ascii_alphanumeric() || character == '_' {
            end = offset_after_character(end, character);
            continue;
        }

        break;
    }

    if end == start {
        match first_character(snapshot, start) {
            Some(character) => offset_after_character(start, character),
            None => start,
        }
    } else {
        end
    }
}

fn decimal_digit_sequence_end(snapshot: &SourceSnapshot, start: TextSize) -> TextSize {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start);

    while let Some(byte) = bytes.get(index).copied() {
        if byte.is_ascii_digit() {
            index += 1;
            continue;
        }

        break;
    }

    text_size_from_usize(index)
}

fn operator_cluster_end(snapshot: &SourceSnapshot, start: TextSize) -> TextSize {
    let bytes = snapshot.bytes();

    let mut index = text_size_to_usize(start);

    while let Some(byte) = bytes.get(index).copied() {
        let character = char::from(byte);

        if is_operator_cluster_character(character) {
            index += 1;
            continue;
        }

        break;
    }

    text_size_from_usize(index)
}

fn token_text(snapshot: &SourceSnapshot, range: TextRange) -> &str {
    match snapshot.text_slice(range) {
        Some(text) => text,
        None => panic!("lexer token range is not on UTF-8 boundaries"),
    }
}

fn tuple_index_text_is_valid(text: &str) -> bool {
    if text == "0" {
        return true;
    }

    matches!(text.as_bytes().first().copied(), Some(b'1'..=b'9'))
}

fn keyword_kind(text: &str) -> Option<SyntaxKind> {
    match text {
        "all" => Some(SyntaxKind::AllKeyword),
        "any" => Some(SyntaxKind::AnyKeyword),
        "as" => Some(SyntaxKind::AsKeyword),
        "assert" => Some(SyntaxKind::AssertKeyword),
        "async" => Some(SyntaxKind::AsyncKeyword),
        "await" => Some(SyntaxKind::AwaitKeyword),
        "box" => Some(SyntaxKind::BoxKeyword),
        "break" => Some(SyntaxKind::BreakKeyword),
        "callable" => Some(SyntaxKind::CallableKeyword),
        "case" => Some(SyntaxKind::CaseKeyword),
        "catch" => Some(SyntaxKind::CatchKeyword),
        "const" => Some(SyntaxKind::ConstKeyword),
        "construct" => Some(SyntaxKind::ConstructKeyword),
        "consume" => Some(SyntaxKind::ConsumeKeyword),
        "continue" => Some(SyntaxKind::ContinueKeyword),
        "destruct" => Some(SyntaxKind::DestructKeyword),
        "detached" => Some(SyntaxKind::DetachedKeyword),
        "each" => Some(SyntaxKind::EachKeyword),
        "else" => Some(SyntaxKind::ElseKeyword),
        "ensures" => Some(SyntaxKind::EnsuresKeyword),
        "enter" => Some(SyntaxKind::EnterKeyword),
        "exit" => Some(SyntaxKind::ExitKeyword),
        "export" => Some(SyntaxKind::ExportKeyword),
        "extern" => Some(SyntaxKind::ExternKeyword),
        "false" => Some(SyntaxKind::FalseKeyword),
        "finalize" => Some(SyntaxKind::FinalizeKeyword),
        "for" => Some(SyntaxKind::ForKeyword),
        "func" => Some(SyntaxKind::FuncKeyword),
        "if" => Some(SyntaxKind::IfKeyword),
        "impl" => Some(SyntaxKind::ImplKeyword),
        "in" => Some(SyntaxKind::InKeyword),
        "internal" => Some(SyntaxKind::InternalKeyword),
        "lambda" => Some(SyntaxKind::LambdaKeyword),
        "let" => Some(SyntaxKind::LetKeyword),
        "loop" => Some(SyntaxKind::LoopKeyword),
        "match" => Some(SyntaxKind::MatchKeyword),
        "module" => Some(SyntaxKind::ModuleKeyword),
        "move" => Some(SyntaxKind::MoveKeyword),
        "mut" => Some(SyntaxKind::MutKeyword),
        "none" => Some(SyntaxKind::NoneKeyword),
        "overload" => Some(SyntaxKind::OverloadKeyword),
        "panic" => Some(SyntaxKind::PanicKeyword),
        "pos" => Some(SyntaxKind::PosKeyword),
        "predicate" => Some(SyntaxKind::PredicateKeyword),
        "public" => Some(SyntaxKind::PublicKeyword),
        "requires" => Some(SyntaxKind::RequiresKeyword),
        "return" => Some(SyntaxKind::ReturnKeyword),
        "self" => Some(SyntaxKind::SelfValueKeyword),
        "Self" => Some(SyntaxKind::SelfTypeKeyword),
        "spawn" => Some(SyntaxKind::SpawnKeyword),
        "static" => Some(SyntaxKind::StaticKeyword),
        "struct" => Some(SyntaxKind::StructKeyword),
        "thread" => Some(SyntaxKind::ThreadKeyword),
        "trait" => Some(SyntaxKind::TraitKeyword),
        "trusted" => Some(SyntaxKind::TrustedKeyword),
        "true" => Some(SyntaxKind::TrueKeyword),
        "try" => Some(SyntaxKind::TryKeyword),
        "type" => Some(SyntaxKind::TypeKeyword),
        "union" => Some(SyntaxKind::UnionKeyword),
        "unit" => Some(SyntaxKind::UnitKeyword),
        "using" => Some(SyntaxKind::UsingKeyword),
        "uses" => Some(SyntaxKind::UsesKeyword),
        "view" => Some(SyntaxKind::ViewKeyword),
        "when" => Some(SyntaxKind::WhenKeyword),
        "while" => Some(SyntaxKind::WhileKeyword),
        "with" => Some(SyntaxKind::WithKeyword),
        "yield" => Some(SyntaxKind::YieldKeyword),
        _ => None,
    }
}

fn operator_or_punctuation_kind(text: &str) -> Option<SyntaxKind> {
    match text {
        "->" => Some(SyntaxKind::ArrowToken),
        "==" => Some(SyntaxKind::EqualsEqualsToken),
        "!=" => Some(SyntaxKind::BangEqualsToken),
        "<=" => Some(SyntaxKind::LessEqualsToken),
        ">=" => Some(SyntaxKind::GreaterEqualsToken),
        "&&" => Some(SyntaxKind::AmpersandAmpersandToken),
        "||" => Some(SyntaxKind::PipePipeToken),
        "<<" => Some(SyntaxKind::LessLessToken),
        ">>" => Some(SyntaxKind::GreaterGreaterToken),
        "**" => Some(SyntaxKind::StarStarToken),
        ".." => Some(SyntaxKind::DotDotToken),
        ":" => Some(SyntaxKind::ColonToken),
        "." => Some(SyntaxKind::DotToken),
        "?" => Some(SyntaxKind::QuestionToken),
        "=" => Some(SyntaxKind::EqualsToken),
        "+" => Some(SyntaxKind::PlusToken),
        "-" => Some(SyntaxKind::MinusToken),
        "*" => Some(SyntaxKind::StarToken),
        "/" => Some(SyntaxKind::SlashToken),
        "%" => Some(SyntaxKind::PercentToken),
        "@" => Some(SyntaxKind::AtToken),
        "&" => Some(SyntaxKind::AmpersandToken),
        "|" => Some(SyntaxKind::PipeToken),
        "^" => Some(SyntaxKind::CaretToken),
        "~" => Some(SyntaxKind::TildeToken),
        "!" => Some(SyntaxKind::BangToken),
        "<" => Some(SyntaxKind::LessToken),
        ">" => Some(SyntaxKind::GreaterToken),
        _ => None,
    }
}

fn delimiter_or_separator_kind(character: char) -> Option<SyntaxKind> {
    match character {
        '(' => Some(SyntaxKind::OpenParenToken),
        ')' => Some(SyntaxKind::CloseParenToken),
        '{' => Some(SyntaxKind::OpenBraceToken),
        '}' => Some(SyntaxKind::CloseBraceToken),
        '[' => Some(SyntaxKind::OpenBracketToken),
        ']' => Some(SyntaxKind::CloseBracketToken),
        ',' => Some(SyntaxKind::CommaToken),
        ';' => Some(SyntaxKind::SemicolonToken),
        _ => None,
    }
}

fn is_operator_cluster_character(character: char) -> bool {
    matches!(
        character,
        '-' | '='
            | '!'
            | '<'
            | '>'
            | '&'
            | '|'
            | '*'
            | '.'
            | '+'
            | '/'
            | '%'
            | '@'
            | '^'
            | '~'
            | '?'
            | ':'
    )
}

fn is_ascii_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_non_ascii_identifier_character(character: char) -> bool {
    !character.is_ascii() && character.is_alphanumeric()
}

fn text_size_to_usize(size: TextSize) -> usize {
    match usize::try_from(size.bytes()) {
        Ok(size) => size,
        Err(_) => panic!("TextSize did not fit in usize on this target"),
    }
}

fn text_size_from_usize(size: usize) -> TextSize {
    match TextSize::try_from(size) {
        Ok(size) => size,
        Err(error) => panic!("source offset should fit in TextSize: {error:?}"),
    }
}

fn offset_after_character(start: TextSize, character: char) -> TextSize {
    let character_len = text_size_from_usize(character.len_utf8());

    match start.checked_add(character_len) {
        Some(end) => end,
        None => panic!("lexer token range overflowed TextSize"),
    }
}
