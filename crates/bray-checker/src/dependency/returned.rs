use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{BoundCallableTarget, SelectedArgument, SelectedCall};
use bray_symbols::{
    CallableSymbolId, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal,
};

pub(crate) fn parameter_root<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    reference: bray_bound_tree::BoundReferenceTarget,
) -> Option<DependencySubjectRoot> {
    use bray_bound_tree::BoundReferenceTarget;
    use bray_symbols::{AnyLocalSymbolId, AnySymbolId};

    match reference {
        BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(parameter)) => {
            Some(DependencySubjectRoot::Parameter(SymbolOrdinal::new(
                request
                    .symbols()
                    .callable_parameter(parameter)
                    .expect("bound parameter retains its declaration")
                    .ordinal(),
            )))
        }
        BoundReferenceTarget::Local(AnyLocalSymbolId::AnonymousCallableParameter(parameter)) => {
            Some(DependencySubjectRoot::Parameter(
                request
                    .unit()
                    .local_symbols()
                    .anonymous_parameter(parameter)
                    .expect("anonymous parameter retains its local symbol")
                    .ordinal(),
            ))
        }
        BoundReferenceTarget::Local(AnyLocalSymbolId::Binding(parameter))
            if request.unit().key().callable_type_header().is_some() =>
        {
            request
                .unit()
                .local_symbols()
                .binding(parameter)
                .and_then(|binding| binding.key().ordinal())
                .map(DependencySubjectRoot::Parameter)
        }
        BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(_)) => {
            Some(DependencySubjectRoot::Receiver)
        }
        _ => None,
    }
}

pub(crate) fn opaque_result(call: &SelectedCall) -> bool {
    call.implementation_hook()
        .and_then(super::implementation_dependency_source)
        .is_none()
        && call.resolution().declaration_source().is_none()
        && !matches!(call.target(), BoundCallableTarget::Anonymous(_))
}

/// Resolves the selected callee independently of caller-owned input evaluation.
pub(crate) fn callee_result_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    declaration: impl FnOnce(
        CallableSymbolId,
    ) -> Result<
        DependencyContractTemplateId,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let store = request.semantic_values();

    let requirement = call
        .resolution()
        .trait_dispatch()
        .map(|dispatch| request.context().dispatch_requirement(dispatch))
        .transpose()?;

    let template = if let Some((source, authority)) = call
        .implementation_hook()
        .and_then(super::implementation_dependency_source)
    {
        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new(
                std::iter::once(value_requirement(source))
                    .chain(authority.map(|kind| DependencyRequirement::direct(
                        DependencySubject::root(source),
                        DependencyRequirementKind::BorrowCapabilityActive(kind),
                    )))
            )).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in call_result_template: {error:?}"))
    } else if matches!(call.target(), BoundCallableTarget::Predicate(_)) {
        // Predicate evaluation is checked in an effect-free context. It cannot retain
        // an owned cleanup incident or return a value with input dependencies.
        store
            .empty_dependency_contract_template()
            .expect("predicate outputs have no retained ownership")
    } else if requirement.is_some() {
        let template = super::witness::deferred_result(request, call, requirement)?;

        store
            .intern_dependency_contract_template(template).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in call_result_template: {error:?}"))
    } else if let BoundCallableTarget::Anonymous(_) = call.target() {
        request.anonymous_result_dependencies(
            call.resolution()
                .anonymous_body()
                .expect("selected anonymous callable retains its body key")
                .0,
        )?
    } else if let Some(instance) = call.resolution().declaration_source() {
        declaration(instance.definition().callable_symbol())?
    } else {
        // An opaque target can retain any input value contract, including its receiver.
        let requirements = input_requirements(call).collect::<Vec<_>>();

        let body = call
            .phase_behaviors()
            .deferred_execution()
            .unwrap_or(call.phase_behaviors().invocation());

        // Callable values have no hidden environment. Only explicit inputs and
        // externally reachable storage can participate in their body effects.
        let mut template = DependencyContractTemplateData::new(requirements.iter().cloned())
            .with_effects(bray_symbols::DependencyStorageEffect::capture_free_call());

        if call.invocation() == bray_symbols::CallableInvocation::Bray {
            template = template.with_abnormal_reports(requirements.iter().cloned());

            if !body
                .execution_properties()
                .contains(&bray_symbols::ExecutionProperty::Pure)
            {
                template = template.with_completion_reports(requirements);
            }
        }

        store
            .intern_dependency_contract_template(template).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in call_result_template: {error:?}"))
    };

    let instance = call.resolution().declaration_source();

    let mut template = resolved_result_template(request, instance, template, requirement)?;

    if call.invocation() == bray_symbols::CallableInvocation::Native {
        // The native entry consumes body incidents and contains abnormal completion.
        // Input providers execute separately in the caller.
        template = template.without_reports();
    }

    if let Some(relation) = call
        .implementation_hook()
        .and_then(super::implementation::implementation_borrow)
    {
        template = template.with_borrows([relation]);
    }

    if matches!(
        call.implementation_hook(),
        Some(
            bray_compiler_known::ImplementationHook::TaskJoin
                | bray_compiler_known::ImplementationHook::TaskCancel
        )
    ) {
        // An opaque Task can return dependencies from any capture. Known body
        // producers are narrowed independently by completion-source discovery.
        let requirements = template
            .requirements()
            .iter()
            .cloned()
            .chain([value_requirement(DependencySubjectRoot::Receiver)])
            .collect::<Vec<_>>();

        template = template.with_requirements(requirements);
    }

    Ok(template)
}

/// Projects normal body dependencies onto the immediate value produced by invocation.
/// An inactive future retains all inputs until execution or disposal, independently of its result.
pub(crate) fn invocation_result_template(
    call: &SelectedCall,
    template: DependencyContractTemplateData,
) -> DependencyContractTemplateData {
    if matches!(
        call.resolution().result(),
        bray_bound_tree::BoundCallResult::LazyFuture(_)
    ) {
        template.with_requirements(input_requirements(call))
    } else {
        template
    }
}

/// Applies the selected implementation, generic substitution and parameter defaults once.
pub(crate) fn call_result_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selection: &bray_bound_tree::SemanticSelection,
    declaration: impl FnOnce(
        CallableSymbolId,
    ) -> Result<
        DependencyContractTemplateId,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<super::defaults::CallResultDependencies, CheckerQueryError<C::UpstreamError>> {
    let bray_bound_tree::SemanticSelection::Call(call) = selection else {
        panic!("call static expansion retains its selected callable");
    };

    let template =
        invocation_result_template(call, callee_result_template(request, call, declaration)?);

    super::defaults::expand_result_defaults(request, selection, &template)
}

pub(crate) fn resolved_result_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    instance: Option<bray_symbols::CallableInstanceData>,
    template: DependencyContractTemplateId,
    requirement: Option<bray_symbols::ImplementationRequirementKey>,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let store = request.semantic_values();

    let data = store.dependency_contract_template_data(template);

    let cleanup_call =
        instance.filter(|_| !data.synchronous_cleanup().is_empty() || requirement.is_some());

    // Defer callee cleanup before substitution so recursive type arguments cannot grow this summary.
    let template = if cleanup_call.is_some() {
        store
            .intern_dependency_contract_template(data.as_ref().clone().with_synchronous_cleanup([]))
            .expect("deferred cleanup preserves the remaining checked contract")
    } else {
        template
    };

    let template = match instance {
        Some(instance) => store
            .substitute_dependency_contract(template, instance.substitution()).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolved_result_template: {error:?}")),
        None => template,
    };

    let template = store.dependency_contract_template_data(template);

    let mut result = super::witness::resolve(request, &template)?;

    if template
        .effects()
        .iter()
        .any(|effect| matches!(effect, bray_symbols::DependencyStorageEffect::Direct { kind: bray_symbols::DependencyMutationKind::Cleanup(_), .. } | bray_symbols::DependencyStorageEffect::Indirect { kind: bray_symbols::DependencyMutationKind::Cleanup(_), .. } | bray_symbols::DependencyStorageEffect::UnboundedMutation { kind: bray_symbols::DependencyMutationKind::Cleanup(_), .. }))
    {
        result = result.with_effects(super::cleanup::resolve_effects(request, template.effects())?);
    }

    if let Some(instance) = cleanup_call {
        let callable = store
            .intern_callable_instance(instance)
            .expect("selected cleanup call retains its checked semantic identity");

        result = result.with_synchronous_cleanup([DependencyRequirement::result_call(
            callable,
            requirement,
            [],
            bray_symbols::DependencyCallOutput::SynchronousCleanup,
        )]);
    }

    Ok(result)
}

pub(super) fn input_requirements(
    call: &SelectedCall,
) -> impl Iterator<Item = DependencyRequirement> + '_ {
    call.arguments()
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
        )
}

fn value_requirement(root: DependencySubjectRoot) -> DependencyRequirement {
    DependencyRequirement::direct(
        DependencySubject::root(root),
        DependencyRequirementKind::ValueDependencies,
    )
}

/// Resolves a portable input root to its complete checked input adaptation.
pub(crate) fn result_input(
    call: &SelectedCall,
    root: DependencySubjectRoot,
) -> Option<bray_bound_tree::SelectedInput<'_>> {
    match root {
        DependencySubjectRoot::Parameter(parameter) => call.arguments().iter().find_map(|argument| {
            let ordinal = match argument {
                SelectedArgument::Explicit { ordinal, .. } | SelectedArgument::Default { ordinal, .. } => *ordinal,
            };

            (ordinal == parameter.raw()).then_some(bray_bound_tree::SelectedInput::Argument(argument))
        }),
        DependencySubjectRoot::Receiver => call.receiver().map(bray_bound_tree::SelectedInput::Receiver),
        _ => None,
    }
}

/// Retains the inputs that bound result authority through lifetime or reference relations.
pub(crate) fn result_inputs(
    expression: bray_bound_tree::BoundExpressionId,
    call: &SelectedCall,
    template: &bray_symbols::DependencyContractTemplateData,
) -> std::collections::BTreeSet<bray_bound_tree::BoundExecutionSite> {
    // An address does not keep an ordinary borrow active. Its access guarantees
    // still expire with the physical owner named by the checked reference relation.
    let mut inputs = template
        .borrows()
        .iter()
        .filter(|relation| relation.exit().subject_root() == DependencySubjectRoot::Result)
        .filter(|relation| {
            relation.provenance() != bray_symbols::DependencyReferenceProvenance::InputRegions
        })
        .filter_map(|relation| relation.entry())
        .filter_map(|entry| call.input_site(expression, entry.subject_root()))
        .collect::<std::collections::BTreeSet<_>>();

    let mut pending = template.requirements().iter().collect::<Vec<_>>();

    while let Some(requirement) = pending.pop() {
        match requirement {
            DependencyRequirement::Direct { subject, .. } => {
                inputs.extend(call.input_site(expression, subject.subject_root()));
            }
            DependencyRequirement::CleanupDependencies { requirements, .. } => {
                pending.extend(requirements.iter())
            }
            DependencyRequirement::Guarded(guarded) => {
                let subject = match guarded.guard() {
                    bray_symbols::DependencyGuard::NullablePresent(subject)
                    | bray_symbols::DependencyGuard::ActiveUnionVariant { subject, .. } => subject,
                };

                inputs.extend(call.input_site(expression, subject.subject_root()));

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
            DependencyRequirement::Variable { .. }
            | DependencyRequirement::SynchronousCleanup(_)
            | DependencyRequirement::FinalizerBodyCleanup(_)
            | DependencyRequirement::DefaultCleanup { .. } => {}
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

            let borrowed_result = matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. });

            let representation = request
                .available_compiler_known_symbols()
                .type_representation(request.semantic_values(), ty.ty());

            if representation.is_some_and(|role| {
                role.numeric_kind().is_some()
                    || matches!(
                        role,
                        bray_compiler_known::RepresentationRole::ScalarBool
                            | bray_compiler_known::RepresentationRole::Unit
                            | bray_compiler_known::RepresentationRole::Never
                    )
            }) {
                continue;
            }

            let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                selections.expression(expression)
            else {
                if borrowed_result
                    && matches!(
                        node,
                        bray_bound_tree::BoundExpression::Block(_)
                            | bray_bound_tree::BoundExpression::Match(_)
                            | bray_bound_tree::BoundExpression::Structured(_)
                    )
                {
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

            if matches!(call.resolution().result(), bray_bound_tree::BoundCallResult::LazyFuture(_)) {
                continue;
            }

            let template = call_result_template(
                request,
                selections
                    .expression(expression)
                    .expect("selected call has its owning selection"),
                &mut declaration,
            )?;

            let mut sources = std::collections::BTreeSet::new();

            for relation in template.template.borrows() {
                let Some(entry) = relation.entry() else {
                    continue;
                };

                if !relation.is_exact()
                    || relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                    || relation.dereferences() != 0
                    || relation.exit().subject_root() != DependencySubjectRoot::Result
                {
                    continue;
                }

                let Some(input) = result_input(call, entry.subject_root()) else { continue; };

                let Some(argument) = input.expression() else { continue; };

                let Some(projections) = crate::storage::unadapted_projections(
                    request.semantic_values(), input.conversion(), entry.projections(),
                ) else { continue; };

                let mut entry_ty = types
                    .expression(argument)
                    .expect("returned borrow origin retains its checked argument")
                    .ty();

                for projection in projections {
                    entry_ty =
                        crate::storage::projected_value_type(request, entry_ty, *projection)?
                            .value()
                            .expect("returned borrow origin retains its checked projection");
                }

                // A lifetime anchor need not contain the referent. Only exact
                // language-borrow origins identify aggregate storage here.
                if crate::representation::reference_kind(request, entry_ty)
                    == Some(bray_symbols::DependencyReferenceKind::Address)
                {
                    continue;
                }

                if borrowed_result && relation.exit().projections().is_empty() {
                    sources.insert((
                        argument,
                        projections
                            .iter()
                            .chain(relation.target_projections())
                            .copied()
                            .collect::<Vec<_>>(),
                    ));
                }

                let [projection] = relation.exit().projections() else {
                    continue;
                };

                if !relation.target_projections().is_empty() || !entry.projections().is_empty() {
                    continue;
                }

                // Resolve a copied borrow field only when its complete value
                // contract and physical origin both name this input.
                let expected = DependencyRequirement::direct(
                    DependencySubject::root(entry.subject_root()),
                    DependencyRequirementKind::ValueDependencies,
                );

                if template.template.requirements() != [expected]
                    || !template.template.carriers().is_empty()
                {
                    continue;
                }

                let projected =
                    crate::storage::projected_value_type(request, ty.ty(), *projection)?;

                let field_ty = projected
                    .value()
                    .expect("returned borrow field retains its checked projection");

                if request.semantic_values().unborrowed_type(field_ty)
                    != request.semantic_values().unborrowed_type(entry_ty)
                {
                    continue;
                }

                self.projections
                    .entry(expression)
                    .or_default()
                    .insert(*projection, (argument, None));
            }

            if !sources.is_empty() {
                self.borrowed
                    .insert(expression, sources.into_iter().collect());
            }
        }

        Ok(())
    }
}
