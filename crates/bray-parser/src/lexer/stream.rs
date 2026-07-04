use bray_diagnostics::DiagnosticBag;
use bray_source::SourceSnapshot;
use bray_syntax::SyntaxToken;

use super::LexerTokenSource;

/// Result of lexing one source unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LexResult {
    tokens: Vec<SyntaxToken>,
    diagnostics: DiagnosticBag,
}

impl LexResult {
    fn new(tokens: Vec<SyntaxToken>, diagnostics: DiagnosticBag) -> Self {
        Self {
            tokens,
            diagnostics,
        }
    }

    /// Returns lexed tokens in source order, including EOF.
    pub fn tokens(&self) -> &[SyntaxToken] {
        &self.tokens
    }

    /// Returns lexical diagnostics produced while scanning the source unit.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns tokens plus diagnostics.
    pub fn into_parts(self) -> (Vec<SyntaxToken>, DiagnosticBag) {
        (self.tokens, self.diagnostics)
    }
}

/// Lexes one source snapshot into a token stream and lexical diagnostics.
pub fn lex_source_unit(snapshot: &SourceSnapshot) -> LexResult {
    // LexerTokenSource owns its snapshot. Cloning shares immutable source text.
    let mut token_source = LexerTokenSource::new(snapshot.clone());

    let tokens = drain_tokens(&mut token_source);

    LexResult::new(tokens, token_source.into_diagnostics())
}

fn drain_tokens(token_source: &mut LexerTokenSource) -> Vec<SyntaxToken> {
    let mut tokens = Vec::new();

    loop {
        let token = token_source.consume();
        let reached_end = token.is_end_of_file();

        tokens.push(token);

        if reached_end {
            return tokens;
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
    use bray_syntax::SyntaxKind;

    use super::lex_source_unit;

    #[test]
    fn lex_source_unit_drains_tokens_through_eof() {
        let result = lex_source_unit(&snapshot("func main"));

        let kinds = result
            .tokens()
            .iter()
            .map(|token| token.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                SyntaxKind::FuncKeyword,
                SyntaxKind::IdentifierToken,
                SyntaxKind::EndOfFileToken
            ]
        );
    }

    #[test]
    fn lex_source_unit_returns_lexical_diagnostics() {
        let result = lex_source_unit(&snapshot("$"));

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::LexicalInvalidCharacter)
                .count(),
            1
        );
    }

    fn snapshot(text: &str) -> SourceSnapshot {
        match SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("lexer-stream-test"),
            SourceVersion::new(0),
            text,
        ) {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test source should fit in TextSize: {error:?}"),
        }
    }
}
