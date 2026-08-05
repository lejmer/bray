use bray_source::TextRange;
use bray_syntax::{
    BlockExpressionSyntax, ConditionalExpressionSyntax, ExpressionSyntax, SourceSyntaxNode,
    SourceUnitSyntax, SyntaxKind, SyntaxWalkControl, SyntaxWalkEvent, syntax_node_view,
    walk_source_unit,
};

pub(super) fn simplify_nested_conditionals(source_unit: &SourceUnitSyntax) -> Option<String> {
    let mut replacements = Vec::new();

    walk_source_unit(source_unit, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        let Some(conditional) = node.cast::<ConditionalExpressionSyntax>() else {
            return SyntaxWalkControl::Continue;
        };

        let Some(replacement) = conditional_replacement(&conditional) else {
            return SyntaxWalkControl::Continue;
        };

        replacements.push((conditional.full_range(), replacement));

        SyntaxWalkControl::SkipChildren
    });

    if replacements.is_empty() {
        return None;
    }

    let mut rewritten = source_unit.source().text().to_owned();

    for (range, replacement) in replacements.into_iter().rev() {
        rewritten.replace_range(byte_range(range), &replacement);
    }

    Some(rewritten)
}

fn conditional_replacement(outer: &ConditionalExpressionSyntax) -> Option<String> {
    let source = outer.source();
    let mut conditions = Vec::new();

    push_condition(outer, &mut conditions)?;

    let mut conditional = sole_nested_conditional(&outer.block_expression())?;

    loop {
        push_condition(&conditional, &mut conditions)?;

        let block = conditional.block_expression();

        match sole_nested_conditional(&block) {
            Some(nested) => conditional = nested,
            None => {
                let block = source.text_slice(block.full_range())?.trim();

                return Some(format!("if {} {block}", conditions.join(" && ")));
            }
        }
    }
}

fn push_condition(
    conditional: &ConditionalExpressionSyntax,
    conditions: &mut Vec<String>,
) -> Option<()> {
    if conditional.conditional_else().is_some() || contains_comment(conditional) {
        return None;
    }

    let condition = conditional.condition_expression()?;

    if !is_simple_condition(&condition) {
        return None;
    }

    conditions.push(
        conditional
            .source()
            .text_slice(condition.full_range())?
            .trim()
            .to_owned(),
    );

    Some(())
}

fn sole_nested_conditional(block: &BlockExpressionSyntax) -> Option<ConditionalExpressionSyntax> {
    let mut items = block.block_items();
    let item = items.next()?;

    if items.next().is_some() {
        return None;
    }

    let expression = item.block_shaped_expression()?;

    if !expression.is_block_shaped() {
        return None;
    }

    expression
        .primary_expression()?
        .conditional_expressions()
        .next()
}

fn is_simple_condition(condition: &ExpressionSyntax) -> bool {
    let mut saw_value = false;

    for token in condition.tokens() {
        match token.kind() {
            SyntaxKind::IdentifierToken
            | SyntaxKind::SelfValueKeyword
            | SyntaxKind::TrueKeyword
            | SyntaxKind::FalseKeyword => saw_value = true,
            SyntaxKind::DotToken => {}
            _ => return false,
        }
    }

    saw_value
}

fn contains_comment(node: &ConditionalExpressionSyntax) -> bool {
    syntax_node_view(node).tokens().any(|token| {
        token
            .leading_trivia()
            .iter()
            .chain(token.trailing_trivia())
            .any(|trivia| trivia.kind() != SyntaxKind::WhitespaceTrivia)
    })
}

fn byte_range(range: TextRange) -> std::ops::Range<usize> {
    range.start().bytes() as usize..range.end().bytes() as usize
}
