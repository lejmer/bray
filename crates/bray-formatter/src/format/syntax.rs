use bray_syntax::{BlockExpressionSyntax, MatchArmSyntax, SyntaxKind, SyntaxNodeView};

use super::context::is_callable_declaration;

pub(super) fn block_expression_is_empty(node: SyntaxNodeView<'_>) -> bool {
    let Some(block) = node.cast::<BlockExpressionSyntax>() else {
        return false;
    };

    if block.block_items().next().is_some() || block.skipped_syntax().next().is_some() {
        return false;
    }

    [block.open_brace_token(), block.close_brace_token()]
        .into_iter()
        .all(|token| {
            token.is_present()
                && token
                    .leading_trivia()
                    .iter()
                    .chain(token.trailing_trivia())
                    .all(|trivia| trivia.kind() == SyntaxKind::WhitespaceTrivia)
        })
}

pub(super) fn compact_match_arm_body_is_empty(node: SyntaxNodeView<'_>) -> Option<bool> {
    let arm = node.cast::<MatchArmSyntax>()?;
    let body = arm.block_expression();
    let mut items = body.block_items();

    let Some(item) = items.next() else {
        return Some(true);
    };

    if items.next().is_some()
        || item.expression().is_none() && item.generator_iteration_expression().is_none()
    {
        return None;
    }

    Some(false)
}

pub(super) const fn is_declaration_clause_keyword(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::RequiresKeyword
            | SyntaxKind::EnsuresKeyword
            | SyntaxKind::ExecutesKeyword
            | SyntaxKind::WhenKeyword
            | SyntaxKind::WithKeyword
            | SyntaxKind::UsesKeyword
    )
}

pub(super) fn is_callable_declaration_header(nodes: &[SyntaxKind]) -> bool {
    nodes
        .iter()
        .rev()
        .skip(1)
        .copied()
        .find(|kind| {
            is_callable_declaration(*kind)
                || matches!(
                    kind,
                    SyntaxKind::ParameterList
                        | SyntaxKind::CallableBodyBlockExpression
                        | SyntaxKind::BlockExpression
                        | SyntaxKind::LambdaExpression
                )
        })
        .is_some_and(is_callable_declaration)
}

pub(super) const fn is_open_delimiter(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::OpenParenToken | SyntaxKind::OpenBracketToken | SyntaxKind::LessToken
    )
}

pub(super) const fn is_close_delimiter(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::CloseParenToken | SyntaxKind::CloseBracketToken | SyntaxKind::GreaterToken
    )
}
