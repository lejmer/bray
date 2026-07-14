use bray_bound_tree::{
    BoundMemberSelector, BoundOperator, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnresolvedReferenceKind,
};
use bray_symbols::MemberLookupResult;
use bray_syntax::{SourceSyntaxNode, SyntaxKind};

use crate::binding::name::symbol_name;
use crate::lookup::ResolvedName;

pub(super) fn classify_operator(kind: SyntaxKind) -> Option<BoundOperator> {
    Some(match kind {
        SyntaxKind::EqualsToken => BoundOperator::Assign,
        SyntaxKind::PipePipeToken => BoundOperator::LogicalOr,
        SyntaxKind::AmpersandAmpersandToken => BoundOperator::LogicalAnd,
        SyntaxKind::EqualsEqualsToken => BoundOperator::Equal,
        SyntaxKind::BangEqualsToken => BoundOperator::NotEqual,
        SyntaxKind::LessToken => BoundOperator::Less,
        SyntaxKind::LessEqualsToken => BoundOperator::LessEqual,
        SyntaxKind::GreaterToken => BoundOperator::Greater,
        SyntaxKind::GreaterEqualsToken => BoundOperator::GreaterEqual,
        SyntaxKind::PipeToken => BoundOperator::BitwiseOr,
        SyntaxKind::CaretToken => BoundOperator::BitwiseXor,
        SyntaxKind::AmpersandToken => BoundOperator::BitwiseAnd,
        SyntaxKind::LessLessToken => BoundOperator::ShiftLeft,
        SyntaxKind::GreaterGreaterToken => BoundOperator::ShiftRight,
        SyntaxKind::PlusToken => BoundOperator::Add,
        SyntaxKind::MinusToken => BoundOperator::Subtract,
        SyntaxKind::StarToken => BoundOperator::Multiply,
        SyntaxKind::SlashToken => BoundOperator::Divide,
        SyntaxKind::PercentToken => BoundOperator::Remainder,
        SyntaxKind::AtToken => BoundOperator::MatrixMultiply,
        SyntaxKind::StarStarToken => BoundOperator::Exponentiate,
        SyntaxKind::TildeToken => BoundOperator::BitwiseNot,
        SyntaxKind::BangToken => BoundOperator::LogicalNot,
        _ => return None,
    })
}

pub(super) fn structured_kind(kind: SyntaxKind) -> Option<BoundStructuredExpressionKind> {
    Some(match kind {
        SyntaxKind::LiteralExpression => BoundStructuredExpressionKind::Literal,
        SyntaxKind::UnitExpression => BoundStructuredExpressionKind::Unit,
        SyntaxKind::AbsenceExpression => BoundStructuredExpressionKind::Absence,
        SyntaxKind::TupleExpression => BoundStructuredExpressionKind::Tuple,
        SyntaxKind::ArrayExpression => BoundStructuredExpressionKind::Array,
        SyntaxKind::ConditionalExpression => BoundStructuredExpressionKind::Conditional,
        SyntaxKind::WhileExpression => BoundStructuredExpressionKind::While,
        SyntaxKind::LoopExpression => BoundStructuredExpressionKind::Loop,
        SyntaxKind::WithExpression => BoundStructuredExpressionKind::With,
        SyntaxKind::BorrowExpression => BoundStructuredExpressionKind::Borrow,
        SyntaxKind::TrustBoundaryExpression => BoundStructuredExpressionKind::TrustBoundary,
        SyntaxKind::AssertionExpression => BoundStructuredExpressionKind::Assertion,
        SyntaxKind::ResultPropagationExpression => BoundStructuredExpressionKind::ResultPropagation,
        SyntaxKind::CatchExpression => BoundStructuredExpressionKind::Catch,
        SyntaxKind::AwaitExpression => BoundStructuredExpressionKind::Await,
        SyntaxKind::TypeFormConstructionExpression => {
            BoundStructuredExpressionKind::TypeFormConstruction
        }
        SyntaxKind::BooleanFoldExpression => BoundStructuredExpressionKind::BooleanFold,
        SyntaxKind::AsyncBlockExpression => BoundStructuredExpressionKind::AsyncBlock,
        SyntaxKind::PanicExpression => BoundStructuredExpressionKind::Panic,
        _ => return None,
    })
}

pub(super) fn member_selector(
    operation: &bray_syntax::MemberAccessOperationSyntax,
) -> Option<BoundMemberSelector> {
    if let Some(token) = operation.identifier_token() {
        return symbol_name(operation.source(), &token).map(BoundMemberSelector::Name);
    }

    let token = operation.tuple_element_index_token()?;
    let text = token.text(operation.source().text())?;
    let index = text.trim_start_matches('.').parse().ok()?;

    Some(BoundMemberSelector::TupleElement(index))
}

pub(super) fn reference_target(target: ResolvedName) -> BoundReferenceTarget {
    match target {
        ResolvedName::Local(id) => BoundReferenceTarget::Local(id),
        ResolvedName::Surface(id) => BoundReferenceTarget::Surface(id),
    }
}

pub(super) enum ReferenceResolution {
    Resolved(BoundReferenceTarget),
    Unresolved(BoundUnresolvedReferenceKind, Vec<BoundReferenceTarget>),
}

pub(super) fn classify_reference_result(
    result: MemberLookupResult<ResolvedName>,
) -> ReferenceResolution {
    let result = result.map(reference_target, reference_target);

    match result {
        MemberLookupResult::Found(target) => ReferenceResolution::Resolved(target),
        MemberLookupResult::NotFound => {
            ReferenceResolution::Unresolved(BoundUnresolvedReferenceKind::NotFound, Vec::new())
        }
        MemberLookupResult::WrongKind(candidates) => ReferenceResolution::Unresolved(
            BoundUnresolvedReferenceKind::WrongKind,
            candidates.into_vec(),
        ),
        MemberLookupResult::Ambiguous(candidates) => ReferenceResolution::Unresolved(
            BoundUnresolvedReferenceKind::Ambiguous,
            candidates.into_vec(),
        ),
        MemberLookupResult::Inaccessible(candidates) => ReferenceResolution::Unresolved(
            BoundUnresolvedReferenceKind::Inaccessible,
            candidates.into_vec(),
        ),
        MemberLookupResult::Malformed(candidates) => ReferenceResolution::Unresolved(
            BoundUnresolvedReferenceKind::Malformed,
            candidates.into_vec(),
        ),
    }
}
