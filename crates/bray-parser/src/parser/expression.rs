use bray_syntax::{
    AbsenceExpressionSyntax, AccessExpressionSyntax, ArgumentListSyntax, ArgumentListSyntaxBuilder,
    ArgumentSyntax, ArrayExpressionSyntax, ArrayExpressionSyntaxBuilder, CallOperationSyntax,
    ConversionOperationSyntax, ElementIndexOperationSyntax, ExpressionSyntax,
    GroupedExpressionSyntax, LeadingDotVariantExpressionSyntax, LiteralExpressionSyntax,
    MemberAccessOperationSyntax, NullablePropagationOperationSyntax, PrimaryExpressionSyntax,
    SliceIndexOperationSyntax, StructConstructionBodySyntax, StructConstructionBodySyntaxBuilder,
    StructFieldInitializerSyntax, SyntaxKind, SyntaxToken, TraitQualifiedMemberOperationSyntax,
    TupleExpressionSyntax, TupleExpressionSyntaxBuilder, UnitExpressionSyntax,
};

use super::delimiter::DelimiterDepth;
use super::separated::{SeparatedListSpec, SeparatedListSyntaxSink, separated_list_recovery_kinds};
use super::state::Parser;

const PREFIX_RIGHT_BINDING_POWER: u8 = 20;

pub(super) const EXPRESSION_START_KINDS: [SyntaxKind; 43] = [
    SyntaxKind::AllKeyword,
    SyntaxKind::AmpersandToken,
    SyntaxKind::AnyKeyword,
    SyntaxKind::AssertKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::AwaitKeyword,
    SyntaxKind::BangToken,
    SyntaxKind::BinaryIntegerLiteralToken,
    SyntaxKind::BreakKeyword,
    SyntaxKind::CatchKeyword,
    SyntaxKind::CharacterLiteralToken,
    SyntaxKind::ContinueKeyword,
    SyntaxKind::DecimalIntegerLiteralToken,
    SyntaxKind::DotToken,
    SyntaxKind::FalseKeyword,
    SyntaxKind::ForKeyword,
    SyntaxKind::HexadecimalIntegerLiteralToken,
    SyntaxKind::IdentifierToken,
    SyntaxKind::IfKeyword,
    SyntaxKind::ImaginaryLiteralToken,
    SyntaxKind::LambdaKeyword,
    SyntaxKind::LoopKeyword,
    SyntaxKind::MatchKeyword,
    SyntaxKind::MinusToken,
    SyntaxKind::NoneKeyword,
    SyntaxKind::OpenBraceToken,
    SyntaxKind::OpenBracketToken,
    SyntaxKind::OpenParenToken,
    SyntaxKind::PanicKeyword,
    SyntaxKind::RealLiteralToken,
    SyntaxKind::ReturnKeyword,
    SyntaxKind::SelfValueKeyword,
    SyntaxKind::SpawnKeyword,
    SyntaxKind::StringLiteralToken,
    SyntaxKind::TildeToken,
    SyntaxKind::TrueKeyword,
    SyntaxKind::TrustedKeyword,
    SyntaxKind::TryKeyword,
    SyntaxKind::TupleElementIndexToken,
    SyntaxKind::UnitKeyword,
    SyntaxKind::WhileKeyword,
    SyntaxKind::WithKeyword,
    SyntaxKind::YieldKeyword,
];

const ARGUMENT_LIST_TERMINATORS: [SyntaxKind; 5] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const STRUCT_FIELD_INITIALIZER_START_KINDS: [SyntaxKind; 1] = [SyntaxKind::IdentifierToken];

const STRUCT_CONSTRUCTION_BODY_TERMINATORS: [SyntaxKind; 5] = [
    SyntaxKind::CloseBraceToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::EndOfFileToken,
];

const ARRAY_EXPRESSION_TERMINATORS: [SyntaxKind; 4] = [
    SyntaxKind::CloseBracketToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const SLICE_SELECTOR_TERMINATORS: [SyntaxKind; 4] = [
    SyntaxKind::DotDotToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

const SLICE_END_TERMINATORS: [SyntaxKind; 3] = [
    SyntaxKind::CloseBracketToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperatorAssociativity {
    Left,
    Right,
    None,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InfixOperator {
    left_binding_power: u8,
    right_binding_power: u8,
    associativity: OperatorAssociativity,
}

impl Parser {
    pub(super) fn parse_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        self.parse_assignment_expression_until(at_boundary)
    }

    pub(super) fn parse_non_assignment_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        self.parse_expression_with_min_binding_power_until(at_boundary, 0)
    }

    fn parse_assignment_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let left = self.parse_non_assignment_expression_until(at_boundary);

        if at_boundary(self) || !self.at(SyntaxKind::EqualsToken) {
            return left;
        }

        let start = left.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(left);
        builder.push_operator_token(self.expect(SyntaxKind::EqualsToken));
        builder.push_expression(self.parse_expression_until(at_boundary));

        builder.build()
    }

    fn parse_expression_with_min_binding_power_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
        min_binding_power: u8,
    ) -> ExpressionSyntax {
        let mut expression = self.parse_prefix_expression_until(at_boundary);

        loop {
            if at_boundary(self) {
                break;
            }

            let Some(operator) = infix_operator(self.peek().kind()) else {
                break;
            };

            if operator.left_binding_power < min_binding_power {
                break;
            }

            let start = expression.full_range().start();
            let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

            builder.push_expression(expression);
            builder.push_operator_token(self.consume());
            builder.push_expression(self.parse_expression_with_min_binding_power_until(
                at_boundary,
                operator.right_binding_power,
            ));

            expression = builder.build();

            if operator.associativity == OperatorAssociativity::None {
                break;
            }
        }

        expression
    }

    fn parse_prefix_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        if at_boundary(self) {
            return self.missing_expression();
        }

        if !at_unary_operator(self.peek().kind()) {
            return self.parse_postfix_expression_until(at_boundary);
        }

        let start = self.peek().full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_operator_token(self.consume());
        builder.push_expression(self.parse_expression_with_min_binding_power_until(
            at_boundary,
            PREFIX_RIGHT_BINDING_POWER,
        ));

        builder.build()
    }

    fn parse_postfix_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let primary = self.parse_primary_expression_until(at_boundary);
        let mut expression = self.primary_to_expression(primary);

        loop {
            if at_boundary(self) {
                break;
            }

            expression = match self.peek().kind() {
                SyntaxKind::DotToken => self.parse_member_access_postfix(expression),
                SyntaxKind::OpenBracketToken if self.should_parse_slice_index_operation() => {
                    self.parse_slice_index_postfix(expression, at_boundary)
                }
                SyntaxKind::OpenBracketToken => {
                    self.parse_element_index_postfix(expression, at_boundary)
                }
                SyntaxKind::OpenParenToken
                    if self.should_parse_trait_qualified_member_operation() =>
                {
                    self.parse_trait_qualified_member_postfix(expression)
                }
                SyntaxKind::OpenParenToken => self.parse_call_postfix(expression),
                SyntaxKind::QuestionToken => self.parse_nullable_propagation_postfix(expression),
                SyntaxKind::AsKeyword => self.parse_conversion_postfix(expression, at_boundary),
                _ => break,
            };
        }

        expression
    }

    fn primary_to_expression(&mut self, primary: PrimaryExpressionSyntax) -> ExpressionSyntax {
        let start = primary.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_primary_expression(primary);

        builder.build()
    }

    fn parse_member_access_postfix(&mut self, expression: ExpressionSyntax) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_member_access_operation(self.parse_member_access_operation());

        builder.build()
    }

    fn parse_element_index_postfix(
        &mut self,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_element_index_operation(self.parse_element_index_operation(at_boundary));

        builder.build()
    }

    fn parse_call_postfix(&mut self, expression: ExpressionSyntax) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_call_operation(self.parse_call_operation());

        builder.build()
    }

    fn parse_slice_index_postfix(
        &mut self,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_slice_index_operation(self.parse_slice_index_operation(at_boundary));

        builder.build()
    }

    fn parse_nullable_propagation_postfix(
        &mut self,
        expression: ExpressionSyntax,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_nullable_propagation_operation(self.parse_nullable_propagation_operation());

        builder.build()
    }

    fn parse_conversion_postfix(
        &mut self,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder.push_conversion_operation(self.parse_conversion_operation(at_boundary));

        builder.build()
    }

    fn parse_trait_qualified_member_postfix(
        &mut self,
        expression: ExpressionSyntax,
    ) -> ExpressionSyntax {
        let start = expression.full_range().start();
        let mut builder = ExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_expression(expression);
        builder
            .push_trait_qualified_member_operation(self.parse_trait_qualified_member_operation());

        builder.build()
    }

    fn parse_primary_expression_until(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        if at_boundary(self) || at_primary_hard_boundary(self.peek().kind()) {
            return self.missing_primary_expression();
        }

        match self.peek().kind() {
            kind if at_literal_expression_start(kind) => self.parse_literal_primary_expression(),
            SyntaxKind::UnitKeyword => self.parse_unit_primary_expression(),
            SyntaxKind::NoneKeyword => self.parse_absence_primary_expression(),
            kind if at_access_expression_start(kind) => {
                self.parse_access_primary_expression(at_boundary)
            }
            SyntaxKind::OpenParenToken => self.parse_parenthesized_primary_expression(at_boundary),
            SyntaxKind::OpenBracketToken => self.parse_array_primary_expression(at_boundary),
            SyntaxKind::OpenBraceToken if self.should_parse_expected_type_struct_construction() => {
                self.parse_expected_type_struct_construction_primary()
            }
            SyntaxKind::OpenBraceToken => self.parse_block_primary_expression(at_boundary),
            SyntaxKind::DotToken => self.parse_leading_dot_variant_primary_expression(),
            kind if EXPRESSION_START_KINDS.contains(&kind) => {
                self.parse_unsupported_primary_expression(at_boundary)
            }
            _ => self.parse_unknown_primary_expression(at_boundary),
        }
    }

    fn parse_literal_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut literal = LiteralExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        literal.push_literal_token(self.consume());
        primary.push_literal_expression(literal.build());

        primary.build()
    }

    fn parse_unit_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut unit = UnitExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        unit.push_unit_keyword(self.expect(SyntaxKind::UnitKeyword));
        primary.push_unit_expression(unit.build());

        primary.build()
    }

    fn parse_absence_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut absence = AbsenceExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        absence.push_none_keyword(self.expect(SyntaxKind::NoneKeyword));
        primary.push_absence_expression(absence.build());

        primary.build()
    }

    fn parse_access_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_access_expression(self.parse_access_expression(at_boundary));

        if self.at(SyntaxKind::OpenBraceToken) {
            primary.push_struct_construction_body(self.parse_struct_construction_body());
        }

        primary.build()
    }

    fn parse_expected_type_struct_construction_primary(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_struct_construction_body(self.parse_struct_construction_body());

        primary.build()
    }

    fn parse_block_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_block_expression(self.parse_block_expression_until(at_boundary));

        primary.build()
    }

    fn parse_parenthesized_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let open_paren = self.expect(SyntaxKind::OpenParenToken);

        let mut at_expression_boundary =
            |parser: &mut Parser| parser.at_parenthesized_expression_boundary(at_boundary);

        let expression = self.parse_expression_until(&mut at_expression_boundary);

        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        if self.at(SyntaxKind::CommaToken) || self.at_parenthesized_tuple_missing_separator() {
            primary.push_tuple_expression(self.parse_tuple_expression_tail(
                open_paren,
                expression,
                at_boundary,
            ));
        } else {
            primary.push_grouped_expression(
                self.parse_grouped_expression_tail(open_paren, expression),
            );
        }

        primary.build()
    }

    fn parse_grouped_expression_tail(
        &mut self,
        open_paren: SyntaxToken,
        expression: ExpressionSyntax,
    ) -> GroupedExpressionSyntax {
        let start = open_paren.full_range().start();
        let mut builder = GroupedExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(open_paren);
        builder.push_expression(expression);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_tuple_expression_tail(
        &mut self,
        open_paren: SyntaxToken,
        expression: ExpressionSyntax,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> TupleExpressionSyntax {
        let start = open_paren.full_range().start();
        let mut builder = TupleExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(open_paren);

        builder.push_expression(expression);
        self.parse_tuple_expression_tail_items(&mut builder, at_boundary);

        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_tuple_expression_tail_items(
        &mut self,
        builder: &mut TupleExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        loop {
            if self.at(SyntaxKind::CloseParenToken)
                || self.at(SyntaxKind::EndOfFileToken)
                || at_boundary(self)
            {
                break;
            }

            builder.push_separator_token(self.expect(SyntaxKind::CommaToken));

            if self.at(SyntaxKind::CloseParenToken)
                || self.at(SyntaxKind::EndOfFileToken)
                || at_boundary(self)
            {
                break;
            }

            let mut at_tuple_boundary = Parser::at_tuple_expression_element_boundary;

            builder.push_expression(self.parse_expression_until(&mut at_tuple_boundary));
        }
    }

    fn parse_array_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        primary.push_array_expression(self.parse_array_expression(at_boundary));

        primary.build()
    }

    fn parse_array_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ArrayExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ArrayExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        if self.at(SyntaxKind::CloseBracketToken) {
            builder.push_expression(self.missing_expression());
        } else {
            self.parse_array_expression_items(&mut builder, at_boundary);
        }

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_array_expression_items(
        &mut self,
        builder: &mut ArrayExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        let mut at_element_boundary =
            |parser: &mut Parser| parser.at_array_expression_element_boundary(at_boundary);

        builder.push_expression(self.parse_expression_until(&mut at_element_boundary));

        if self.at(SyntaxKind::SemicolonToken) {
            builder.push_semicolon_token(self.expect(SyntaxKind::SemicolonToken));

            let mut at_count_boundary = Parser::at_repeated_array_count_boundary;

            builder.push_expression(
                self.parse_non_assignment_expression_until(&mut at_count_boundary),
            );

            return;
        }

        self.parse_array_element_tail(builder, at_boundary);
    }

    fn parse_array_element_tail(
        &mut self,
        builder: &mut ArrayExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        let recovery_kinds = array_element_recovery_kinds();

        while !self.at(SyntaxKind::CloseBracketToken)
            && !self.at(SyntaxKind::EndOfFileToken)
            && !at_boundary(self)
        {
            if self.at(SyntaxKind::CommaToken) || self.at_array_element_missing_separator() {
                builder.push_separator_token(self.expect(SyntaxKind::CommaToken));

                if self.at(SyntaxKind::CloseBracketToken) || self.at(SyntaxKind::EndOfFileToken) {
                    break;
                }

                let mut at_element_boundary =
                    |parser: &mut Parser| parser.at_array_expression_element_boundary(at_boundary);

                builder.push_expression(self.parse_expression_until(&mut at_element_boundary));
                continue;
            }

            if self.recover_until(builder, &recovery_kinds) {
                continue;
            }

            self.recover_current_token(builder);
        }
    }

    fn parse_leading_dot_variant_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();

        let mut variant = LeadingDotVariantExpressionSyntax::builder(self.syntax_source(), start);
        let mut primary = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        variant.push_dot_token(self.expect(SyntaxKind::DotToken));
        variant.push_identifier_token(self.parse_identifier());
        primary.push_leading_dot_variant_expression(variant.build());

        primary.build()
    }

    fn parse_unsupported_primary_expression(
        &mut self,
        _at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        // TODO(parser): Parse remaining primary-expression roots as typed syntax.
        builder.push_token(self.consume());

        builder.build()
    }

    fn parse_unknown_primary_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        self.recover_current_and_until_predicate(&mut builder, |parser| {
            parser.at_primary_tail_boundary(at_boundary)
        });

        builder.build()
    }

    fn parse_access_expression(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> AccessExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = AccessExpressionSyntax::builder(self.syntax_source(), start);

        match self.peek().kind() {
            SyntaxKind::IdentifierToken => builder.push_identifier_token(self.parse_identifier()),
            SyntaxKind::SelfValueKeyword => {
                builder.push_self_token(self.expect(SyntaxKind::SelfValueKeyword));
            }
            SyntaxKind::InternalKeyword => {
                builder.push_internal_token(self.expect(SyntaxKind::InternalKeyword));
                builder.push_access_expression(self.parse_access_expression(at_boundary));
            }
            SyntaxKind::OpenParenToken => {
                builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
                builder.push_access_expression(self.parse_access_expression(at_boundary));
                builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
            }
            _ => {
                self.recover_current_and_until_predicate(&mut builder, |parser| {
                    parser.at_primary_tail_boundary(at_boundary)
                });
            }
        }

        self.parse_access_expression_postfixes(&mut builder, at_boundary);

        builder.build()
    }

    fn parse_access_expression_postfixes(
        &mut self,
        builder: &mut bray_syntax::AccessExpressionSyntaxBuilder,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) {
        loop {
            if at_boundary(self) {
                break;
            }

            match self.peek().kind() {
                SyntaxKind::DotToken => {
                    builder.push_member_access_operation(self.parse_member_access_operation());
                }
                SyntaxKind::OpenBracketToken if !self.should_parse_slice_index_operation() => {
                    builder.push_element_index_operation(
                        self.parse_element_index_operation(at_boundary),
                    );
                }
                _ => break,
            }
        }
    }

    fn parse_member_access_operation(&mut self) -> MemberAccessOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = MemberAccessOperationSyntax::builder(self.syntax_source(), start);

        builder.push_dot_token(self.expect(SyntaxKind::DotToken));

        match self.peek().kind() {
            SyntaxKind::TupleElementIndexToken => {
                builder.push_tuple_element_index_token(
                    self.expect(SyntaxKind::TupleElementIndexToken),
                );
            }
            _ => builder.push_identifier_token(self.parse_identifier()),
        }

        builder.build()
    }

    fn parse_element_index_operation(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ElementIndexOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ElementIndexOperationSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));
        builder.push_expression(
            self.parse_expression_until(&mut |parser| {
                parser.at_element_index_boundary(at_boundary)
            }),
        );
        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_call_operation(&mut self) -> CallOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = CallOperationSyntax::builder(self.syntax_source(), start);

        builder.push_argument_list(self.parse_argument_list());

        builder.build()
    }

    fn parse_argument_list(&mut self) -> ArgumentListSyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &EXPRESSION_START_KINDS,
            SyntaxKind::CommaToken,
            &ARGUMENT_LIST_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &ARGUMENT_LIST_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = ArgumentListSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_argument);
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));

        builder.build()
    }

    fn parse_argument(&mut self) -> ArgumentSyntax {
        let start = self.peek().full_range().start();
        let mut builder = ArgumentSyntax::builder(self.syntax_source(), start);

        if self.at_named_argument_start() {
            builder.push_identifier_token(self.parse_identifier());
            builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));
        }

        let mut at_argument_boundary = Parser::at_argument_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_argument_boundary));

        builder.build()
    }

    fn parse_slice_index_operation(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> SliceIndexOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = SliceIndexOperationSyntax::builder(self.syntax_source(), start);

        builder.push_open_bracket_token(self.expect(SyntaxKind::OpenBracketToken));

        if !self.at(SyntaxKind::DotDotToken) {
            let mut at_start_boundary =
                |parser: &mut Parser| parser.at_slice_selector_start_boundary(at_boundary);

            builder.push_expression(self.parse_expression_until(&mut at_start_boundary));
        }

        builder.push_dot_dot_token(self.expect(SyntaxKind::DotDotToken));

        if !self.at(SyntaxKind::CloseBracketToken) && !at_boundary(self) {
            let mut at_end_boundary =
                |parser: &mut Parser| parser.at_slice_selector_end_boundary(at_boundary);

            builder.push_expression(self.parse_expression_until(&mut at_end_boundary));
        }

        builder.push_close_bracket_token(self.expect(SyntaxKind::CloseBracketToken));

        builder.build()
    }

    fn parse_nullable_propagation_operation(&mut self) -> NullablePropagationOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = NullablePropagationOperationSyntax::builder(self.syntax_source(), start);

        builder.push_question_token(self.expect(SyntaxKind::QuestionToken));

        builder.build()
    }

    fn parse_conversion_operation(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> ConversionOperationSyntax {
        let start = self.peek().full_range().start();

        let mut builder = ConversionOperationSyntax::builder(self.syntax_source(), start);
        let mut at_type_boundary =
            |parser: &mut Parser| parser.at_conversion_type_boundary(at_boundary);

        builder.push_as_keyword(self.expect(SyntaxKind::AsKeyword));
        builder.push_type_expression(self.parse_type_expression_until(&mut at_type_boundary));

        builder.build()
    }

    fn parse_trait_qualified_member_operation(&mut self) -> TraitQualifiedMemberOperationSyntax {
        let start = self.peek().full_range().start();
        let mut builder = TraitQualifiedMemberOperationSyntax::builder(self.syntax_source(), start);

        builder.push_open_paren_token(self.expect(SyntaxKind::OpenParenToken));
        builder.push_trait_application(self.parse_trait_application());
        builder.push_close_paren_token(self.expect(SyntaxKind::CloseParenToken));
        builder.push_member_access_operation(self.parse_member_access_operation());

        builder.build()
    }

    fn parse_struct_construction_body(&mut self) -> StructConstructionBodySyntax {
        let start = self.peek().full_range().start();

        let recovery_kinds = separated_list_recovery_kinds(
            &STRUCT_FIELD_INITIALIZER_START_KINDS,
            SyntaxKind::CommaToken,
            &STRUCT_CONSTRUCTION_BODY_TERMINATORS,
        );

        let spec = SeparatedListSpec {
            separator_kind: SyntaxKind::CommaToken,
            terminators: &STRUCT_CONSTRUCTION_BODY_TERMINATORS,
            recovery_kinds: &recovery_kinds,
            allow_trailing_separator: true,
        };

        let mut builder = StructConstructionBodySyntax::builder(self.syntax_source(), start);

        builder.push_open_brace_token(self.expect(SyntaxKind::OpenBraceToken));
        self.parse_separated_list(&mut builder, spec, Parser::parse_struct_field_initializer);
        builder.push_close_brace_token(self.expect(SyntaxKind::CloseBraceToken));

        builder.build()
    }

    fn parse_struct_field_initializer(&mut self) -> StructFieldInitializerSyntax {
        let start = self.peek().full_range().start();
        let mut builder = StructFieldInitializerSyntax::builder(self.syntax_source(), start);

        builder.push_identifier_token(self.parse_identifier());
        builder.push_equals_token(self.expect(SyntaxKind::EqualsToken));

        let mut at_initializer_boundary = Parser::at_struct_field_initializer_boundary;

        builder.push_expression(self.parse_expression_until(&mut at_initializer_boundary));

        builder.build()
    }

    fn missing_expression(&mut self) -> ExpressionSyntax {
        let primary = self.missing_primary_expression();

        self.primary_to_expression(primary)
    }

    fn missing_primary_expression(&mut self) -> PrimaryExpressionSyntax {
        let start = self.peek().full_range().start();
        let mut builder = PrimaryExpressionSyntax::builder(self.syntax_source(), start);

        builder.push_token(self.expect(SyntaxKind::IdentifierToken));

        builder.build()
    }

    fn should_parse_slice_index_operation(&mut self) -> bool {
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

    fn should_parse_trait_qualified_member_operation(&mut self) -> bool {
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

    fn should_parse_expected_type_struct_construction(&mut self) -> bool {
        self.at(SyntaxKind::OpenBraceToken)
            && self.lookahead(1).kind() == SyntaxKind::IdentifierToken
            && self.lookahead(2).kind() == SyntaxKind::EqualsToken
    }

    fn at_named_argument_start(&mut self) -> bool {
        self.at(SyntaxKind::IdentifierToken) && self.lookahead(1).kind() == SyntaxKind::EqualsToken
    }

    fn at_parenthesized_expression_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CloseParenToken) || self.at(SyntaxKind::CommaToken) || at_boundary(self)
    }

    fn at_parenthesized_tuple_missing_separator(&mut self) -> bool {
        EXPRESSION_START_KINDS.contains(&self.peek().kind())
    }

    fn at_tuple_expression_element_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken)
            || self.at(SyntaxKind::CloseParenToken)
            || self.at(SyntaxKind::EndOfFileToken)
    }

    fn at_array_expression_element_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CommaToken)
            || self.at(SyntaxKind::SemicolonToken)
            || self.at_any(&ARRAY_EXPRESSION_TERMINATORS)
            || at_boundary(self)
    }

    fn at_array_element_missing_separator(&mut self) -> bool {
        EXPRESSION_START_KINDS.contains(&self.peek().kind())
    }

    fn at_repeated_array_count_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CloseBracketToken) || self.at(SyntaxKind::EndOfFileToken)
    }

    fn at_element_index_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at(SyntaxKind::CloseBracketToken) || at_boundary(self)
    }

    fn at_argument_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at_any(&ARGUMENT_LIST_TERMINATORS)
    }

    fn at_slice_selector_start_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at_any(&SLICE_SELECTOR_TERMINATORS) || at_boundary(self)
    }

    fn at_slice_selector_end_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        self.at_any(&SLICE_END_TERMINATORS) || at_boundary(self)
    }

    fn at_conversion_type_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        let kind = self.peek().kind();

        at_boundary(self)
            || at_primary_hard_boundary(kind)
            || kind == SyntaxKind::CommaToken
            || infix_operator(kind).is_some()
    }

    fn at_struct_field_initializer_boundary(&mut self) -> bool {
        self.at(SyntaxKind::CommaToken) || self.at_any(&STRUCT_CONSTRUCTION_BODY_TERMINATORS)
    }

    fn at_primary_tail_boundary(
        &mut self,
        at_boundary: &mut dyn FnMut(&mut Parser) -> bool,
    ) -> bool {
        let kind = self.peek().kind();

        at_boundary(self)
            || at_primary_hard_boundary(kind)
            || kind == SyntaxKind::EqualsToken
            || infix_operator(kind).is_some()
    }
}

impl SeparatedListSyntaxSink<ArgumentSyntax> for ArgumentListSyntaxBuilder {
    fn push_item(&mut self, item: ArgumentSyntax) {
        ArgumentListSyntaxBuilder::push_argument(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        ArgumentListSyntaxBuilder::push_separator_token(self, separator);
    }
}

impl SeparatedListSyntaxSink<StructFieldInitializerSyntax> for StructConstructionBodySyntaxBuilder {
    fn push_item(&mut self, item: StructFieldInitializerSyntax) {
        StructConstructionBodySyntaxBuilder::push_field_initializer(self, item);
    }

    fn push_separator(&mut self, separator: SyntaxToken) {
        StructConstructionBodySyntaxBuilder::push_separator_token(self, separator);
    }
}

fn infix_operator(kind: SyntaxKind) -> Option<InfixOperator> {
    let (left_binding_power, right_binding_power, associativity) = match kind {
        SyntaxKind::PipePipeToken => (1, 2, OperatorAssociativity::Left),
        SyntaxKind::AmpersandAmpersandToken => (3, 4, OperatorAssociativity::Left),
        SyntaxKind::EqualsEqualsToken
        | SyntaxKind::BangEqualsToken
        | SyntaxKind::LessToken
        | SyntaxKind::LessEqualsToken
        | SyntaxKind::GreaterToken
        | SyntaxKind::GreaterEqualsToken => (5, 6, OperatorAssociativity::None),
        SyntaxKind::PipeToken => (7, 8, OperatorAssociativity::Left),
        SyntaxKind::CaretToken => (9, 10, OperatorAssociativity::Left),
        SyntaxKind::AmpersandToken => (11, 12, OperatorAssociativity::Left),
        SyntaxKind::LessLessToken | SyntaxKind::GreaterGreaterToken => {
            (13, 14, OperatorAssociativity::Left)
        }
        SyntaxKind::PlusToken | SyntaxKind::MinusToken => (15, 16, OperatorAssociativity::Left),
        SyntaxKind::StarToken
        | SyntaxKind::SlashToken
        | SyntaxKind::PercentToken
        | SyntaxKind::AtToken => (17, 18, OperatorAssociativity::Left),
        SyntaxKind::StarStarToken => (21, 20, OperatorAssociativity::Right),
        _ => return None,
    };

    Some(InfixOperator {
        left_binding_power,
        right_binding_power,
        associativity,
    })
}

fn at_unary_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::MinusToken | SyntaxKind::TildeToken | SyntaxKind::BangToken
    )
}

fn at_literal_expression_start(kind: SyntaxKind) -> bool {
    kind.is_literal() || matches!(kind, SyntaxKind::FalseKeyword | SyntaxKind::TrueKeyword)
}

fn at_access_expression_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IdentifierToken | SyntaxKind::InternalKeyword | SyntaxKind::SelfValueKeyword
    )
}

fn at_primary_hard_boundary(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CommaToken
            | SyntaxKind::CloseParenToken
            | SyntaxKind::CloseBracketToken
            | SyntaxKind::CloseBraceToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::EndOfFileToken
    )
}

fn array_element_recovery_kinds() -> Vec<SyntaxKind> {
    let mut recovery_kinds = Vec::with_capacity(EXPRESSION_START_KINDS.len() + 6);

    recovery_kinds.extend_from_slice(&EXPRESSION_START_KINDS);
    recovery_kinds.push(SyntaxKind::CommaToken);
    recovery_kinds.push(SyntaxKind::SemicolonToken);
    recovery_kinds.extend_from_slice(&ARRAY_EXPRESSION_TERMINATORS);

    recovery_kinds
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;
    use bray_source::{TextRange, TextSize};
    use bray_syntax::{SyntaxKind, SyntaxText};
    use bray_testing::test_source_store as source_store;

    use crate::test_support::{diagnostic_kinds, source};

    use super::super::state::Parser;

    #[test]
    fn parser_parses_expression_precedence_and_associativity() {
        let sources = source_store(["left + right * -tail ** value;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [left, right] = children.as_slice() else {
            panic!("expected binary expression children: {children:?}");
        };

        assert_eq!(expression.full_text(), "left + right * -tail ** value");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::PlusToken)
        );

        assert!(left.primary_expression().is_some());

        assert_eq!(
            right.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::StarToken)
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_assignment_as_right_associative() {
        let sources = source_store(["left = middle = right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [_left, right] = children.as_slice() else {
            panic!("expected assignment expression children: {children:?}");
        };

        assert_eq!(expression.full_text(), "left = middle = right");

        assert_eq!(
            expression.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::EqualsToken)
        );

        assert_eq!(
            right.operator_token().map(|token| token.kind()),
            Some(SyntaxKind::EqualsToken)
        );

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_non_assignment_expression_stops_before_assignment() {
        let sources = source_store(["left = right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_non_assignment_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), "left ");
        assert_eq!(parser.peek().kind(), SyntaxKind::EqualsToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_leaves_second_comparison_for_recovery() {
        let sources = source_store(["left < middle < right;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);

        assert_eq!(expression.full_text(), "left < middle ");
        assert_eq!(parser.peek().kind(), SyntaxKind::LessToken);

        let diagnostics = parser.finish();

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_inserts_missing_expression_operand_at_boundary() {
        let sources = source_store(["value + ;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();
        let children = expression.expressions().collect::<Vec<_>>();

        let [_left, right] = children.as_slice() else {
            panic!("expected binary expression children: {children:?}");
        };

        let primary = match right.primary_expression() {
            Some(primary) => primary,
            None => panic!("expected missing primary expression"),
        };

        let token = match primary.primary_token() {
            Some(token) => token,
            None => panic!("expected missing primary token"),
        };

        assert_eq!(expression.full_text(), "value + ");
        assert!(token.is_missing());

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [DiagnosticKind::SyntaxExpectedToken]
        );
    }

    #[test]
    fn parser_parses_typed_access_call_index_slice_and_conversion_postfixes() {
        let sources = source_store(["target.call(1, named = value)[0][start..end]? as Result;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(
            expression.full_text(),
            "target.call(1, named = value)[0][start..end]? as Result"
        );

        assert_eq!(count_call_operations(&expression), 1);
        assert_eq!(count_element_index_operations(&expression), 1);
        assert_eq!(count_slice_index_operations(&expression), 1);
        assert_eq!(count_nullable_propagation_operations(&expression), 1);
        assert_eq!(count_conversion_operations(&expression), 1);

        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_trait_qualified_member_postfix() {
        let sources = source_store(["target(Display).format;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        assert_eq!(expression.full_text(), "target(Display).format");
        assert_eq!(count_trait_qualified_member_operations(&expression), 1);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn parser_parses_grouped_tuple_array_literals_structs_and_leading_dot_variants() {
        let cases = [
            ("(value);", "(value)", SyntaxKind::GroupedExpression),
            (
                "(value, next,);",
                "(value, next,)",
                SyntaxKind::TupleExpression,
            ),
            (
                "[value, next,];",
                "[value, next,]",
                SyntaxKind::ArrayExpression,
            ),
            ("[value; 3];", "[value; 3]", SyntaxKind::ArrayExpression),
            (
                "Point { x = 1, };",
                "Point { x = 1, }",
                SyntaxKind::StructConstructionBody,
            ),
            (
                "{ x = 1 };",
                "{ x = 1 }",
                SyntaxKind::StructConstructionBody,
            ),
            (
                ".Some(1);",
                ".Some(1)",
                SyntaxKind::LeadingDotVariantExpression,
            ),
            ("true;", "true", SyntaxKind::LiteralExpression),
            ("unit;", "unit", SyntaxKind::UnitExpression),
            ("none;", "none", SyntaxKind::AbsenceExpression),
        ];

        for (source_text, expected_text, expected_kind) in cases {
            let sources = source_store([source_text]);
            let snapshot = source(&sources, 0);

            let mut parser = Parser::new(snapshot);
            let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

            let expression = parser.parse_expression_until(&mut boundary);
            let diagnostics = parser.finish();

            assert_eq!(expression.full_text(), expected_text, "{source_text}");

            assert!(
                expression
                    .tokens()
                    .next()
                    .is_some_and(|token| !token.is_missing()),
                "{source_text}"
            );

            assert!(
                first_primary_expression(&expression).is_some(),
                "{source_text}"
            );

            assert!(
                first_primary_expression(&expression)
                    .is_some_and(|primary| primary_contains_child_kind(&primary, expected_kind)),
                "{source_text}"
            );

            assert!(diagnostics.is_empty(), "{source_text}: {diagnostics:?}");
        }
    }

    #[test]
    fn parser_represents_missing_argument_separators_and_skipped_argument_tokens() {
        let sources = source_store(["call(first second, $, third);"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let call = match expression.call_operations().next() {
            Some(call) => call,
            None => panic!("call operation should be present"),
        };

        let list = call.argument_list();
        let separators = list.separator_tokens().collect::<Vec<_>>();
        let skipped = list.skipped_syntax().collect::<Vec<_>>();

        assert_eq!(expression.full_text(), "call(first second, $, third)");
        assert!(separators.iter().any(|separator| separator.is_missing()));

        let [skipped_syntax] = skipped.as_slice() else {
            panic!("expected skipped argument syntax: {skipped:?}");
        };

        assert_eq!(skipped_syntax.full_text(), "$");

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::LexicalInvalidCharacter,
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxSkippedSyntax
            ]
        );
    }

    #[test]
    fn parser_reports_missing_close_for_grouped_expression_at_insertion_point() {
        let sources = source_store(["(value next;"]);
        let snapshot = source(&sources, 0);

        let mut parser = Parser::new(snapshot);
        let mut boundary = |parser: &mut Parser| parser.at(SyntaxKind::SemicolonToken);

        let expression = parser.parse_expression_until(&mut boundary);
        let diagnostics = parser.finish();

        let token = match expression
            .primary_expression()
            .and_then(|primary| primary.tuple_expressions().next())
            .map(|tuple| tuple.close_paren_token())
        {
            Some(token) => token,
            None => panic!("tuple close paren token should be present"),
        };

        assert_eq!(expression.full_text(), "(value next");
        assert!(token.is_missing());
        assert_eq!(token.range(), TextRange::empty(TextSize::new(11)));

        assert_eq!(
            diagnostic_kinds(&diagnostics),
            [
                DiagnosticKind::SyntaxExpectedToken,
                DiagnosticKind::SyntaxExpectedToken
            ]
        );
    }

    fn primary_contains_child_kind(
        primary: &bray_syntax::PrimaryExpressionSyntax,
        kind: SyntaxKind,
    ) -> bool {
        match kind {
            SyntaxKind::AbsenceExpression => primary.absence_expressions().next().is_some(),
            SyntaxKind::ArrayExpression => primary.array_expressions().next().is_some(),
            SyntaxKind::GroupedExpression => primary.grouped_expressions().next().is_some(),
            SyntaxKind::LeadingDotVariantExpression => {
                primary.leading_dot_variant_expressions().next().is_some()
            }
            SyntaxKind::LiteralExpression => primary.literal_expressions().next().is_some(),
            SyntaxKind::StructConstructionBody => {
                primary.struct_construction_bodies().next().is_some()
            }
            SyntaxKind::TupleExpression => primary.tuple_expressions().next().is_some(),
            SyntaxKind::UnitExpression => primary.unit_expressions().next().is_some(),
            _ => false,
        }
    }

    fn first_primary_expression(
        expression: &bray_syntax::ExpressionSyntax,
    ) -> Option<bray_syntax::PrimaryExpressionSyntax> {
        expression.primary_expression().or_else(|| {
            expression
                .expressions()
                .find_map(|child| first_primary_expression(&child))
        })
    }

    fn count_call_operations(expression: &bray_syntax::ExpressionSyntax) -> usize {
        expression.call_operations().count()
            + expression
                .expressions()
                .map(|child| count_call_operations(&child))
                .sum::<usize>()
    }

    fn count_element_index_operations(expression: &bray_syntax::ExpressionSyntax) -> usize {
        expression.element_index_operations().count()
            + expression
                .expressions()
                .map(|child| count_element_index_operations(&child))
                .sum::<usize>()
    }

    fn count_slice_index_operations(expression: &bray_syntax::ExpressionSyntax) -> usize {
        expression.slice_index_operations().count()
            + expression
                .expressions()
                .map(|child| count_slice_index_operations(&child))
                .sum::<usize>()
    }

    fn count_nullable_propagation_operations(expression: &bray_syntax::ExpressionSyntax) -> usize {
        expression.nullable_propagation_operations().count()
            + expression
                .expressions()
                .map(|child| count_nullable_propagation_operations(&child))
                .sum::<usize>()
    }

    fn count_conversion_operations(expression: &bray_syntax::ExpressionSyntax) -> usize {
        expression.conversion_operations().count()
            + expression
                .expressions()
                .map(|child| count_conversion_operations(&child))
                .sum::<usize>()
    }

    fn count_trait_qualified_member_operations(
        expression: &bray_syntax::ExpressionSyntax,
    ) -> usize {
        expression.trait_qualified_member_operations().count()
            + expression
                .expressions()
                .map(|child| count_trait_qualified_member_operations(&child))
                .sum::<usize>()
    }
}
