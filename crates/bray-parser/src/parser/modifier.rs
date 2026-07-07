use bray_syntax::{SyntaxKind, SyntaxToken};

use super::state::Parser;

impl Parser {
    pub(super) fn at_visibility_modifier(&mut self) -> bool {
        self.peek().kind().is_visibility_modifier()
    }

    pub(super) fn parse_visibility_modifier(&mut self) -> SyntaxToken {
        if self.at(SyntaxKind::PublicKeyword) {
            return self.expect(SyntaxKind::PublicKeyword);
        }

        self.expect(SyntaxKind::InternalKeyword)
    }
}
