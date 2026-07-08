use bray_syntax::{SyntaxKind, TypeAnnotationSyntax, TypedIdentifierSyntax};

use super::state::Parser;

impl Parser {
    pub(super) fn parse_typed_identifier_until(
        &mut self,
        at_type_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypedIdentifierSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypedIdentifierSyntax::builder(self.syntax_source(), start);

        builder.push_identifier_token(self.parse_identifier());
        builder.push_type_annotation(self.parse_type_annotation_until(at_type_boundary));

        builder.build()
    }

    fn parse_type_annotation_until(
        &mut self,
        at_type_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TypeAnnotationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TypeAnnotationSyntax::builder(self.syntax_source(), start);

        builder.push_colon_token(self.expect(SyntaxKind::ColonToken));
        builder.push_type_expression(self.parse_type_expression_until(at_type_boundary));

        builder.build()
    }
}
