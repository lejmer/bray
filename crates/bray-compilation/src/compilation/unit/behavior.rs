use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_bound_tree::{BodyBehaviorContributions, BoundUnitKey};
use bray_checker::{BodyBehaviorCollector, CheckerUnitView, DefaultBodyBehaviorCollector};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::{Compilation, checker::checker_result};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn selected_body_behavior_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<BodyBehaviorContributions>>, FactQueryError> {
        self.unit_query(
            &self.state.selected_body_behavior,
            CompilationFactKey::SelectedBodyBehavior(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let control_flow =
                    self.control_flow_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;
                let semantic = semantic_unit_context(context.symbols(), bound.result().value());
                let request = CheckerUnitView::new(bound.result().value(), &semantic, &context);

                let result =
                    checker_result(DefaultBodyBehaviorCollector.collect_selected_behavior(
                        request,
                        control_flow.result().value(),
                        expressions.result().value().selections(),
                    ))?;

                let (behavior, diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    control_flow.result().diagnostics(),
                    expressions.result().diagnostics(),
                    &diagnostics,
                ]);

                Ok((DiagnosticResult::new(behavior, diagnostics), Box::new([])))
            },
        )
    }
}
