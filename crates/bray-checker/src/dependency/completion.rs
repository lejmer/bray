use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, CheckedSemanticSelections, SemanticSelection,
};

use super::ValueInputs;
use crate::{CheckerRequestContext, CheckerUnitView};

impl ValueInputs {
    /// Keeps completion dependencies separate from captures for known lazy producers.
    pub(super) fn collect_deferred_sources<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        types: &bray_bound_tree::CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
    ) {
        let unit = request.unit();

        for (expression, node) in unit.tree().expressions() {
            let BoundExpression::Await(awaited) = node else {
                continue;
            };

            if let Some((sources, opaque)) = self.lazy_sources(awaited.operand(), request, types, selections) {
                self.deferred_sources.insert(expression, sources);
                self.expressions.insert(expression, opaque.into_iter().collect());
            }
        }
    }

    /// Unknown producers retain their complete captured contract.
    fn lazy_sources<C: CheckerRequestContext + ?Sized>(
        &self,
        value: BoundExpressionId,
        request: CheckerUnitView<'_, C>,
        types: &bray_bound_tree::CheckedExpressionTypes,
        selections: &CheckedSemanticSelections,
    ) -> Option<(BTreeSet<BoundExpressionId>, BTreeSet<BoundExpressionId>)> {
        let unit = request.unit();
        let mut pending = vec![value];
        let mut visited = BTreeSet::new();
        let mut sources = BTreeSet::new();
        let mut opaque = BTreeSet::new();

        let is_deferred = |value| types.expression(value).is_some_and(|ty| matches!(
            crate::representation::type_representation(request, ty.ty()),
            Some(bray_compiler_known::RepresentationRole::Future | bray_compiler_known::RepresentationRole::Task)
        ));

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
                if matches!(
                    call.implementation_hook(),
                    Some(
                        bray_compiler_known::ImplementationHook::FutureStart
                            | bray_compiler_known::ImplementationHook::TaskJoin
                            | bray_compiler_known::ImplementationHook::TaskCancel
                    )
                ) {
                    pending.push(
                        call.receiver()
                            .expect("task operations retain their consumed receiver")
                            .expression(),
                    );

                    continue;
                }

                if !matches!(
                    call.resolution().result(),
                    bray_bound_tree::BoundCallResult::LazyFuture(_)
                ) {
                    if !is_deferred(value) { return None; }
                    opaque.insert(value);
                } else {
                    sources.insert(value);
                }
            } else if let Some(initializer) = self
                .projected_initializer(value)
                .or_else(|| self.initializers.get(&value).copied())
            {
                pending.push(initializer);
            } else {
                let previous = pending.len();

                pending.extend(self.operands(value));

                if pending.len() == previous && !has_assignment {
                    if !is_deferred(value) { return None; }
                    opaque.insert(value);
                }
            }
        }

        (!sources.is_empty() || !opaque.is_empty()).then_some((sources, opaque))
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
