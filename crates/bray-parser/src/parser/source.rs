use bray_syntax::{SourceUnitSyntax, SyntaxKind};

use super::state::Parser;

const SOURCE_UNIT_ITEM_TERMINATORS: [SyntaxKind; 1] = [SyntaxKind::EndOfFileToken];

impl Parser {
    pub(super) fn parse_source_unit(&mut self) -> SourceUnitSyntax {
        let mut builder = SourceUnitSyntax::builder(self.syntax_source());

        if !self.should_parse_block_module_declaration() {
            let declaration = self.parse_source_unit_module_declaration();

            builder.push_source_unit_module_declaration(declaration);

            while !self.at(SyntaxKind::EndOfFileToken)
                && !self.should_parse_block_module_declaration()
            {
                self.parse_module_item(&mut builder, &SOURCE_UNIT_ITEM_TERMINATORS);
            }
        }

        self.parse_block_module_declarations(&mut builder);
        builder.push_token(self.expect(SyntaxKind::EndOfFileToken));

        builder.build()
    }
}
