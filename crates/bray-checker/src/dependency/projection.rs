use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundBlockItem, BoundExpression, BoundExpressionId, BoundPatternKind, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, CheckedSemanticSelections, ConstructionInputId,
    SelectedConstructionInput, SelectedOperation, SemanticSelection,
};
use bray_symbols::{AnyLocalSymbolId, DependencyProjection, SymbolOrdinal};

use super::ValueInputs;

impl ValueInputs {
    pub(super) fn collect_projections(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
    ) {
        let mut initializers = BTreeMap::new();

        for (_, block) in unit.tree().blocks() {
            for item in block.items() {
                if let BoundBlockItem::LocalBinding(binding) = item
                    && let Some(pattern) = unit.tree().pattern(binding.pattern())
                    && pattern.kind() == BoundPatternKind::Binding
                    && let [local] = pattern.bindings()
                {
                    initializers.insert(*local, (binding.initializer(), pattern.is_mutable()));
                }
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

            if let Some((initializer, mutable)) =
                local.and_then(|binding| initializers.get(&binding))
            {
                self.initializers.insert(id, *initializer);

                if !mutable {
                    self.aliases.insert(id, *initializer);
                }
            }

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

        for (id, _) in unit.tree().expressions() {
            let Some((BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(binding)), path)) =
                super::assignment::value_place(unit, selections, &self.aliases, id)
            else {
                continue;
            };

            if path.is_empty() {
                continue;
            }

            let Some(path) = path.into_iter().collect::<Option<Vec<_>>>() else {
                continue;
            };

            let Some((initializer, _)) = initializers.get(&binding) else {
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
