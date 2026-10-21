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

        result.extend(
            self.mapped_result_values(template.template.requirements(), |root| {
                crate::dependency::result_argument(call.arguments(), call.receiver(), root)
            })?,
        );

        Ok(result)
    }

    pub(super) fn selected_result_template(
        &self,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<
        crate::dependency::defaults::CallResultDependencies,
        CheckerQueryError<C::UpstreamError>,
    > {
        let recursive = call.resolution().declaration_source().is_some_and(|instance| {
            self.request
                .is_recursive_result(instance.definition().callable_symbol())
                && call.resolution().trait_dispatch().is_none()
        });

        if recursive {
            let template = crate::dependency::witness::deferred_result(self.request, call, None)?;

            crate::dependency::defaults::expand_result_defaults(
                self.request,
                call.arguments(),
                call.receiver(),
                call.resolution().declaration_source(),
                &template,
            )
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
                    bray_symbols::DependencyCallOutput::Value,
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

        self.mapped_result_values(template.requirements(), |root| {
            (root == bray_symbols::DependencySubjectRoot::Receiver).then_some(scoped.initializer())
        })
    }

    pub(crate) fn exit_values(
        &self,
        scoped: &bray_bound_tree::SelectedScopedUse,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let instance = scoped.exit().0;

        let template = self
            .request
            .callable_result_dependencies(instance.definition().callable_symbol())?;

        let template = crate::dependency::returned::resolved_result_template(
            self.request,
            Some(instance),
            template,
        )?;

        self.map_exit_values(scoped, template.requirements())
    }

    fn map_exit_values(
        &self,
        scoped: &bray_bound_tree::SelectedScopedUse,
        requirements: &[DependencyRequirement],
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let input = self.scoped_values(scoped)?;

        Ok(
            crate::dependency::witness::map_requirements(requirements, &mut |subject, kind| {
                if subject.subject_root()
                    != bray_symbols::DependencySubjectRoot::Parameter(
                        bray_symbols::SymbolOrdinal::new(0),
                    )
                {
                    return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
                }

                if kind == DependencyRequirementKind::ValueDependencies {
                    // The returned capability's dependencies already describe the entry result.
                    // Keeping the whole set is conservative when its payload has several fields.
                    Ok(input.iter().cloned().collect())
                } else {
                    Ok(vec![DependencyRequirement::direct(
                        bray_symbols::DependencySubject::root(
                            bray_symbols::DependencySubjectRoot::EvaluationStorage,
                        ),
                        kind,
                    )])
                }
            })
            .expect("exit requirements retain checked variable binding")
            .into_iter()
            .collect(),
        )
    }

    pub(crate) fn report_values(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        output: bray_bound_tree::CallReportOutput,
    ) -> Result<Option<BTreeSet<DependencyRequirement>>, CheckerQueryError<C::UpstreamError>> {
        use bray_bound_tree::BoundExecutionSite;

        match occurrence {
            BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(expression)) => {
                if let Some(SemanticSelection::Operation(operation)) =
                    self.selections.expression(expression)
                {
                    let template = crate::dependency::call::operation_result_dependencies(
                        self.request,
                        expression,
                        operation,
                    )?
                    .expect("report-producing operator retains its source invocation");

                    let reports = template.immediate_reports(false);

                    let requirements = if output.is_abnormal() {
                        reports.abnormal_reports()
                    } else {
                        reports.completion_reports()
                    };

                    return requirements
                        .map(|requirements| {
                            self.operation_values(expression, operation, requirements)
                        })
                        .transpose();
                }

                let Some(SemanticSelection::Call(call)) = self.selections.expression(expression)
                else {
                    panic!("report-producing invocation retains its selected call");
                };

                let template = self.selected_result_template(call)?;

                let reports = if matches!(
                    output,
                    bray_bound_tree::CallReportOutput::DeferredCompleted
                        | bray_bound_tree::CallReportOutput::DeferredAbnormal
                ) {
                    template.template
                } else {
                    template.immediate_reports(matches!(
                        call.resolution().result(),
                        bray_bound_tree::BoundCallResult::LazyFuture(_)
                    ))
                };

                let requirements = if output.is_abnormal() {
                    reports.abnormal_reports()
                } else {
                    reports.completion_reports()
                };

                requirements
                    .map(|requirements| {
                        self.mapped_result_values(requirements, |root| {
                            crate::dependency::result_argument(
                                call.arguments(),
                                call.receiver(),
                                root,
                            )
                        })
                    })
                    .transpose()
            }
            BoundExecutionSite::ScopedEnter(expression)
            | BoundExecutionSite::ScopedExit(expression) => {
                let Some(SemanticSelection::ScopedUse(scoped)) =
                    self.selections.expression(expression)
                else {
                    panic!("report-producing scope retains its selected invocation");
                };

                let instance = scoped.invocation(occurrence).0;

                let template = self
                    .request
                    .callable_result_dependencies(instance.definition().callable_symbol())?;

                let template = crate::dependency::returned::resolved_result_template(
                    self.request,
                    Some(instance),
                    template,
                )?;

                let requirements = if output.is_abnormal() {
                    template.abnormal_reports()
                } else {
                    template.completion_reports()
                };

                requirements
                    .map(|requirements| {
                        let mapped = if matches!(occurrence, BoundExecutionSite::ScopedExit(_)) {
                            self.map_exit_values(scoped, requirements)
                        } else {
                            self.mapped_result_values(requirements, |root| {
                                (root == bray_symbols::DependencySubjectRoot::Receiver)
                                    .then_some(scoped.initializer())
                            })
                        };

                        mapped
                    })
                    .transpose()
            }
            _ => panic!("report-producing operation has a selected invocation: {occurrence:?}"),
        }
    }

    pub(super) fn operation_values(
        &self,
        expression: BoundExpressionId,
        operation: &bray_bound_tree::SelectedOperation,
        requirements: &[DependencyRequirement],
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let inputs =
            crate::dependency::call::operation_inputs(self.request.view(), expression, operation);

        Ok(
            crate::dependency::witness::map_requirements(requirements, &mut |subject, kind| {
                let Some((_, input)) = inputs
                    .iter()
                    .find(|(root, _)| *root == subject.subject_root())
                else {
                    return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
                };

                match *input {
                    crate::dependency::call::OperationInput::Borrow(operand, _)
                        if kind == DependencyRequirementKind::ValueDependencies
                            && !subject.projections().is_empty() =>
                    {
                        self.mapped_input_values(operand, subject.projections(), kind)
                    }
                    crate::dependency::call::OperationInput::Borrow(operand, _) => Ok(self
                        .borrowed_values(operand, subject.projections())
                        .into_iter()
                        .collect()),
                    crate::dependency::call::OperationInput::Value(operand)
                    | crate::dependency::call::OperationInput::Nullable(Some(operand)) => {
                        self.mapped_input_values(operand, subject.projections(), kind)
                    }
                    crate::dependency::call::OperationInput::Nullable(None) => Ok(Vec::new()),
                }
            })
            .expect("operation inputs preserve checked dependency equation bindings")
            .into_iter()
            .collect(),
        )
    }

    fn mapped_result_values(
        &self,
        requirements: &[DependencyRequirement],
        argument: impl Fn(bray_symbols::DependencySubjectRoot) -> Option<BoundExpressionId>,
    ) -> Result<BTreeSet<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        Ok(
            crate::dependency::witness::map_requirements(requirements, &mut |subject, kind| {
                let Some(argument) = argument(subject.subject_root()) else {
                    return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
                };

                self.mapped_input_values(argument, subject.projections(), kind)
            }).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in mapped_result_values: {error:?}"))
            .into_iter()
            .collect(),
        )
    }
    fn mapped_input_values(
        &self,
        argument: BoundExpressionId,
        path: &[bray_symbols::DependencyProjection],
        kind: DependencyRequirementKind,
    ) -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError> {
        // A supplied borrow retains its referent and original loan dependencies.
        // Requiring the slot containing that borrow would invent a caller loan,
        // especially when the containing value is consumed.
        if kind == DependencyRequirementKind::ValueDependencies
            || self.types.expression(argument).is_some_and(|ty| {
                matches!(
                    self.request.semantic_values().type_data(ty.ty()).as_ref(),
                    bray_symbols::TypeData::Borrow { .. }
                )
            })
        {
            return Ok(self.projected_values(argument, path).into_iter().collect());
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
                    super::sources::source_subject(source, path.iter().copied()),
                    kind,
                )
            })
            .collect())
    }
}
