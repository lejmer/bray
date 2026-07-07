use bray_syntax::{
    AbiDirectiveSyntax, CopyDirectiveSyntax, DirectiveArgumentListSyntax,
    EntrypointDirectiveSyntax, LayoutDirectiveSyntax, LinkDirectiveSyntax, SymbolDirectiveSyntax,
    SyntaxKind, TargetDirectiveSyntax, TestDirectiveSyntax,
};

use super::state::Parser;

pub(super) const ABI_DIRECTIVE_NAME: &str = "abi";
pub(super) const COPY_DIRECTIVE_NAME: &str = "copy";
pub(super) const ENTRYPOINT_DIRECTIVE_NAME: &str = "entrypoint";
pub(super) const LAYOUT_DIRECTIVE_NAME: &str = "layout";
pub(super) const LINK_DIRECTIVE_NAME: &str = "link";
pub(super) const SYMBOL_DIRECTIVE_NAME: &str = "symbol";
pub(super) const TARGET_DIRECTIVE_NAME: &str = "target";
pub(super) const TEST_DIRECTIVE_NAME: &str = "test";

impl Parser {
    pub(super) fn parse_target_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> TargetDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TargetDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        // TODO(parser): Parse target directive arguments once directive arguments are implemented.
        self.recover_until_balanced_close_paren(&mut builder, argument_recovery_kinds);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    pub(super) fn parse_test_directive(&mut self) -> TestDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TestDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    pub(super) fn parse_entrypoint_directive(&mut self) -> EntrypointDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = EntrypointDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    pub(super) fn parse_copy_directive(&mut self) -> CopyDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CopyDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    pub(super) fn parse_layout_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> LayoutDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LayoutDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));
        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_link_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> LinkDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = LinkDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));
        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_abi_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> AbiDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AbiDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));
        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_symbol_directive(
        &mut self,
        argument_recovery_kinds: &[SyntaxKind],
    ) -> SymbolDirectiveSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SymbolDirectiveSyntax::builder(self.syntax_source(), start);

        builder.push_directive_marker_token(self.expect(SyntaxKind::AtToken));
        builder.push_name_token(self.expect(SyntaxKind::IdentifierToken));
        builder.push_directive_argument_list(
            self.parse_directive_argument_list(argument_recovery_kinds),
        );

        builder.build()
    }

    pub(super) fn parse_directive_argument_list(
        &mut self,
        recovery_kinds: &[SyntaxKind],
    ) -> DirectiveArgumentListSyntax {
        let start = self.peek().full_range().start();
        let mut builder = DirectiveArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        // TODO(parser): Parse directive argument items once directive arguments are implemented.
        self.recover_until_balanced_close_paren(&mut builder, recovery_kinds);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    pub(super) fn at_directive_name(&mut self, name: &str) -> bool {
        if !self.at(SyntaxKind::AtToken) {
            return false;
        }

        let name_token = self.lookahead(1);

        name_token.kind() == SyntaxKind::IdentifierToken
            && self.token_text(&name_token) == Some(name)
    }

    pub(super) fn consume_directive_argument_list_for_scan(&mut self, stop_kinds: &[SyntaxKind]) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_none() {
            return;
        }

        self.scan_until_balanced_close_paren(stop_kinds);
        self.consume_if(SyntaxKind::CloseParenToken);
    }

    pub(super) fn skip_unknown_directive_for_scan(&mut self, stop_kinds: &[SyntaxKind]) {
        if self.consume_if(SyntaxKind::OpenParenToken).is_some() {
            self.scan_until_balanced_close_paren(stop_kinds);
            self.consume_if(SyntaxKind::CloseParenToken);

            return;
        }

        while !self.at_any(stop_kinds) && !self.at(SyntaxKind::EndOfFileToken) {
            self.consume();
        }
    }
}
