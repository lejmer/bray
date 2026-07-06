use bray_syntax::{PathSyntax, SyntaxKind, SyntaxToken};

use super::state::Parser;

impl Parser {
    pub(super) fn parse_path(&mut self) -> PathSyntax {
        let mut builder = PathSyntax::builder(self.syntax_source());

        builder.push_identifier_token(self.parse_identifier());

        while self.at(SyntaxKind::DotToken) {
            builder.push_dot_token(self.expect(SyntaxKind::DotToken));
            builder.push_identifier_token(self.parse_identifier());
        }

        builder.build()
    }

    pub(super) fn parse_identifier(&mut self) -> SyntaxToken {
        self.expect(SyntaxKind::IdentifierToken)
    }
}
