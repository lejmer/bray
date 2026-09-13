use crate::{CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext};
use bray_bound_tree::{BoundExpressionId, SemanticSelection};
use bray_symbols::{DependencyRequirement, DependencyRequirementKind};
use std::collections::BTreeSet;

use super::core::ResultInference;

impl<C: CheckerRequestContext + ?Sized> ResultInference<'_, C> {
    pub(super) fn call_values(
        &self,
        expression: BoundExpressionId,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let Some(SemanticSelection::Call(call)) = self.selections.expression(expression) else {
            return Ok(BTreeSet::new());
        };

        let template = crate::dependency::call_result_template(self.request, call, |callable| {
            self.callees
                .get(&callable)
                .copied()
                .ok_or_else(|| CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
        })?;

        let mut result = BTreeSet::new();

        if crate::dependency::opaque_result(call)
            && let Some(bray_bound_tree::BoundExpression::Call(bound)) =
                self.request.view().expression(expression)
        {
            result.extend(
                self.values
                    .get(&bound.callee())
                    .into_iter()
                    .flatten()
                    .cloned(),
            );
        }

        for requirement in template.template.requirements() {
            let DependencyRequirement::Direct { subject, kind } = requirement else {
                continue;
            };

            let argument = crate::dependency::result_argument(call, subject.subject_root());

            let Some(argument) = argument else {
                // The output template owns these non-argument roots independently of the callee.
                result.insert(requirement.clone());

                continue;
            };

            if *kind == DependencyRequirementKind::ValueDependencies {
                result.extend(self.projected_values(argument, subject.projections()));
            } else {
                for source in self.sources.get(&argument).into_iter().flatten() {
                    let source = super::sources::normalized_subject(
                        source.subject_root(),
                        source
                            .projections()
                            .iter()
                            .chain(subject.projections())
                            .copied(),
                    );

                    result.insert(DependencyRequirement::direct(source, *kind));
                }
            }
        }

        Ok(result)
    }
}
