use bray_source::{SourceSnapshot, TextSize};
use bray_syntax::{
    SyntaxCast, SyntaxKind, SyntaxToken, SyntaxWalkControl, SyntaxWalkEvent, SyntaxWalkRoot,
    walk_syntax_node,
};

/// Returns the first source-ordered descendant of the requested typed syntax category.
pub fn first_syntax_descendant<T>(root: &impl SyntaxWalkRoot) -> Option<T>
where
    T: SyntaxCast,
{
    let mut result = None;

    walk_syntax_node(root, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        if node.kind() != T::KIND {
            return SyntaxWalkControl::Continue;
        }

        result = node.cast();

        SyntaxWalkControl::Stop
    });

    result
}

/// Returns all source-ordered descendants of the requested typed syntax category.
pub fn syntax_descendants<T>(root: &impl SyntaxWalkRoot) -> Vec<T>
where
    T: SyntaxCast,
{
    let mut result = Vec::new();

    walk_syntax_node(root, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        if node.kind() == T::KIND
            && let Some(node) = node.cast()
        {
            result.push(node);
        }

        SyntaxWalkControl::Continue
    });

    result
}

/// Asserts that a token stream contains exactly one final EOF token.
pub fn assert_single_final_eof(tokens: &[SyntaxToken], source_text: &str) {
    let eof_count = tokens
        .iter()
        .filter(|token| token.kind() == SyntaxKind::EndOfFileToken)
        .count();

    assert_eq!(eof_count, 1, "expected exactly one EOF in {source_text:?}");

    let eof = match tokens.last() {
        Some(token) => token,
        None => panic!("token stream should contain EOF for {source_text:?}"),
    };

    assert_eq!(eof.kind(), SyntaxKind::EndOfFileToken);
}

/// Asserts that token and trivia ranges reconstruct the full source text.
pub fn assert_tokens_cover_source_text(snapshot: &SourceSnapshot, tokens: &[SyntaxToken]) {
    let mut cursor = TextSize::ZERO;
    let mut reconstructed = String::new();

    for token in tokens {
        let range = token.full_range();

        assert_eq!(
            range.start(),
            cursor,
            "token stream has a gap or overlap in {:?}",
            snapshot.text()
        );

        reconstructed.push_str(&token.full_text(snapshot.text()));

        cursor = range.end();
    }

    assert_eq!(cursor, snapshot.text_len());
    assert_eq!(reconstructed, snapshot.text());
}
