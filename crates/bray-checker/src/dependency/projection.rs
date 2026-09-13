use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundBlockItem, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, CheckedPatterns, CheckedSemanticSelections,
    ConstructionInputId, SelectedConstructionInput, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, DependencyProjection, LocalBindingSymbolId, SymbolOrdinal};

use super::ValueInputs;

impl ValueInputs {
    pub(super) fn collect_projections(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        patterns: &CheckedPatterns,
    ) {
        self.collect_aggregate_projections(unit, selections);
        let initializers = self.collect_binding_initializers(unit, patterns);
        self.collect_initialized_fields(unit, selections, &initializers);
    }

    fn collect_aggregate_projections(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
    ) {
        for (id, expression) in unit.tree().expressions() {
            if let Some(SemanticSelection::Operation(SelectedOperation::Construction(
                construction,
            ))) = selections.expression(id)
            {
                for input in construction.inputs() {
                    if let SelectedConstructionInput::Explicit {
                        expression, input, ..
                    } = input
                    {
                        let projection = match input {
                            ConstructionInputId::StructField(field) => {
                                DependencyProjection::ProductField(*field)
                            }
                            ConstructionInputId::UnionPayloadField(field) => {
                                DependencyProjection::UnionPayloadField(*field)
                            }
                            ConstructionInputId::CallableParameter(_) => continue,
                        };

                        self.projections.insert((id, projection), *expression);
                    }
                }
            }

            if let BoundExpression::Structured(value) = expression
                && value.kind() == BoundStructuredExpressionKind::Tuple
            {
                for (ordinal, child) in value.operands().iter().enumerate() {
                    if let Ok(ordinal) = u32::try_from(ordinal) {
                        self.projections.insert(
                            (
                                id,
                                DependencyProjection::TupleElement(SymbolOrdinal::new(ordinal)),
                            ),
                            *child,
                        );
                    }
                }
            }
        }
    }

    fn collect_binding_initializers(
        &mut self,
        unit: &BoundUnit,
        patterns: &CheckedPatterns,
    ) -> BTreeMap<LocalBindingSymbolId, BoundExpressionId> {
        let mut initializers = BTreeMap::new();
        let subjects = binding_subjects(unit);

        loop {
            let mut changed = false;

            for (pattern, initializer) in &subjects {
                let mut pending = vec![(*pattern, *initializer)];

                while let Some((id, initializer)) = pending.pop() {
                    let Some(pattern) = unit.tree().pattern(id) else {
                        continue;
                    };

                    let Some(checked) = patterns.pattern(id) else {
                        continue;
                    };

                    let Some(initializer) =
                        self.pattern_initializer(unit, initializer, checked.projection())
                    else {
                        continue;
                    };

                    for local in checked.bindings(pattern) {
                        let Some(ty) = patterns.binding_type(local) else {
                            continue;
                        };

                        let Some(value) = self.pattern_initializer(
                            unit,
                            initializer,
                            binding_projection(pattern, ty),
                        ) else {
                            continue;
                        };

                        initializers.insert(local, value);
                    }

                    pending.extend(pattern.children().iter().map(|child| (*child, initializer)));
                }
            }

            for (id, expression) in unit.tree().expressions() {
                let local = match expression {
                    BoundExpression::Name(name) => match name.target() {
                        BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)) => {
                            Some(binding)
                        }
                        _ => None,
                    },
                    BoundExpression::PatternReference(reference) => Some(reference.binding()),
                    _ => None,
                };

                if let Some(initializer) = local.and_then(|binding| initializers.get(&binding)) {
                    changed |= self.initializers.insert(id, *initializer) != Some(*initializer);
                }
            }

            if !changed {
                break;
            }
        }

        initializers
    }

    fn pattern_initializer(
        &self,
        unit: &BoundUnit,
        mut initializer: BoundExpressionId,
        projection: Option<bray_bound_tree::PatternProjection>,
    ) -> Option<BoundExpressionId> {
        let Some(projection) = projection else {
            return Some(initializer);
        };

        if let bray_bound_tree::PatternProjection::ElementFromStart(index)
        | bray_bound_tree::PatternProjection::ElementFromEnd(index) = projection
        {
            let mut visited = BTreeSet::new();

            while let Some(source) = self.initializers.get(&initializer) {
                if !visited.insert(initializer) {
                    return None;
                }

                initializer = *source;
            }

            let BoundExpression::Structured(value) = unit.tree().expression(initializer)? else {
                return None;
            };

            if value.kind() == BoundStructuredExpressionKind::RepeatedArray {
                return value.operands().first().copied();
            }

            if value.kind() != BoundStructuredExpressionKind::Array {
                return None;
            }

            let index = index.to_index()?;

            let index = if matches!(
                projection,
                bray_bound_tree::PatternProjection::ElementFromEnd(_)
            ) {
                value.operands().len().checked_sub(index.checked_add(1)?)?
            } else {
                index
            };

            return value.operands().get(index).copied();
        }

        let path = [pattern_projection(projection)?];

        let (value, remaining) = self.project(initializer, &path);

        remaining.is_empty().then_some(value)
    }

    fn collect_initialized_fields(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        initializers: &BTreeMap<LocalBindingSymbolId, BoundExpressionId>,
    ) {
        for (id, _) in unit.tree().expressions() {
            let places = super::assignment::value_places(unit, selections, self, id);
            let mut places = places.into_iter();

            let Some((BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)), path)) =
                places.next()
            else {
                continue;
            };

            if places.next().is_some() || path.is_empty() {
                continue;
            }

            let Some(path) = path.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };

            let Some(initializer) = initializers.get(&binding) else {
                continue;
            };

            let (value, remaining) = self.project(*initializer, &path);

            if remaining.is_empty() {
                self.projected_initializers.insert(id, value);
            }
        }
    }

    /// Resolves initialized aggregate fields without treating carried sources as aggregate storage.
    pub(crate) fn project<'a>(
        &self,
        mut expression: BoundExpressionId,
        mut path: &'a [DependencyProjection],
    ) -> (BoundExpressionId, &'a [DependencyProjection]) {
        let mut visited = BTreeSet::new();

        while let Some((projection, rest)) = path.split_first() {
            if !visited.insert((expression, path.len())) {
                break;
            }

            if let Some(child) = self.projections.get(&(expression, *projection)) {
                expression = *child;
                path = rest;
            } else if let Some(initializer) = self.initializers.get(&expression) {
                if self
                    .writes
                    .get(&expression)
                    .into_iter()
                    .flatten()
                    .any(|written| written.project(path).is_some())
                {
                    break;
                }

                expression = *initializer;
            } else {
                break;
            }
        }

        (expression, path)
    }
}

pub(super) fn pattern_projection(
    projection: bray_bound_tree::PatternProjection,
) -> Option<DependencyProjection> {
    use bray_bound_tree::PatternProjection;

    match projection {
        PatternProjection::ProductField(field) => Some(DependencyProjection::ProductField(field)),
        PatternProjection::TupleElement(index) => Some(DependencyProjection::TupleElement(index)),
        PatternProjection::ActiveUnionPayloadField { field, .. } => {
            Some(DependencyProjection::UnionPayloadField(field))
        }
        PatternProjection::NullableValue => Some(DependencyProjection::NullableValue),
        PatternProjection::OwnedTarget => Some(DependencyProjection::OwnedTarget),
        PatternProjection::ElementFromStart(_) | PatternProjection::ElementFromEnd(_) => None,
    }
}

fn binding_subjects(unit: &BoundUnit) -> Vec<(bray_bound_tree::BoundPatternId, BoundExpressionId)> {
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

/// Direct bindings already use the pattern projection; shorthand entries add their own.
pub(super) fn binding_projection(
    pattern: &bray_bound_tree::BoundPattern,
    binding: bray_bound_tree::PatternBindingTypeEntry,
) -> Option<bray_bound_tree::PatternProjection> {
    if pattern.bindings().contains(&binding.binding()) {
        None
    } else {
        binding.projection()
    }
}
