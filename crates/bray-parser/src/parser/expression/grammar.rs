use bray_syntax::SyntaxKind;

pub(in crate::parser) const EXPRESSION_START_KINDS: [SyntaxKind; 46] = [
    SyntaxKind::AllKeyword,
    SyntaxKind::AmpersandToken,
    SyntaxKind::AnyKeyword,
    SyntaxKind::AssertKeyword,
    SyntaxKind::AsyncKeyword,
    SyntaxKind::AtToken,
    SyntaxKind::AwaitKeyword,
    SyntaxKind::BangToken,
    SyntaxKind::BinaryIntegerLiteralToken,
    SyntaxKind::BoxKeyword,
    SyntaxKind::BreakKeyword,
    SyntaxKind::CatchKeyword,
    SyntaxKind::CharacterLiteralToken,
    SyntaxKind::ConstKeyword,
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

pub(in crate::parser::expression) const ARGUMENT_LIST_TERMINATORS: [SyntaxKind; 5] = [
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

pub(in crate::parser::expression) const STRUCT_FIELD_INITIALIZER_START_KINDS: [SyntaxKind; 1] =
    [SyntaxKind::IdentifierToken];

pub(in crate::parser::expression) const STRUCT_CONSTRUCTION_BODY_TERMINATORS: [SyntaxKind; 5] = [
    SyntaxKind::CloseBraceToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::SemicolonToken,
    SyntaxKind::EndOfFileToken,
];

pub(in crate::parser::expression) const ARRAY_EXPRESSION_TERMINATORS: [SyntaxKind; 4] = [
    SyntaxKind::CloseBracketToken,
    SyntaxKind::CloseParenToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

pub(in crate::parser::expression) const SLICE_SELECTOR_TERMINATORS: [SyntaxKind; 4] = [
    SyntaxKind::DotDotToken,
    SyntaxKind::CloseBracketToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

pub(in crate::parser::expression) const SLICE_END_TERMINATORS: [SyntaxKind; 3] = [
    SyntaxKind::CloseBracketToken,
    SyntaxKind::CloseBraceToken,
    SyntaxKind::EndOfFileToken,
];

pub(in crate::parser::expression) fn at_literal_expression_start(kind: SyntaxKind) -> bool {
    kind.is_literal() || matches!(kind, SyntaxKind::FalseKeyword | SyntaxKind::TrueKeyword)
}

pub(in crate::parser::expression) fn at_access_expression_start(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::IdentifierToken | SyntaxKind::InternalKeyword | SyntaxKind::SelfValueKeyword
    )
}

pub(in crate::parser::expression) fn at_primary_hard_boundary(kind: SyntaxKind) -> bool {
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

pub(in crate::parser::expression) fn array_element_recovery_kinds() -> Vec<SyntaxKind> {
    let mut recovery_kinds = Vec::with_capacity(EXPRESSION_START_KINDS.len() + 6);

    recovery_kinds.extend_from_slice(&EXPRESSION_START_KINDS);
    recovery_kinds.push(SyntaxKind::CommaToken);
    recovery_kinds.push(SyntaxKind::SemicolonToken);
    recovery_kinds.extend_from_slice(&ARRAY_EXPRESSION_TERMINATORS);

    recovery_kinds
}
