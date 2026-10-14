use bray_bound_tree::{BoundBlockItem, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit};

pub(crate) fn binding_subjects(
    unit: &BoundUnit,
) -> Vec<(bray_bound_tree::BoundPatternId, BoundExpressionId)> {
    let mut subjects = Vec::new();

    for (_, block) in unit.tree().blocks() {
        for item in block.items() {
            if let BoundBlockItem::LocalBinding(binding) = item {
                subjects.push((binding.pattern(), binding.initializer()));
            }
        }
    }

    for (_, expression) in unit.tree().expressions() {
        match expression {
            BoundExpression::Structured(value)
                if value.kind() == BoundStructuredExpressionKind::PatternBinding =>
            {
                if let Some(subject) = value.operands().first() {
                    subjects.extend(value.patterns().iter().map(|pattern| (*pattern, *subject)));
                }
            }
            BoundExpression::For(value) => subjects.push((value.pattern(), value.source())),
            BoundExpression::Generator(value) => subjects.push((value.pattern(), value.source())),
            BoundExpression::Match(value) => {
                subjects.extend(
                    value
                        .arms()
                        .iter()
                        .map(|arm| (arm.pattern(), value.subject())),
                );
            }
            _ => {}
        }
    }

    subjects
}

