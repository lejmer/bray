use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};
use bray_bound_tree::{BoundCallableTarget, SelectedArgument, SelectedCall};
use bray_symbols::{
    CallableSymbolId, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal,
};

pub(crate) fn opaque_result(call: &SelectedCall) -> bool {
    call.implementation_hook()
        .and_then(super::implementation_dependency_source)
        .is_none()
        && !matches!(call.target(), BoundCallableTarget::Declaration(_))
}

/// Applies the selected implementation, generic substitution and parameter defaults once.
pub(crate) fn call_result_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    declaration: impl FnOnce(
        CallableSymbolId,
    ) -> Result<
        DependencyContractTemplateId,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<super::defaults::CallResultDependencies, CheckerQueryError<C::UpstreamError>> {
    let store = request.semantic_values();

    let template = if let Some((parameter, _)) = call
        .implementation_hook()
        .and_then(super::implementation_dependency_source)
    {
        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                value_requirement(DependencySubjectRoot::Parameter(parameter)),
            ]))
            .map_err(CheckerInfrastructureError::SemanticValueStore)?
    } else if let Some(dispatch) = call.resolution().trait_dispatch() {
        let requirement = request.context().result_dispatch_requirement(dispatch)?;
        let template = super::witness::deferred_result(request, call, Some(requirement))?;

        store
            .intern_dependency_contract_template(template)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?
    } else if !opaque_result(call)
        && let BoundCallableTarget::Declaration(instance) = call.target()
    {
        declaration(instance.definition().callable_symbol())?
    } else {
        // An opaque target can retain any input value contract, including its receiver.
        let requirements = call
            .arguments()
            .iter()
            .map(|argument| match argument {
                SelectedArgument::Explicit { ordinal, .. }
                | SelectedArgument::Default { ordinal, .. } => value_requirement(
                    DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal)),
                ),
            })
            .chain(
                call.receiver()
                    .map(|_| value_requirement(DependencySubjectRoot::Receiver)),
            );

        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new(requirements))
            .map_err(CheckerInfrastructureError::SemanticValueStore)?
    };

    let template = match call.target() {
        BoundCallableTarget::Declaration(instance) => store
            .substitute_dependency_contract(template, instance.substitution())
            .map_err(CheckerInfrastructureError::SemanticValueStore)?,
        _ => template,
    };

    let template = store.dependency_contract_template_data(template);

    let template = DependencyContractTemplateData::new(super::witness::resolve(
        request,
        template.requirements(),
    )?);

    super::defaults::expand_result_defaults(request, call, &template)
}

fn value_requirement(root: DependencySubjectRoot) -> DependencyRequirement {
    DependencyRequirement::direct(
        DependencySubject::root(root),
        DependencyRequirementKind::ValueDependencies,
    )
}

/// Resolves a portable input root to the expression selected at this call site.
pub(crate) fn result_argument(
    call: &SelectedCall,
    root: DependencySubjectRoot,
) -> Option<bray_bound_tree::BoundExpressionId> {
    match root {
        DependencySubjectRoot::Parameter(parameter) => {
            call.arguments().iter().find_map(|argument| match argument {
                SelectedArgument::Explicit {
                    ordinal,
                    expression,
                    ..
                } if *ordinal == parameter.raw() => Some(*expression),
                _ => None,
            })
        }
        DependencySubjectRoot::Receiver => call.receiver().map(|receiver| receiver.expression()),
        _ => None,
    }
}

impl super::ValueInputs {
    pub(super) fn collect_returned_borrows<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        types: &bray_bound_tree::CheckedExpressionTypes,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        mut declaration: impl FnMut(
            CallableSymbolId,
        ) -> Result<
            DependencyContractTemplateId,
            CheckerQueryError<C::UpstreamError>,
        >,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        for (expression, node) in request.unit().tree().expressions() {
            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            let Some(ty) = types.expression(expression) else {
                continue;
            };

            let data = request.semantic_values().type_data(ty.ty());

            if !matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. }) {
                continue;
            }

            let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                selections.expression(expression)
            else {
                if matches!(
                    node,
                    bray_bound_tree::BoundExpression::Block(_)
                        | bray_bound_tree::BoundExpression::Match(_)
                        | bray_bound_tree::BoundExpression::Structured(_)
                ) {
                    let sources = self
                        .operands(expression)
                        .map(|source| (source, Vec::new()))
                        .collect::<Vec<_>>();

                    if !sources.is_empty() {
                        self.borrowed.insert(expression, sources);
                    }
                }

                continue;
            };

            let template = call_result_template(request, call, &mut declaration)?;

            let sources = template
                .template
                .requirements()
                .iter()
                .filter_map(|requirement| {
                    let DependencyRequirement::Direct { subject, .. } = requirement else {
                        return None;
                    };

                    let argument = result_argument(call, subject.subject_root())?;

                    Some((argument, subject.projections().to_vec()))
                })
                .collect();

            self.borrowed.insert(expression, sources);
        }

        Ok(())
    }
}
