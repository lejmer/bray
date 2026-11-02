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
        let mutable = unit
            .tree()
            .patterns()
            .filter(|(_, pattern)| pattern.is_mutable())
            .flat_map(|(_, pattern)| pattern.bindings().iter().copied())
            .collect::<BTreeSet<_>>();

        for (expression, node) in unit.tree().expressions() {
            let BoundExpression::Await(awaited) = node else {
                continue;
            };

            if let Some(source) = self.lazy_source(awaited.operand(), unit, selections, &mutable) {
                self.deferred_sources.insert(expression, source);
                self.expressions.insert(expression, Vec::new());
            }
        }
    }

    /// Unknown or reassigned future values retain their complete captured contract.
    fn lazy_source(
        &self,
        value: BoundExpressionId,
        unit: &BoundUnit,
        selections: &CheckedSemanticSelections,
        mutable: &BTreeSet<bray_symbols::LocalBindingSymbolId>,
    ) -> Option<BoundExpressionId> {
        let mut pending = vec![value];
        let mut visited = BTreeSet::new();
        let mut sources = BTreeSet::new();

        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }

            if self.writes.values(value).next().is_some() {
                return None;
            }

            if super::assignment::value_places(unit, selections, self, value).iter().any(|(root, _)| {
                matches!(root, bray_bound_tree::BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(binding)) if mutable.contains(binding))
            }) {
                return None;
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

                if pending.len() == previous {
                    return None;
                }
            }
        }

        (sources.len() == 1).then(|| {
            sources
                .into_iter()
                .next()
                .expect("one lazy producer was established")
        })
    }

    pub(crate) fn deferred_sources(
        &self,
    ) -> impl Iterator<Item = (BoundExpressionId, BoundExpressionId)> {
        self.deferred_sources
            .iter()
            .map(|(expression, source)| (*expression, *source))
    }

    pub(super) fn completion_source(
        &self,
        expression: BoundExpressionId,
    ) -> Option<BoundExpressionId> {
        self.deferred_sources.get(&expression).copied()
    }
}
