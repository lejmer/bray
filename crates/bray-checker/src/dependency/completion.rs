use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundUnit, CheckedSemanticSelections, SemanticSelection,
};

use super::ValueInputs;

impl ValueInputs {
    /// Keeps completion dependencies separate from captures for known lazy producers.
    pub(super) fn collect_deferred_sources(
        &mut self,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
    ) {
        for (expression, node) in unit.tree().expressions() {
            let BoundExpression::Await(awaited) = node else {
                continue;
            };

            if let Some(sources) = self.lazy_sources(awaited.operand(), unit, selections) {
                self.deferred_sources.insert(expression, sources);
                self.expressions.insert(expression, Vec::new());
            }
        }
    }

    /// Unknown producers retain their complete captured contract.
    fn lazy_sources(
        &self,
        value: BoundExpressionId,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
    ) -> Option<BTreeSet<BoundExpressionId>> {
        let mut pending = vec![value];
        let mut visited = BTreeSet::new();
        let mut sources = BTreeSet::new();

        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }

            let assigned = self
                .writes
                .values(value)
                .map(|assigned| assigned.value)
                .collect::<Vec<_>>();

            let has_assignment = !assigned.is_empty();

            pending.extend(assigned);

            if matches!(unit.view().expression(value), Some(BoundExpression::Structured(value)) if value.kind() == bray_bound_tree::BoundStructuredExpressionKind::Absence)
            {
                continue;
            }

            if let Some(SemanticSelection::Call(call)) = selections.expression(value) {
                if !matches!(
                    call.resolution().result(),
                    bray_bound_tree::BoundCallResult::LazyFuture(_)
                ) {
                    return None;
                }

                sources.insert(value);
            } else if let Some(initializer) = self
                .projected_initializer(value)
                .or_else(|| self.initializers.get(&value).copied())
            {
                pending.push(initializer);
            } else {
                let previous = pending.len();

                pending.extend(self.operands(value));

                if pending.len() == previous && !has_assignment {
                    return None;
                }
            }
        }

        (!sources.is_empty()).then_some(sources)
    }

    pub(crate) fn deferred_sources(
        &self,
    ) -> impl Iterator<Item = (BoundExpressionId, BoundExpressionId)> {
        self.deferred_sources
            .iter()
            .flat_map(|(expression, sources)| sources.iter().map(|source| (*expression, *source)))
    }

    pub(super) fn completion_sources(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&BTreeSet<BoundExpressionId>> {
        self.deferred_sources.get(&expression)
    }
}
