use super::super::state::ExecutionState;
use super::model::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition};
use bray_bound_tree::{Refinement, RefinementKind};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn assume_patterns<'a>(
        &self,
        state: &mut ExecutionState,
        refinements: impl Iterator<Item = &'a Refinement>,
    ) {
        let mut changed = false;

        for refinement in refinements {
            let RefinementKind::Pattern {
                subject,
                predicate,
                access,
                value,
                ..
            } = refinement.kind()
            else {
                continue;
            };

            let ty = self
                .storage
                .access(access)
                .expect("checked pattern retains its subject")
                .reached_type();

            let Some((operator, expected)) = crate::execution_guarantees::pattern_value(
                self.request.semantic_values(),
                ty,
                predicate,
            ) else {
                continue;
            };

            ExecutionCondition::pattern(
                self.storage_value(state, subject, access),
                operator,
                expected,
            )
            .assume(value, &mut state.assumptions);

            changed = true;
        }

        if changed {
            state.refine_witnesses();
        }
    }
}
