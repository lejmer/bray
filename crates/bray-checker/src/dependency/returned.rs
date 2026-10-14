use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
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

    let template = if let Some((parameter, authority)) = call
        .implementation_hook()
        .and_then(super::implementation_dependency_source)
    {
        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new(
                std::iter::once(value_requirement(DependencySubjectRoot::Parameter(parameter)))
                    .chain(authority.map(|kind| DependencyRequirement::direct(
                        DependencySubject::root(DependencySubjectRoot::Parameter(parameter)),
                        DependencyRequirementKind::BorrowCapabilityActive(kind),
                    )))
            )).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in call_result_template: {error:?}"))
    } else if let Some(dispatch) = call.resolution().trait_dispatch() {
        let requirement = request.context().result_dispatch_requirement(dispatch)?;
        let template = super::witness::deferred_result(request, call, Some(requirement))?;

        store
            .intern_dependency_contract_template(template).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in call_result_template: {error:?}"))
    } else if !opaque_result(call)
        && let BoundCallableTarget::Declaration(instance) = call.target()
    {
        declaration(instance.definition().callable_symbol())?
    } else {
        // An opaque target can retain any input value contract, including its receiver.
        let requirements = call.arguments().iter().map(|argument| match argument {
        SelectedArgument::Explicit { ordinal, .. } | SelectedArgument::Default { ordinal, .. } =>
            value_requirement(DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal))),
    }).chain(call.receiver().map(|_| value_requirement(DependencySubjectRoot::Receiver)));

        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new(requirements)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in call_result_template: {error:?}"))
    };

    let instance = match call.target() {
        BoundCallableTarget::Declaration(instance) => Some(instance),
        _ => None,
    };

    let mut template = resolved_result_template(request, instance, template)?;

    if let Some(relation) = call
        .implementation_hook()
        .and_then(super::implementation::implementation_borrow)
    {
        template = template.with_borrows([relation]);
    }

    super::defaults::expand_result_defaults(request, call, &template)
}

pub(crate) fn resolved_result_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    instance: Option<bray_symbols::CallableInstanceData>,
    template: DependencyContractTemplateId,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let store = request.semantic_values();

    let template = match instance {
        Some(instance) => store
            .substitute_dependency_contract(template, instance.substitution()).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolved_result_template: {error:?}")),
        None => template,
    };

    let template = store.dependency_contract_template_data(template);

    let mut result = template.with_requirements(super::witness::resolve(request, template.requirements())?);

    if template.effects().iter().any(|effect| matches!(effect, bray_symbols::DependencyStorageEffect::Cleanup(_))) {
        result = result.with_effects(resolve_cleanup_effects(request, template.effects())?);
    }

    Ok(result)
}

fn resolve_cleanup_effects<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: &[bray_symbols::DependencyStorageEffect],
) -> Result<Vec<bray_symbols::DependencyStorageEffect>, CheckerQueryError<C::UpstreamError>> {
    use bray_symbols::{DependencyStorageEffect, TypeData};

    let mut cleanup = crate::asynchronous::CleanupShapeResolver::new(request);
    let mut resolved = Vec::with_capacity(effects.len());

    for effect in effects {
        let DependencyStorageEffect::Cleanup(ty) = effect else {
            // Instantiation owns its effect list and shares the immutable subject projections.
            resolved.push(effect.clone());
            continue;
        };

        if matches!(cleanup.requirement(*ty)?, bray_bound_tree::AsyncStorageCleanupRequirement::None) {
            continue;
        }

        let parts = crate::asynchronous::full_cleanup_parts(
            &mut cleanup, *ty, request.unit().key().source(), false,
        )?;

        if parts.value().is_empty() {
            return Ok(vec![DependencyStorageEffect::Unknown]);
        }

        for (ty, part) in parts.value() {
            // Retain formal type parameters, not recursively growing instantiated wrappers.
            // A concrete lifecycle or storage protocol has opaque effects at this boundary.
            if matches!(request.semantic_values().type_data(*ty).as_ref(), TypeData::TypeParameter(_))
                && part.as_ref().is_some_and(|part| part.release().is_none()) {
                resolved.push(DependencyStorageEffect::Cleanup(*ty));
            } else {
                return Ok(vec![DependencyStorageEffect::Unknown]);
            }
        }
    }

    Ok(resolved)
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

/// Retains the inputs that bound result authority through lifetime or reference relations.
pub(crate) fn result_inputs(
    call: &SelectedCall,
    template: &bray_symbols::DependencyContractTemplateData,
) -> std::collections::BTreeSet<bray_bound_tree::BoundExpressionId> {
    // An address does not keep an ordinary borrow active. Its access guarantees
    // still expire with the physical owner named by the checked reference relation.
    let mut inputs = template
        .borrows()
        .iter()
        .filter(|relation| relation.exit().subject_root() == DependencySubjectRoot::Result)
        .filter_map(|relation| result_argument(call, relation.entry().subject_root()))
        .collect::<std::collections::BTreeSet<_>>();

    let mut pending = template.requirements().iter().collect::<Vec<_>>();

    while let Some(requirement) = pending.pop() {
        match requirement {
            DependencyRequirement::Direct { subject, .. } => {
                inputs.extend(result_argument(call, subject.subject_root()));
            }
            DependencyRequirement::Guarded(guarded) => {
                let subject = match guarded.guard() {
                    bray_symbols::DependencyGuard::NullablePresent(subject)
                    | bray_symbols::DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
                };

                inputs.extend(result_argument(call, subject.subject_root()));
                pending.extend(guarded.requirements());
            }
            DependencyRequirement::ResultCall { inputs, .. } => {
                for input in inputs.iter() {
                    pending.extend(input.values());
                    pending.extend(input.storage());
                }
            }
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => {
                pending.extend(result.iter());
                pending.extend(definitions.iter().flat_map(|definition| definition.iter()));
            }
            DependencyRequirement::Variable { .. } => {}
        }
    }

    inputs
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
