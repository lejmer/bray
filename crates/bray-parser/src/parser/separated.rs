use bray_syntax::{SyntaxKind, SyntaxToken};

#[cfg(test)]
use bray_syntax::{IdentifierListItemSyntax, IdentifierListSyntax, IdentifierListSyntaxBuilder};

use super::recovery::RecoverySyntaxSink;
use super::state::Parser;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SeparatedListSpec<'kinds> {
    pub(super) separator_kind: SyntaxKind,
    pub(super) terminators: &'kinds [SyntaxKind],
    pub(super) recovery_kinds: &'kinds [SyntaxKind],
    pub(super) allow_trailing_separator: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SeparatedListPosition {
    Item { allow_end: bool },
    Separator,
}

pub(super) trait SeparatedListSyntaxSink<Item>: RecoverySyntaxSink {
    fn push_item(&mut self, item: Item);

    fn push_separator(&mut self, separator: SyntaxToken);
}

pub(super) fn separated_list_recovery_kinds(
    item_start_kinds: &[SyntaxKind],
    separator_kind: SyntaxKind,
    terminators: &[SyntaxKind],
) -> Vec<SyntaxKind> {
    let mut recovery_kinds = Vec::with_capacity(item_start_kinds.len() + 1 + terminators.len());

    recovery_kinds.extend_from_slice(item_start_kinds);
    recovery_kinds.push(separator_kind);
    recovery_kinds.extend_from_slice(terminators);

    recovery_kinds
}

impl Parser {
    fn at_list_end(&mut self, terminators: &[SyntaxKind]) -> bool {
        self.at_any(terminators) || self.at(SyntaxKind::EndOfFileToken)
    }

    #[cfg(test)]
    pub(super) fn parse_identifier_list(
        &mut self,
        terminators: &[SyntaxKind],
    ) -> IdentifierListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &[SyntaxKind::IdentifierToken],
            SyntaxKind::CommaToken,
            terminators,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = IdentifierListSyntax::builder(self.syntax_source(), start);

        self.parse_separated_list(&mut builder, spec, Parser::parse_identifier_list_item);

        builder.build()
    }

    #[cfg(test)]
    fn parse_identifier_list_item(&mut self) -> IdentifierListItemSyntax {
        let mut builder = IdentifierListItemSyntax::builder(self.syntax_source());

        builder.push_identifier_token(self.parse_identifier());

        builder.build()
    }

    pub(super) fn parse_separated_list<Item>(
        &mut self,
        builder: &mut impl SeparatedListSyntaxSink<Item>,
        spec: SeparatedListSpec<'_>,
        mut parse_item: impl FnMut(&mut Parser) -> Item,
    ) {
        self.parse_separated_list_until(
            builder,
            spec,
            |parser| parser.at_list_end(spec.terminators),
            &mut parse_item,
        );
    }

    pub(super) fn parse_separated_list_until<Item>(
        &mut self,
        builder: &mut impl SeparatedListSyntaxSink<Item>,
        spec: SeparatedListSpec<'_>,
        mut at_end: impl FnMut(&mut Parser) -> bool,
        mut parse_item: impl FnMut(&mut Parser) -> Item,
    ) {
        let mut position = SeparatedListPosition::Item { allow_end: true };

        loop {
            let at_end_now = at_end(self);

            match position {
                SeparatedListPosition::Item { allow_end } if allow_end && at_end_now => {
                    return;
                }
                SeparatedListPosition::Separator if at_end_now => {
                    return;
                }
                _ => {}
            }

            match position {
                SeparatedListPosition::Item { .. } => {
                    self.parse_separated_list_item_until(
                        builder,
                        spec,
                        &mut at_end,
                        &mut parse_item,
                    );

                    position = SeparatedListPosition::Separator;
                }
                SeparatedListPosition::Separator => {
                    if let Some(separator) = self.consume_if(spec.separator_kind) {
                        builder.push_separator(separator);
                        position = SeparatedListPosition::Item {
                            allow_end: spec.allow_trailing_separator,
                        };
                        continue;
                    }

                    builder.push_separator(self.expect(spec.separator_kind));
                    position = SeparatedListPosition::Item { allow_end: false };
                }
            }
        }
    }

    fn parse_separated_list_item_until<Item>(
        &mut self,
        builder: &mut impl SeparatedListSyntaxSink<Item>,
        spec: SeparatedListSpec<'_>,
        at_end: &mut impl FnMut(&mut Parser) -> bool,
        parse_item: &mut impl FnMut(&mut Parser) -> Item,
    ) {
        let item_start = self.peek().start();
        let item = parse_item(self);

        builder.push_item(item);

        if self.peek().start() != item_start || self.at_list_boundary_until(spec, at_end) {
            return;
        }

        if self.recover_until(builder, spec.recovery_kinds)
            || self.at_list_boundary_until(spec, at_end)
        {
            return;
        }

        self.recover_current_token(builder);
    }

    fn at_list_boundary_until(
        &mut self,
        spec: SeparatedListSpec<'_>,
        at_end: &mut impl FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(spec.separator_kind) || at_end(self)
    }
}

#[cfg(test)]
impl SeparatedListSyntaxSink<IdentifierListItemSyntax> for IdentifierListSyntaxBuilder {
    fn push_item(&mut self, item: IdentifierListItemSyntax) {
        IdentifierListSyntaxBuilder::push_item(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        IdentifierListSyntaxBuilder::push_separator_token(self, separator);
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use super::super::state::Parser;
    use crate::test_support::{diagnostic_kinds, source, token_kinds};

    #[test]
    fn parser_separated_list_helper_parses_valid_identifier_lists() {
        let sources = source_store(["a,b,"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot.clone());

        let list = parser.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();

        assert_eq!(list.full_text(), "a,b,");
        assert_eq!(list.items().count(), 2);

        assert_eq!(
            token_kinds(list.tokens()),
            [
                SyntaxKind::IdentifierToken,
                SyntaxKind::CommaToken,
                SyntaxKind::IdentifierToken,
                SyntaxKind::CommaToken
            ]
        );

        assert_eq!(list.separator_tokens().count(), 2);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_separated_list_helper_represents_missing_separators() {
        let sources = source_store(["a b"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot.clone());

        let list = parser.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let separators = list.separator_tokens().collect::<Vec<_>>();

        let [separator] = separators.as_slice() else {
            panic!("expected one separator token: {separators:?}");
        };

        assert_eq!(list.full_text(), "a b");
        assert_eq!(list.items().count(), 2);
        assert_eq!(separator.kind(), SyntaxKind::CommaToken);
        assert!(separator.is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_separated_list_helper_recovers_bad_tokens_without_losing_later_items() {
        let sources = source_store(["a,$,b"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot.clone());

        let list = parser.parse_identifier_list(&[SyntaxKind::EndOfFileToken]);
        let diagnostics = parser.finish();
        let items = list.items().collect::<Vec<_>>();
        let skipped_syntax = list.skipped_syntax().collect::<Vec<_>>();

        let [first, recovered, last] = items.as_slice() else {
            panic!("expected three list items: {items:?}");
        };

        let [skipped] = skipped_syntax.as_slice() else {
            panic!("expected one skipped-syntax node: {skipped_syntax:?}");
        };

        assert_eq!(list.full_text(), "a,$,b");
        assert_eq!(first.identifier_token().text(snapshot.text()), Some("a"));
        assert!(recovered.identifier_token().is_missing());
        assert_eq!(last.identifier_token().text(snapshot.text()), Some("b"));
        assert_eq!(skipped.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }
}
