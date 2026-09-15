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

        let recursive = match call.target() {
            bray_bound_tree::BoundCallableTarget::Declaration(instance) => {
                self.recursive_callees
                    .contains(&instance.definition().callable_symbol())
                    && call.resolution().trait_dispatch().is_none()
            }
            _ => false,
        };

        let template = if recursive {
            let template = crate::dependency::witness::deferred_result(self.request, call, None)?;

            crate::dependency::defaults::expand_result_defaults(self.request, call, &template)?
        } else {
            crate::dependency::call_result_template(self.request, call, |callable| {
                self.callees
                    .get(&callable)
                    .copied()
                    .ok_or_else(|| CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
            })?
        };

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

        let borrowed_receiver = call
            .receiver()
            .and_then(|receiver| self.types.expression(receiver.expression()))
            .map(|ty| self.request.semantic_values().type_data(ty.ty()))
            .is_some_and(|ty| matches!(ty.as_ref(), bray_symbols::TypeData::Borrow { .. }));

        result.extend(
            crate::dependency::witness::map_requirements(
                template.template.requirements(),
                &mut |subject, kind| {
                    let Some(argument) =
                        crate::dependency::result_argument(call, subject.subject_root())
                    else {
                        return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
                    };

                    if kind == DependencyRequirementKind::ValueDependencies
                        || borrowed_receiver
                            && subject.subject_root()
                                == bray_symbols::DependencySubjectRoot::Receiver
                    {
                        return Ok(self
                            .projected_values(argument, subject.projections())
                            .into_iter()
                            .collect());
                    }

                    let sources = self.sources.get(&argument);

                    if sources.is_none_or(BTreeSet::is_empty) {
                        if !self.include_evaluation_storage {
                            return Ok(Vec::new());
                        }

                        return Ok(vec![DependencyRequirement::direct(
                            bray_symbols::DependencySubject::root(
                                bray_symbols::DependencySubjectRoot::EvaluationStorage,
                            ),
                            kind,
                        )]);
                    }

                    Ok(sources
                        .into_iter()
                        .flatten()
                        .map(|source| {
                            DependencyRequirement::direct(
                                super::sources::normalized_subject(
                                    source.subject_root(),
                                    source
                                        .projections()
                                        .iter()
                                        .chain(subject.projections())
                                        .copied(),
                                ),
                                kind,
                            )
                        })
                        .collect())
                },
            )
            .map_err(CheckerInfrastructureError::SemanticValueStore)?,
        );

        Ok(result)
    }
}
