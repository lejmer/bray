use bray_syntax::SyntaxKind;

use crate::parser::delimiter::DelimiterDepth;
use crate::parser::state::Parser;

use super::grammar::at_primary_hard_boundary;
use super::grammar::{
    ARGUMENT_LIST_TERMINATORS, ARRAY_EXPRESSION_TERMINATORS, EXPRESSION_START_KINDS,
    SLICE_END_TERMINATORS, SLICE_SELECTOR_TERMINATORS, STRUCT_CONSTRUCTION_BODY_TERMINATORS,
};
use super::operator::at_infix_operator;

impl Parser {
    pub(in crate::parser::expression) fn should_parse_explicit_generic_call(&mut self) -> bool {
        if !self.at(SyntaxKind::LessToken) {
            return false;
        }

        self.scan_ahead(|scan| {
            scan.parse_generic_argument_list();

            scan.at(SyntaxKind::OpenParenToken)
        })
    }

    pub(in crate::parser::expression) fn should_parse_explicit_generic_application(
        &mut self,
    ) -> bool {
        if !self.at(SyntaxKind::LessToken) {
            return false;
        }

        self.scan_ahead(|scan| !scan.parse_generic_argument_list().is_recovered())
    }

    pub(in crate::parser::expression) fn should_parse_slice_index_operation(&mut self) -> bool {
        self.scan_ahead(Parser::scan_bracket_has_top_level_dot_dot)
    }

    fn scan_bracket_has_top_level_dot_dot(&mut self) -> bool {
        if !self.at(SyntaxKind::OpenBracketToken) {
            return false;
        }

        self.consume();

        let mut depth = DelimiterDepth::default();

        while !self.at(SyntaxKind::EndOfFileToken) {
            let kind = self.peek().kind();

            if depth.is_at_root() && kind == SyntaxKind::DotDotToken {
                return true;
            }

            if depth.is_at_root() && kind == SyntaxKind::CloseBracketToken {
                return false;
            }

            depth.observe_grouping(kind);
            self.consume();
        }

        false
    }

    pub(in crate::parser::expression) fn should_parse_trait_qualified_member_operation(
        &mut self,
    ) -> bool {
        self.scan_ahead(Parser::scan_trait_qualified_member_operation)
    }

    fn scan_trait_qualified_member_operation(&mut self) -> bool {
        if !self.at(SyntaxKind::OpenParenToken) {
            return false;
        }

        self.consume();

        if !self.at(SyntaxKind::IdentifierToken) {
            return false;
        }

        let mut depth = DelimiterDepth::default();

        while !self.at(SyntaxKind::EndOfFileToken) {
            let kind = self.peek().kind();

            if depth.is_at_root() && kind == SyntaxKind::CloseParenToken {
                self.consume();

                return self.at(SyntaxKind::DotToken);
            }

            depth.observe_grouping(kind);
            self.consume();
        }

        false
    }

    pub(in crate::parser::expression) fn should_parse_expected_type_struct_construction(
        &mut self,
    ) -> bool {
        self.at(SyntaxKind::OpenBraceToken)
            && self.lookahead(1).kind() == SyntaxKind::IdentifierToken
            && self.lookahead(2).kind() == SyntaxKind::EqualsToken
    }

    pub(in crate::parser::expression) fn should_parse_general_generator_expression(
        &mut self,
    ) -> bool {
        self.at(SyntaxKind::OpenBraceToken) && self.lookahead(1).kind() == SyntaxKind::EachKeyword
    }

    pub(in crate::parser::expression) fn at_named_argument_start(&mut self) -> bool {
        self.at(SyntaxKind::IdentifierToken) && self.lookahead(1).kind() == SyntaxKind::EqualsToken
    }

    pub(in crate::parser::expression) fn at_parenthesized_expression_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::CommaToken) || at_boundary(self)
    }

    pub(in crate::parser::expression) fn at_parenthesized_tuple_missing_separator(
        &mut self,
    ) -> bool {
        EXPRESSION_START_KINDS.contains(&self.peek().kind())
    }

    pub(in crate::parser::expression) fn at_tuple_expression_element_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken)
            || self.at(SyntaxKind::CloseParenToken)
            || self.at(SyntaxKind::EndOfFileToken)
    }

    pub(in crate::parser::expression) fn at_array_expression_element_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CommaToken)
            || self.at(SyntaxKind::SemicolonToken)
            || self.at_any(&ARRAY_EXPRESSION_TERMINATORS)
            || at_boundary(self)
    }

    pub(in crate::parser::expression) fn at_array_element_missing_separator(&mut self) -> bool {
        EXPRESSION_START_KINDS.contains(&self.peek().kind())
    }

    pub(in crate::parser::expression) fn at_repeated_array_count_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CloseBracketToken) || self.at(SyntaxKind::EndOfFileToken)
    }

    pub(in crate::parser::expression) fn at_element_index_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CloseBracketToken) || at_boundary(self)
    }

    pub(in crate::parser::expression) fn at_argument_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at_any(&ARGUMENT_LIST_TERMINATORS)
    }

    pub(in crate::parser::expression) fn at_slice_selector_start_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at_any(&SLICE_SELECTOR_TERMINATORS) || at_boundary(self)
    }

    pub(in crate::parser::expression) fn at_slice_selector_end_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at_any(&SLICE_END_TERMINATORS) || at_boundary(self)
    }

    pub(in crate::parser::expression) fn at_conversion_type_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        let kind = self.peek().kind();

        at_boundary(self)
            || at_primary_hard_boundary(kind)
            || kind == SyntaxKind::CommaToken
            || at_infix_operator(kind)
    }

    pub(in crate::parser::expression) fn at_struct_field_initializer_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at_any(&STRUCT_CONSTRUCTION_BODY_TERMINATORS)
    }

    pub(in crate::parser::expression) fn at_expression_before_block_boundary(&mut self) -> bool {
        self.at(SyntaxKind::OpenBraceToken) || self.at(SyntaxKind::EndOfFileToken)
    }

    pub(in crate::parser::expression) fn at_expression_before_block_recovery_boundary(
        &mut self,
    ) -> bool {
        let kind = self.peek().kind();

        self.at_expression_before_block_boundary()
            || at_primary_hard_boundary(kind)
            || matches!(
                kind,
                SyntaxKind::CaseKeyword | SyntaxKind::ElseKeyword | SyntaxKind::WhenKeyword
            )
    }

    pub(in crate::parser::expression) fn at_flow_block_missing_boundary(&mut self) -> bool {
        let kind = self.peek().kind();

        EXPRESSION_START_KINDS.contains(&kind)
            || at_primary_hard_boundary(kind)
            || matches!(
                kind,
                SyntaxKind::CaseKeyword | SyntaxKind::ElseKeyword | SyntaxKind::WhenKeyword
            )
    }

    pub(in crate::parser::expression) fn at_expression_operand_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        at_boundary(self) || at_primary_hard_boundary(self.peek().kind())
    }

    pub(in crate::parser::expression) fn at_match_arm_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CaseKeyword)
            || self.at(SyntaxKind::CloseBraceToken)
            || self.at(SyntaxKind::EndOfFileToken)
    }

    pub(in crate::parser::expression) fn at_match_arm_pattern_boundary(&mut self) -> bool {
        self.at(SyntaxKind::WhenKeyword)
            || self.at(SyntaxKind::OpenBraceToken)
            || self.at_match_arm_boundary()
    }

    pub(in crate::parser::expression) fn at_match_arm_guard_boundary(&mut self) -> bool {
        self.at(SyntaxKind::OpenBraceToken) || self.at_match_arm_boundary()
    }

    pub(in crate::parser::expression) fn at_iteration_pattern_boundary(&mut self) -> bool {
        self.at(SyntaxKind::InKeyword)
            || self.at(SyntaxKind::OpenBraceToken)
            || self.at(SyntaxKind::EndOfFileToken)
    }

    pub(in crate::parser::expression) fn at_primary_tail_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        let kind = self.peek().kind();

        at_boundary(self)
            || at_primary_hard_boundary(kind)
            || kind == SyntaxKind::EqualsToken
            || at_infix_operator(kind)
    }
}
