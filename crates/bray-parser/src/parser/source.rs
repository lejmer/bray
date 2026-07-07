use bray_syntax::{SourceUnitSyntax, SyntaxKind};

use super::state::Parser;

const SOURCE_UNIT_ITEM_TERMINATORS: [SyntaxKind; 1] = [SyntaxKind::EndOfFileToken];

impl Parser {
    pub(super) fn parse_source_unit(&mut self) -> SourceUnitSyntax {
        let mut builder = SourceUnitSyntax::builder(self.syntax_source());

        if self.should_parse_block_module_declaration() {
            self.parse_block_module_declarations(&mut builder);
        } else {
            let declaration = self.parse_source_unit_module_declaration();

            builder.push_source_unit_module_declaration(declaration);
            // TODO(parser): Parse source-unit module items once declarations are implemented.
            self.parse_module_items(&mut builder, &SOURCE_UNIT_ITEM_TERMINATORS);
        }

        builder.push_token(self.expect(SyntaxKind::EndOfFileToken));

        builder.build()
    }
}
