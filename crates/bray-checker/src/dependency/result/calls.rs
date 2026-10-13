use crate::{CheckerQueryError, CheckerRequestContext};
use bray_bound_tree::{BoundExpressionId, SemanticSelection};
use bray_symbols::{DependencyRequirement, DependencyRequirementKind};
use std::collections::BTreeSet;

use super::core::Transfer;

impl<C: CheckerRequestContext + ?Sized> Transfer<'_, '_, C> {
    pub(super) fn call_values(
        &self,
        expression: BoundExpressionId,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let Some(SemanticSelection::Call(call)) = self.selections.expression(expression) else {
            return Ok(BTreeSet::new());
        };

        let template = self.selected_result_template(call)?;

        let mut result = BTreeSet::new();

        if crate::dependency::opaque_result(call)
            && let Some(bray_bound_tree::BoundExpression::Call(bound)) =
                self.request.view().expression(expression)
        {
            result.extend(
                self.state
                    .values
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

        result.extend(self.mapped_result_values(
            template.template.requirements(),
            |root| crate::dependency::result_argument(call, root),
            borrowed_receiver.then_some(bray_symbols::DependencySubjectRoot::Receiver),
        )?);

        Ok(result)
    }

    pub(super) fn selected_result_template(
        &self,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<
        crate::dependency::defaults::CallResultDependencies,
        CheckerQueryError<C::UpstreamError>,
    > {
        let recursive = match call.target() {
            bray_bound_tree::BoundCallableTarget::Declaration(instance) => {
                self.request
                    .is_recursive_result(instance.definition().callable_symbol())
                    && call.resolution().trait_dispatch().is_none()
            }
            _ => false,
        };

        if recursive {
            let template = crate::dependency::witness::deferred_result(self.request, call, None)?;

            crate::dependency::defaults::expand_result_defaults(self.request, call, &template)
        } else {
            crate::dependency::call_result_template(self.request, call, |callable| {
                self.request.callable_result_dependencies(callable)
            })
        }
    }

    pub(super) fn scoped_values(
        &self,
        scoped: &bray_bound_tree::SelectedScopedUse,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let instance = scoped.enter().0;
        let symbol = instance.definition().callable_symbol();

        let template = if self.request.is_recursive_result(symbol) {
            let callable = self
                .request
                .semantic_values()
                .intern_callable_instance(instance).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in scoped_values: {error:?}"));

            let root = bray_symbols::DependencySubjectRoot::Receiver;

            let requirements = bray_symbols::DependencyContractTemplateData::new([
                DependencyRequirement::result_call(
                    callable,
                    None,
                    [bray_symbols::DependencyCallInput::new(
                        root,
                        [DependencyRequirement::direct(
                            bray_symbols::DependencySubject::root(root),
                            DependencyRequirementKind::ValueDependencies,
                        )],
                        [DependencyRequirement::direct(
                            bray_symbols::DependencySubject::root(root),
                            DependencyRequirementKind::StorageAlive,
                        )],
                    )],
                ),
            ]);

            requirements
        } else {
            let template = self.request.callable_result_dependencies(symbol)?;

            crate::dependency::returned::resolved_result_template(
                self.request,
                Some(instance),
                template,
            )?
        };

        let borrowed = matches!(
            self.request
                .semantic_values()
                .type_data(scoped.source_type())
                .as_ref(),
            bray_symbols::TypeData::Borrow { .. }
        );

        self.mapped_result_values(
            template.requirements(),
            |root| {
                (root == bray_symbols::DependencySubjectRoot::Receiver)
                    .then_some(scoped.initializer())
            },
            borrowed.then_some(bray_symbols::DependencySubjectRoot::Receiver),
        )
    }

    fn mapped_result_values(
        &self,
        requirements: &[DependencyRequirement],
        argument: impl Fn(bray_symbols::DependencySubjectRoot) -> Option<BoundExpressionId>,
        borrowed: Option<bray_symbols::DependencySubjectRoot>,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        Ok(
            crate::dependency::witness::map_requirements(requirements, &mut |subject, kind| {
                let Some(argument) = argument(subject.subject_root()) else {
                    return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
                };

                if kind == DependencyRequirementKind::ValueDependencies
                    || borrowed == Some(subject.subject_root())
                {
                    return Ok(self
                        .projected_values(argument, subject.projections())
                        .into_iter()
                        .collect());
                }

                let sources = self.state.sources.get(&argument);

                if sources.is_none_or(BTreeSet::is_empty) {
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
                            super::sources::source_subject(
                                source,
                                subject.projections().iter().copied(),
                            ),
                            kind,
                        )
                    })
                    .collect())
            }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in mapped_result_values: {error:?}"))
            .into_iter()
            .collect(),
        )
    }
}
