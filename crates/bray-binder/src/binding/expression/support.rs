use bray_bound_tree::{
    BoundMemberSelector, BoundOperator, BoundReferenceTarget, BoundStructuredExpressionKind,
    BoundUnresolvedReferenceKind, IterationSourceMode,
};
use bray_symbols::MemberLookupResult;
use bray_syntax::{SourceSyntaxNode, SyntaxKind};

use crate::binding::name::symbol_name;
use crate::lookup::ResolvedName;

pub(super) fn iteration_source_mode(
    syntax: &bray_syntax::IterationSourceSyntax,
) -> IterationSourceMode {
    iteration_source_mode_from_kind(syntax.mode_token().map(|token| token.kind()))
}

const fn iteration_source_mode_from_kind(kind: Option<SyntaxKind>) -> IterationSourceMode {
    match kind {
        Some(SyntaxKind::MutKeyword) => IterationSourceMode::Mutable,
        Some(SyntaxKind::MoveKeyword) => IterationSourceMode::Move,
        Some(_) | None => IterationSourceMode::Shared,
    }
}

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
        SyntaxKind::UnitExpression => BoundStructuredExpressionKind::Unit,
        SyntaxKind::AbsenceExpression => BoundStructuredExpressionKind::Absence,
        SyntaxKind::TupleExpression => BoundStructuredExpressionKind::Tuple,
        SyntaxKind::ConditionalExpression => BoundStructuredExpressionKind::Conditional,
        SyntaxKind::WhileExpression => BoundStructuredExpressionKind::While,
        SyntaxKind::LoopExpression => BoundStructuredExpressionKind::Loop,
        SyntaxKind::WithExpression => BoundStructuredExpressionKind::With,
        SyntaxKind::BorrowExpression => BoundStructuredExpressionKind::Borrow,
        SyntaxKind::TrustBoundaryExpression => BoundStructuredExpressionKind::TrustBoundary,
        SyntaxKind::AssertionExpression => BoundStructuredExpressionKind::Assertion,
        SyntaxKind::ResultPropagationExpression => BoundStructuredExpressionKind::ResultPropagation,
        SyntaxKind::CatchExpression => BoundStructuredExpressionKind::Catch,
        SyntaxKind::TypeFormConstructionExpression => {
            BoundStructuredExpressionKind::TypeFormConstruction
        }
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

#[cfg(test)]
mod tests {
    use bray_bound_tree::IterationSourceMode;
    use bray_syntax::SyntaxKind;

    use super::iteration_source_mode_from_kind;

    #[test]
    fn iteration_source_modes_preserve_shared_mutable_and_move_access() {
        assert_eq!(
            iteration_source_mode_from_kind(None),
            IterationSourceMode::Shared
        );

        assert_eq!(
            iteration_source_mode_from_kind(Some(SyntaxKind::MutKeyword)),
            IterationSourceMode::Mutable
        );

        assert_eq!(
            iteration_source_mode_from_kind(Some(SyntaxKind::MoveKeyword)),
            IterationSourceMode::Move
        );
    }
}
