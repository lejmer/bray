use bray_bound_tree::{
    BoundBlockItem, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
};

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

/// Only whole immutable declaration bindings preserve their initializer's exact value.
pub(crate) fn immutable_initializers(
    unit: &bray_bound_tree::BoundUnit,
) -> std::collections::BTreeMap<
    bray_symbols::LocalBindingSymbolId,
    bray_bound_tree::BoundExpressionId,
> {
    binding_subjects(unit)
        .into_iter()
        .filter_map(|(id, initializer)| {
            let pattern = unit
                .tree()
                .pattern(id)
                .expect("bound subject retains its pattern");

            if pattern.mode() != bray_bound_tree::BoundPatternMode::Declaration
                || pattern.kind() != bray_bound_tree::BoundPatternKind::Binding
                || pattern.is_mutable()
            {
                return None;
            }

            let [binding] = pattern.bindings() else {
                return None;
            };

            Some((*binding, initializer))
        })
        .collect()
}
