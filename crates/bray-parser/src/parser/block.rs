use bray_syntax::{
    BlockExpressionSyntax, BlockExpressionSyntaxBuilder, BlockItemSyntax, BlockItemSyntaxBuilder,
    LocalBindingDeclarationSyntax, SequencedExpressionSyntax, SyntaxKind,
};

use super::expression::EXPRESSION_START_KINDS;
use super::state::Parser;

const LOCAL_BINDING_PATTERN_BOUNDARY_KINDS: [SyntaxKind; 5] = [
    SyntaxKind::ColonToken,
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const LOCAL_BINDING_TYPE_BOUNDARY_KINDS: [SyntaxKind; 4] = [
    SyntaxKind::EqualsToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const BLOCK_ITEM_END_KINDS: [SyntaxKind; 3] = [
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

impl Parser {
    pub(super) fn parse_block_expression_until(
        &mut self,
        at_missing_open_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> BlockExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BlockExpressionSyntax::builder(self.syntax_source(), start);

        self.parse_braced_body_contents(
            &mut builder,
            at_missing_open_boundary,
            |parser, builder| {
                parser.parse_block_items(builder);
            },
        );

        builder.build()
    }

    fn parse_block_items(&mut self, builder: &mut BlockExpressionSyntaxBuilder) {
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFileToken) {
            builder.push_block_item(self.parse_block_item());
        }
    }

    fn parse_block_item(&mut self) -> BlockItemSyntax {
        let start = self.peek().full_range().start();
        let mut builder = BlockItemSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::LetKeyword) {
            builder.push_local_binding_declaration(self.parse_local_binding_declaration());
        } else if self.at_constant_declaration_start() {
            builder.push_constant_declaration(self.parse_constant_declaration());
        } else if self.at(SyntaxKind::EachKeyword) {
            // TODO(parser): Parse generator-iteration items once generators are implemented.
            self.recover_unknown_block_item(&mut builder);
        } else if self.at_block_expression_item_start() {
            builder.push_sequenced_expression(self.parse_sequenced_expression());
        } else {
            self.recover_unknown_block_item(&mut builder);
        }

        builder.build()
    }

    fn parse_local_binding_declaration(&mut self) -> LocalBindingDeclarationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LocalBindingDeclarationSyntax::builder(self.syntax_source(), start);

        builder.push_let_keyword(self.expect(SyntaxKind::LetKeyword));

        let mut at_pattern_boundary = Parser::at_local_binding_pattern_boundary;

        builder.push_irrefutable_pattern(
            self.parse_irrefutable_pattern_until(&mut at_pattern_boundary),
        );

        if self.at(SyntaxKind::ColonToken) {
            let mut at_type_boundary = Parser::at_local_binding_type_boundary;

            builder.push_type_annotation(self.parse_type_annotation_until(&mut at_type_boundary));
        }

        let equals_token = self.expect(SyntaxKind::EqualsToken);
        let equals_missing = equals_token.is_missing();

        builder.push_equals_token(equals_token);

        if !(equals_missing && self.at_block_item_end_or_following_declaration()) {
            let mut at_value_boundary = Parser::at_block_item_end_or_following_declaration;

            builder.push_expression(self.parse_expression_until(&mut at_value_boundary));
        }

        self.recover_until_predicate(&mut builder, |parser| {
            parser.at_block_item_end_or_following_declaration()
        });

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn parse_sequenced_expression(&mut self) -> SequencedExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SequencedExpressionSyntax::builder(self.syntax_source(), start);
        let mut at_expression_boundary = Parser::at_block_item_end_or_following_declaration;

        builder.push_expression(self.parse_expression_until(&mut at_expression_boundary));

        self.recover_until_predicate(&mut builder, |parser| {
            parser.at_block_item_end_or_following_declaration()
        });

        builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

        builder.build()
    }

    fn recover_unknown_block_item(&mut self, builder: &mut BlockItemSyntaxBuilder) {
        let mut skipped_tokens = Vec::new();

        if !self.at(SyntaxKind::EndOfFileToken) {
            skipped_tokens.push(self.consume());
        }

        while !self.at_unknown_block_item_boundary() {
            skipped_tokens.push(self.consume());
        }

        if self.at(SyntaxKind::SemicolonToken) {
            skipped_tokens.push(self.consume());
        }

        self.record_skipped_syntax_for_tokens(&skipped_tokens);
        builder.push_skipped_tokens(skipped_tokens);
    }

    fn at_local_binding_pattern_boundary(&mut self) -> bool {
        self.at_any(&LOCAL_BINDING_PATTERN_BOUNDARY_KINDS)
            || self.at_following_block_declaration_start()
    }

    fn at_local_binding_type_boundary(&mut self) -> bool {
        self.at_any(&LOCAL_BINDING_TYPE_BOUNDARY_KINDS)
            || self.at_following_block_declaration_start()
    }

    fn at_block_item_end_or_following_declaration(&mut self) -> bool {
        self.at_any(&BLOCK_ITEM_END_KINDS) || self.at_following_block_declaration_start()
    }

    fn at_unknown_block_item_boundary(&mut self) -> bool {
        self.at_any(&BLOCK_ITEM_END_KINDS) || self.at_following_block_declaration_start()
    }

    fn at_following_block_declaration_start(&mut self) -> bool {
        self.at(SyntaxKind::LetKeyword)
            || self.at(SyntaxKind::EachKeyword)
            || self.at_constant_declaration_start()
    }

    fn at_block_expression_item_start(&mut self) -> bool {
        self.at_any(&EXPRESSION_START_KINDS)
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::TextRange;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::test_support::{diagnostic_kinds, marker_offset, source};

    use super::super::state::Parser;

    #[test]
    fn parser_parses_block_expression_items() {
        let source_text = "{ let value: Int = 1; const Answer: Int = 42; value + Answer; }";

        let sources = source_store([source_text]);
        let snapshot = source(&sources, 0);
        let mut parser = Parser::new(snapshot);

        let block = parser
            .parse_block_expression_until(&mut |parser| parser.at(SyntaxKind::EndOfFileToken));

        let diagnostics = parser.finish();
        let items = block.block_items().collect::<Vec<_>>();

        let [local_item, constant_item, sequence_item] = items.as_slice() else {
            panic!("expected local, constant, and sequenced block items: {items:?}");
        };

        let local = match local_item.local_binding_declaration() {
            Some(local) => local,
            None => panic!("expected local binding declaration"),
        };

        let sequence = match sequence_item.sequenced_expression() {
            Some(sequence) => sequence,
            None => panic!("expected sequenced expression"),
        };

        assert_eq!(block.full_text(), source_text);
        assert!(local.type_annotation().is_some());

        assert_eq!(
            local.expression().map(|expression| expression.full_text()),
            Some(String::from("1"))
        );

        assert!(constant_item.constant_declaration().is_some());

        assert_eq!(
            sequence
                .expression()
                .map(|expression| expression.full_text()),
            Some(String::from("value + Answer"))
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_reports_missing_local_binding_semicolon_before_following_const() {
        let source_text = "{ let value = 1\nconst Answer: Int = 42; }";

        let sources = source_store([source_text]);
        let snapshot = source(&sources, 0);
        let insertion = marker_offset(source_text, "const");

        let mut parser = Parser::new(snapshot);

        let block = parser
            .parse_block_expression_until(&mut |parser| parser.at(SyntaxKind::EndOfFileToken));

        let diagnostics = parser.finish();
        let items = block.block_items().collect::<Vec<_>>();

        let [local_item, constant_item] = items.as_slice() else {
            panic!("expected local and constant block items: {items:?}");
        };

        let local = match local_item.local_binding_declaration() {
            Some(local) => local,
            None => panic!("expected local binding declaration"),
        };

        let semicolon_token = local.semicolon_token();

        assert_eq!(block.full_text(), source_text);
        assert!(constant_item.constant_declaration().is_some());
        assert!(semicolon_token.is_missing());
        assert_eq!(semicolon_token.range(), TextRange::empty(insertion));

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_recovers_bad_block_items_without_losing_later_items() {
        let source_text = "{ $; value; }";

        let sources = source_store([source_text]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);

        let block = parser
            .parse_block_expression_until(&mut |parser| parser.at(SyntaxKind::EndOfFileToken));

        let diagnostics = parser.finish();
        let items = block.block_items().collect::<Vec<_>>();

        let [bad_item, sequence_item] = items.as_slice() else {
            panic!("expected recovered and sequenced block items: {items:?}");
        };

        let skipped_syntax = bad_item.skipped_syntax().collect::<Vec<_>>();

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected skipped block item syntax: {skipped_syntax:?}");
        };

        assert_eq!(block.full_text(), source_text);
        assert_eq!(skipped.full_text(), "$; ");
        assert!(sequence_item.sequenced_expression().is_some());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }
}
