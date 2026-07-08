use bray_syntax::{SyntaxKind, TraitApplicationSyntax};

use super::state::Parser;

impl Parser {
    pub(super) fn parse_trait_application(&mut self) -> TraitApplicationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitApplicationSyntax::builder(self.syntax_source(), start);

        builder.push_path(self.parse_path());

        if self.at(SyntaxKind::LessToken) {
            builder.push_generic_argument_list(self.parse_generic_argument_list());
        }

        builder.build()
    }
}
