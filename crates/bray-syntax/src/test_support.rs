use bray_source::{TextRange, TextSize};

use crate::{SyntaxKind, SyntaxToken, SyntaxTrivia};

pub(crate) fn func_keyword_with_trailing_space() -> SyntaxToken {
    func_keyword().with_trailing_trivia([SyntaxTrivia::whitespace(TextRange::new(
        TextSize::new(4),
        TextSize::new(5),
    ))])
}

pub(crate) fn func_keyword() -> SyntaxToken {
    SyntaxToken::new(
        SyntaxKind::FuncKeyword,
        TextRange::new(TextSize::ZERO, TextSize::new(4)),
    )
}
