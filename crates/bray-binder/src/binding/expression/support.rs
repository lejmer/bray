use bray_bound_tree::{
    BoundMemberSelector, BoundOperator, BoundStructuredExpressionKind, BoundValueTarget,
};
use bray_declarations::SyntaxAnchor;
use bray_syntax::{
    SourceSyntaxNode, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    SyntaxWalkRoot, walk_syntax_node,
};

use crate::BinderFactContext;
use crate::binding::name::symbol_name;
use crate::lookup::ResolvedValueName;
use crate::request::{BinderRequestContext, ControlTarget, ControlTargetKind};

pub(super) fn visit_direct_nodes(
    node: &impl SyntaxWalkRoot,
    mut visitor: impl for<'syntax> FnMut(SyntaxNodeView<'syntax>) -> SyntaxWalkControl,
) {
    let mut stack_depth = 0_usize;

    walk_syntax_node(node, |event| match event {
        SyntaxWalkEvent::EnterNode(node) => {
            if stack_depth == 1 {
                stack_depth += 1;

                if visitor(node) == SyntaxWalkControl::Stop {
                    return SyntaxWalkControl::Stop;
                }

                return SyntaxWalkControl::SkipChildren;
            }

            stack_depth += 1;

            SyntaxWalkControl::Continue
        }
        SyntaxWalkEvent::ExitNode(_) => {
            stack_depth = stack_depth.saturating_sub(1);

            SyntaxWalkControl::Continue
        }
        SyntaxWalkEvent::Token(_) => SyntaxWalkControl::Continue,
    });
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
        SyntaxKind::LiteralExpression => BoundStructuredExpressionKind::Literal,
        SyntaxKind::UnitExpression => BoundStructuredExpressionKind::Unit,
        SyntaxKind::AbsenceExpression => BoundStructuredExpressionKind::Absence,
        SyntaxKind::LeadingDotVariantExpression => BoundStructuredExpressionKind::MemberAccess,
        SyntaxKind::TupleExpression => BoundStructuredExpressionKind::Tuple,
        SyntaxKind::ArrayExpression => BoundStructuredExpressionKind::Array,
        SyntaxKind::StructConstructionBody => BoundStructuredExpressionKind::StructConstruction,
        SyntaxKind::GeneralGeneratorExpression => BoundStructuredExpressionKind::Generator,
        SyntaxKind::ConditionalExpression => BoundStructuredExpressionKind::Conditional,
        SyntaxKind::MatchExpression => BoundStructuredExpressionKind::Match,
        SyntaxKind::WhileExpression => BoundStructuredExpressionKind::While,
        SyntaxKind::ForExpression => BoundStructuredExpressionKind::For,
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
        SyntaxKind::SpawnExpression => BoundStructuredExpressionKind::Spawn,
        SyntaxKind::YieldExpression => BoundStructuredExpressionKind::Yield,
        SyntaxKind::ReturnExpression => BoundStructuredExpressionKind::Return,
        SyntaxKind::BreakExpression => BoundStructuredExpressionKind::Break,
        SyntaxKind::ContinueExpression => BoundStructuredExpressionKind::Continue,
        SyntaxKind::PanicExpression => BoundStructuredExpressionKind::Panic,
        _ => return None,
    })
}

pub(super) fn member_selector(syntax: SyntaxNodeView<'_>) -> Option<BoundMemberSelector> {
    if let Some(variant) = syntax.cast::<bray_syntax::LeadingDotVariantExpressionSyntax>() {
        return symbol_name(variant.source(), &variant.identifier_token())
            .map(BoundMemberSelector::Name);
    }

    let operation = syntax.cast::<bray_syntax::MemberAccessOperationSyntax>()?;

    if let Some(token) = operation.identifier_token() {
        return symbol_name(operation.source(), &token).map(BoundMemberSelector::Name);
    }

    let token = operation.tuple_element_index_token()?;
    let text = token.text(operation.source().text())?;
    let index = text.trim_start_matches('.').parse().ok()?;

    Some(BoundMemberSelector::TupleElement(index))
}

pub(super) fn control_transfer_target<C>(
    request: &BinderRequestContext<'_, C>,
    kind: BoundStructuredExpressionKind,
) -> Option<SyntaxAnchor>
where
    C: BinderFactContext + ?Sized,
{
    let target_kind = match kind {
        BoundStructuredExpressionKind::Yield => ControlTargetKind::Block,
        BoundStructuredExpressionKind::Return => ControlTargetKind::Callable,
        BoundStructuredExpressionKind::Break | BoundStructuredExpressionKind::Continue => {
            ControlTargetKind::Loop
        }
        _ => return None,
    };

    request
        .control_target_of_kind(target_kind)
        .map(ControlTarget::syntax)
}

pub(super) fn value_target(target: ResolvedValueName) -> BoundValueTarget {
    match target {
        ResolvedValueName::Local(id) => BoundValueTarget::Local(id),
        ResolvedValueName::GenericConstParameter(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::CallableParameter(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::PredicateParameter(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::ReceiverParameter(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::Constant(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::Function(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::Predicate(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::CallableOverload(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::StructField(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::UnionVariant(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::UnionPayloadField(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TypeCallableMember(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::Constructor(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TraitCallableMember(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TraitConstantMember(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TraitPredicateMember(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TraitCallableFulfillment(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TraitConstantFulfillment(id) => BoundValueTarget::Surface(id.into()),
        ResolvedValueName::TraitPredicateFulfillment(id) => BoundValueTarget::Surface(id.into()),
    }
}
