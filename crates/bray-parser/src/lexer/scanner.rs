use bray_source::{SourceSnapshot, TextRange, TextSize};
use bray_syntax::{SyntaxKind, SyntaxToken};

use super::diagnostic;
use super::literal::{
    scan_character_literal, scan_numeric_literal, scan_string_literal,
    scan_tuple_element_index_token,
};
use super::scan::TokenScan;
use super::text::{
    first_character, make_scalar_token, make_token, offset_after_character, text_size_from_usize,
    text_size_to_usize, token_text,
};
use super::trivia::{scan_leading_trivia, scan_trailing_trivia};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LexerScanMode {
    Normal,
    TupleElementIndexAfterDot,
}

pub(super) fn scan_token_at(
    snapshot: &SourceSnapshot,
    start: TextSize,
    mode: LexerScanMode,
) -> TokenScan {
    if snapshot.text_len() < start {
        panic!("lexer cursor moved past source text");
    }

    let leading_trivia = scan_leading_trivia(snapshot, start);
    let token_start = leading_trivia.end();
    let mut diagnostics = leading_trivia.diagnostics().clone();

    if token_start == snapshot.text_len() {
        let token =
            SyntaxToken::end_of_file(token_start).with_leading_trivia(leading_trivia.into_trivia());

        return TokenScan::with_diagnostics(token, diagnostics);
    }

    let token_scan = scan_token_core(snapshot, token_start, mode);
    diagnostics = diagnostics.merged(token_scan.diagnostics());

    let token = token_scan.into_token();
    let trailing_trivia = scan_trailing_trivia(snapshot, token.end());
    diagnostics = diagnostics.merged(trailing_trivia.diagnostics());

    if trailing_trivia.reached_eof() {
        return TokenScan::with_diagnostics(
            token.with_leading_trivia(leading_trivia.into_trivia()),
            diagnostics,
        );
    }

    TokenScan::with_diagnostics(
        token
            .with_leading_trivia(leading_trivia.into_trivia())
            .with_trailing_trivia(trailing_trivia.into_trivia()),
        diagnostics,
    )
}

fn scan_token_core(snapshot: &SourceSnapshot, start: TextSize, mode: LexerScanMode) -> TokenScan {
    match mode {
        LexerScanMode::Normal => scan_normal_token(snapshot, start),
        LexerScanMode::TupleElementIndexAfterDot => {
            scan_tuple_element_index_or_normal(snapshot, start)
        }
    }
}

fn scan_normal_token(snapshot: &SourceSnapshot, start: TextSize) -> TokenScan {
    let character = match first_character(snapshot, start) {
        Some(character) => character,
        None => return TokenScan::clean(SyntaxToken::end_of_file(start)),
    };

    if character.is_ascii_alphabetic() {
        return scan_identifier_or_keyword(snapshot, start);
    }

    if character == '_' {
        return scan_underscore_or_invalid_identifier(snapshot, start);
    }

    if character.is_ascii_digit() {
        return scan_numeric_literal(snapshot, start);
    }

    if character == '\'' {
        return scan_character_literal(snapshot, start);
    }

    if character == '"' {
        return scan_string_literal(snapshot, start);
    }

    if let Some(kind) = delimiter_or_separator_kind(character) {
        return TokenScan::clean(make_scalar_token(kind, start, character));
    }

    if is_operator_cluster_character(character) {
        return scan_operator_or_punctuation_token(snapshot, start);
    }

    if is_non_ascii_identifier_character(character) {
        let end = invalid_identifier_like_end(snapshot, start);

        return scan_invalid_identifier_like_token(snapshot, start, end);
    }

    scan_invalid_scalar_token(snapshot, start)
}

fn scan_tuple_element_index_or_normal(snapshot: &SourceSnapshot, start: TextSize) -> TokenScan {
    let character = match first_character(snapshot, start) {
        Some(character) => character,
        None => return TokenScan::clean(SyntaxToken::end_of_file(start)),
    };

    if character.is_ascii_digit() {
        return scan_tuple_element_index_token(snapshot, start);
    }

    scan_normal_token(snapshot, start)
}

fn scan_identifier_or_keyword(snapshot: &SourceSnapshot, start: TextSize) -> TokenScan {
    match identifier_like_end(snapshot, start) {
        IdentifierScanEnd::Valid(end) => {
            let text = token_text(snapshot, TextRange::new(start, end));
            let kind = keyword_kind(text).unwrap_or(SyntaxKind::IdentifierToken);

            TokenScan::clean(make_token(kind, start, end))
        }
        IdentifierScanEnd::Invalid(end) => scan_invalid_identifier_like_token(snapshot, start, end),
    }
}

fn scan_underscore_or_invalid_identifier(snapshot: &SourceSnapshot, start: TextSize) -> TokenScan {
    let end = offset_after_character(start, '_');

    match first_character(snapshot, end) {
        Some(character)
            if character.is_ascii_alphanumeric()
                || character == '_'
                || is_non_ascii_identifier_character(character) =>
        {
            let end = invalid_identifier_like_end(snapshot, start);

            scan_invalid_identifier_like_token(snapshot, start, end)
        }
        _ => TokenScan::clean(make_token(SyntaxKind::UnderscoreToken, start, end)),
    }
}

fn scan_operator_or_punctuation_token(snapshot: &SourceSnapshot, start: TextSize) -> TokenScan {
    let end = operator_cluster_end(snapshot, start);
    let text = token_text(snapshot, TextRange::new(start, end));

    match operator_or_punctuation_kind(text) {
        Some(kind) => TokenScan::clean(make_token(kind, start, end)),
        None => {
            let range = TextRange::new(start, end);
            let token = make_token(SyntaxKind::InvalidToken, start, end);

            TokenScan::with_diagnostic(
                token,
                diagnostic::invalid_operator_or_punctuation(snapshot, range),
            )
        }
    }
}

fn scan_invalid_identifier_like_token(
    snapshot: &SourceSnapshot,
    start: TextSize,
    end: TextSize,
) -> TokenScan {
    let range = TextRange::new(start, end);
    let token = make_token(SyntaxKind::InvalidToken, start, end);

    match first_non_ascii_identifier_character(snapshot, start, end) {
        Some(character) => TokenScan::with_diagnostic(
            token,
            diagnostic::non_ascii_identifier(snapshot, range, character),
        ),
        None => TokenScan::with_diagnostic(token, diagnostic::invalid_identifier(snapshot, range)),
    }
}

fn scan_invalid_scalar_token(snapshot: &SourceSnapshot, start: TextSize) -> TokenScan {
    let character = match first_character(snapshot, start) {
        Some(character) => character,
        None => return TokenScan::clean(SyntaxToken::end_of_file(start)),
    };

    let token = make_scalar_token(SyntaxKind::InvalidToken, start, character);
    let range = token.range();
    let diagnostic = match character {
        '\u{feff}' => diagnostic::misplaced_bom(snapshot, range),
        '\r' => diagnostic::lone_carriage_return(snapshot, range),
        _ => diagnostic::invalid_character(snapshot, range, character),
    };

    TokenScan::with_diagnostic(token, diagnostic)
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

fn first_non_ascii_identifier_character(
    snapshot: &SourceSnapshot,
    start: TextSize,
    end: TextSize,
) -> Option<char> {
    let mut cursor = start;

    while cursor < end {
        let character = first_character(snapshot, cursor)?;

        if is_non_ascii_identifier_character(character) {
            return Some(character);
        }

        cursor = offset_after_character(cursor, character);
    }

    None
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
        "static" => Some(SyntaxKind::StaticKeyword),
        "struct" => Some(SyntaxKind::StructKeyword),
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
