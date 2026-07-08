use bray_syntax::SyntaxKind;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct DelimiterDepth {
    paren_depth: usize,
    bracket_depth: usize,
    brace_depth: usize,
    angle_depth: usize,
}

impl DelimiterDepth {
    pub(super) const fn is_at_root(self) -> bool {
        self.paren_depth == 0
            && self.bracket_depth == 0
            && self.brace_depth == 0
            && self.angle_depth == 0
    }

    pub(super) fn observe_grouping(&mut self, kind: SyntaxKind) {
        match kind {
            SyntaxKind::OpenParenToken => self.paren_depth += 1,
            SyntaxKind::CloseParenToken => self.paren_depth = self.paren_depth.saturating_sub(1),
            SyntaxKind::OpenBracketToken => self.bracket_depth += 1,
            SyntaxKind::CloseBracketToken => {
                self.bracket_depth = self.bracket_depth.saturating_sub(1);
            }
            SyntaxKind::OpenBraceToken => self.brace_depth += 1,
            SyntaxKind::CloseBraceToken => self.brace_depth = self.brace_depth.saturating_sub(1),
            _ => {}
        }
    }

    pub(super) fn observe_grouping_or_angle(&mut self, kind: SyntaxKind) {
        match kind {
            SyntaxKind::LessToken => self.angle_depth += 1,
            SyntaxKind::GreaterToken => self.angle_depth = self.angle_depth.saturating_sub(1),
            _ => self.observe_grouping(kind),
        }
    }
}
