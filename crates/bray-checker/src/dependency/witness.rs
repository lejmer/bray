use bray_bound_tree::{SelectedArgument, SelectedCall};
use bray_symbols::{
    DependencyCallInput, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal,
};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(super) fn deferred_result<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    requirement: Option<bray_symbols::ImplementationRequirementKey>,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let Some(instance) = call.resolution().declaration_source() else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in deferred_result"
        );
    };

    let roots = input_roots(call);

    let body = call
        .phase_behaviors()
        .deferred_execution()
        .unwrap_or(call.phase_behaviors().invocation());

    deferred_outputs(
        request,
        instance,
        requirement,
        roots,
        call.invocation() != bray_symbols::CallableInvocation::Native,
        !body
            .execution_properties()
            .contains(&bray_symbols::ExecutionProperty::Pure),
    )
}

pub(in crate::dependency) fn deferred_outputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    instance: bray_symbols::CallableInstanceData,
    requirement: Option<bray_symbols::ImplementationRequirementKey>,
    roots: impl IntoIterator<Item = DependencySubjectRoot>,
    publishes_reports: bool,
    normal_reports: bool,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let callable = request
        .semantic_values()
        .intern_callable_instance(instance)
        .expect("selected callable belongs to the current semantic store");

    let inputs = roots
        .into_iter()
        .map(|root| {
            DependencyCallInput::new(
                root,
                [DependencyRequirement::direct(
                    DependencySubject::root(root),
                    DependencyRequirementKind::ValueDependencies,
                )],
                [DependencyRequirement::direct(
                    DependencySubject::root(root),
                    DependencyRequirementKind::StorageAlive,
                )],
            )
        })
        .collect::<Vec<_>>();

    let mut template = DependencyContractTemplateData::new([DependencyRequirement::result_call(
        callable,
        requirement,
        inputs.iter().cloned(),
        bray_symbols::DependencyCallOutput::Value,
    )]);

    if requirement.is_some()
        || !request
            .semantic_values()
            .dependency_contract_template_data(
                request.callable_result_dependencies(instance.definition().callable_symbol())?,
            )
            .synchronous_cleanup()
            .is_empty()
    {
        template = template.with_synchronous_cleanup([DependencyRequirement::result_call(
            callable,
            requirement,
            [],
            bray_symbols::DependencyCallOutput::SynchronousCleanup,
        )]);
    }

    if publishes_reports {
        template = template.with_abnormal_reports([DependencyRequirement::result_call(
            callable,
            requirement,
            inputs.iter().cloned(),
            bray_symbols::DependencyCallOutput::AbnormalReport,
        )]);

        if normal_reports {
            template = template.with_completion_reports([DependencyRequirement::result_call(
                callable,
                requirement,
                inputs,
                bray_symbols::DependencyCallOutput::CompletionReport,
            )]);
        }
    }

    Ok(template)
}

fn input_roots(call: &SelectedCall) -> impl Iterator<Item = DependencySubjectRoot> + '_ {
    call.arguments()
        .iter()
        .map(|argument| match argument {
            SelectedArgument::Explicit { ordinal, .. }
            | SelectedArgument::Default { ordinal, .. } => {
                DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal))
            }
        })
        .chain(call.receiver().map(|_| DependencySubjectRoot::Receiver))
}

pub(super) fn instantiate_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    template: DependencyContractTemplateId,
    key: super::parameters::ResultKey,
) -> Result<std::sync::Arc<DependencyContractTemplateData>, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();

    let template = values
        .substitute_dependency_contract(template, key.callable.substitution()).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in instantiate_template: {error:?}"));

    let template = match key.contextual_self {
        Some((context, subject)) => values
            .substitute_contextual_self_in_dependency_contract(template, context, subject).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in instantiate_template: {error:?}")),
        None => template,
    };

    Ok(values.dependency_contract_template_data(template))
}

struct EquationFrame {
    definitions: std::sync::Arc<[std::sync::Arc<[DependencyRequirement]>]>,
    parent: Option<usize>,
    active: std::collections::BTreeSet<u32>,
    values: std::collections::BTreeMap<u32, Vec<DependencyRequirement>>,
    changed: bool,
}

struct ResultResolver<'a, C: CheckerRequestContext + ?Sized> {
    request: CheckerUnitView<'a, C>,
    active: std::collections::BTreeSet<super::parameters::ResultIdentity>,
    results:
        std::collections::BTreeMap<super::parameters::ResultIdentity, Vec<DependencyRequirement>>,
    changed: bool,
    equations: Vec<EquationFrame>,
    scope: Option<usize>,
    parameters: super::parameters::ResultParameters,
}

pub(super) fn resolve<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    template: &DependencyContractTemplateData,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let mut resolver = ResultResolver {
        request,
        active: Default::default(),
        results: Default::default(),
        changed: false,
        equations: Vec::new(),
        scope: None,
        parameters: Default::default(),
    };

    loop {
        resolver.changed = false;

        let result = template.try_map_requirements(|output, requirements| {
            if output == bray_symbols::DependencyCallOutput::SynchronousCleanup {
                Ok(requirements.to_vec())
            } else {
                resolver.requirements(requirements)
            }
        })?;

        if !resolver.changed {
            return Ok(result);
        }
    }
}

impl<C: CheckerRequestContext + ?Sized> ResultResolver<'_, C> {
    fn selected_result(
        &mut self,
        key: super::parameters::ResultKey,
    ) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let selected = key;

        let key = self.parameters.key(self.request, key)?;

        // Active membership and the stored approximation share immutable condition truths.
        if !self
            .active
            .insert((key.0, key.1.as_ref().map(std::sync::Arc::clone)))
        {
            // Recursive calls read the preceding approximation while this result is recomputed.
            return Ok(self.results.get(&key).cloned().unwrap_or_default());
        }

        let request = self.request;

        let template = request
            .callable_result_dependencies(selected.callable.definition().callable_symbol())?;

        let template = instantiate_template(request, template, selected)?;

        let requirements = template.output_requirements(selected.output);
        let result = DependencyContractTemplateData::new(self.requirements(requirements)?);

        self.active.remove(&key);

        let result = result.requirements().to_vec();

        self.changed |= self.results.get(&key) != Some(&result);

        // The memoized fixed-point value and this caller need independent owned snapshots.
        self.results.insert(key, result.clone());

        Ok(result)
    }

    fn fixed_point(
        &mut self,
        definitions: &std::sync::Arc<[std::sync::Arc<[DependencyRequirement]>]>,
        result: &[DependencyRequirement],
    ) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let parent = self.scope;
        let scope = self.equations.len();

        self.equations.push(EquationFrame {
            definitions: std::sync::Arc::clone(definitions),
            parent,
            active: Default::default(),
            values: Default::default(),
            changed: false,
        });

        self.scope = Some(scope);

        let evaluated = loop {
            self.equations[scope].changed = false;

            let evaluated = self.requirements(result)?;

            if super::equations::has_variables(&evaluated, true) {
                break vec![DependencyRequirement::fixed_point(
                    definitions
                        .iter()
                        .map(|definition| definition.iter().cloned()),
                    result.iter().cloned(),
                )];
            }

            if !self.equations[scope].changed {
                break super::equations::shift_variables(&evaluated, -1).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in fixed_point: {error:?}"));
            }
        };

        self.equations.pop();
        self.scope = parent;

        Ok(evaluated)
    }

    fn variable(
        &mut self,
        depth: u32,
        ordinal: SymbolOrdinal,
    ) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let mut selected = self.scope;

        for _ in 0..depth {
            selected = selected.and_then(|scope| self.equations[scope].parent);
        }

        let scope = selected.unwrap_or_else(|| {
            panic!(
                "dependency variable {depth}:{} has no enclosing equation",
                ordinal.raw()
            )
        });

        // Keep the definition alive while recursive evaluation mutates the frame stack.
        let definition = self.equations[scope]
            .definitions
            .get(usize::try_from(ordinal.raw()).expect("dependency ordinal must fit host"))
            .cloned()
            .unwrap_or_else(|| {
                panic!(
                    "dependency variable {depth}:{} has no definition",
                    ordinal.raw()
                )
            });

        if !self.equations[scope].active.insert(ordinal.raw()) {
            return Ok(super::equations::shift_variables(
                self.equations[scope]
                    .values
                    .get(&ordinal.raw())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                i64::from(depth),
            )
            .unwrap_or_else(|error| {
                panic!("variable must satisfy its checked construction contract: {error:?}")
            }));
        }

        let previous = self.scope;

        self.scope = Some(scope);

        let value = self.requirements(&definition)?;

        self.scope = previous;
        self.equations[scope].active.remove(&ordinal.raw());

        let value = DependencyContractTemplateData::new(value)
            .requirements()
            .to_vec();

        self.equations[scope].changed |=
            self.equations[scope].values.get(&ordinal.raw()) != Some(&value);

        // The equation's previous approximation and its caller retain independent snapshots.
        self.equations[scope]
            .values
            .insert(ordinal.raw(), value.clone());

        Ok(
            super::equations::shift_variables(&value, i64::from(depth)).unwrap_or_else(|error| {
                panic!("variable must satisfy its checked construction contract: {error:?}")
            }),
        )
    }

    fn requirements(
        &mut self,
        requirements: &[DependencyRequirement],
    ) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let request = self.request;
        let mut result = Vec::new();

        // Resolved contracts share unchanged guards and open expressions with their portable input.
        for item in requirements {
            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            match item {
                DependencyRequirement::CleanupDependencies { ty, requirements } => {
                    if !request.semantic_values().type_is_concrete(*ty) {
                        result.push(DependencyRequirement::cleanup_dependencies(
                            *ty,
                            self.requirements(requirements)?,
                        ));
                    } else {
                        let mut cleanup = crate::asynchronous::CleanupShapeResolver::new(request);

                        if cleanup.requirement(*ty)?
                            != bray_bound_tree::AsyncStorageCleanupRequirement::None
                        {
                            result.extend(self.requirements(requirements)?);
                        }
                    }
                }
                DependencyRequirement::FixedPoint {
                    definitions,
                    result: roots,
                } => result.extend(self.fixed_point(definitions, roots)?),
                DependencyRequirement::Variable { depth, ordinal } => {
                    result.extend(self.variable(*depth, *ordinal)?)
                }
                DependencyRequirement::ResultCall {
                    output,
                    callable,
                    requirement,
                    inputs,
                } => {
                    let key = match requirement {
                        Some(requirement) => {
                            let Some((selected, context, _)) =
                                request
                                    .context()
                                    .witness_callable(*callable, *requirement, &[])?
                            else {
                                // The enclosing contract retains the unresolved selection.
                                result.push(item.clone());
                                continue;
                            };

                            super::parameters::ResultKey {
                                callable: selected,
                                contextual_self: Some((context, requirement.subject())),
                                output: *output,
                            }
                        }
                        None => {
                            let selected =
                                request.semantic_values().callable_instance_data(*callable);

                            if !request
                                .semantic_values()
                                .substitution_is_concrete(selected.substitution())
                            {
                                result.push(item.clone());
                                continue;
                            }

                            super::parameters::ResultKey {
                                callable: *selected,
                                contextual_self: None,
                                output: *output,
                            }
                        }
                    };

                    let resolved = self.selected_result(key)?;

                    let mapped = map_call_inputs(&resolved, inputs).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in requirements: {error:?}"));

                    result.extend(self.requirements(&mapped)?);
                }
                DependencyRequirement::Guarded(guarded) => {
                    result.push(DependencyRequirement::guarded(
                        guarded.guard().clone(),
                        self.requirements(guarded.requirements())?,
                    ))
                }
                DependencyRequirement::Direct { .. }
                | DependencyRequirement::SynchronousCleanup(_)
                | DependencyRequirement::FinalizerBodyCleanup(_)
                | DependencyRequirement::DefaultCleanup { .. } => result.push(item.clone()),
            }
        }

        Ok(result)
    }
}

fn map_call_inputs(
    requirements: &[DependencyRequirement],
    inputs: &[DependencyCallInput],
) -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError> {
    // Unmapped subjects retain the input contract's immutable projection snapshot.
    map_requirements(requirements, &mut |subject, kind, _| {
        let Some(input) = inputs
            .iter()
            .find(|input| input.root() == subject.subject_root())
        else {
            return Ok(vec![DependencyRequirement::direct(subject.clone(), kind)]);
        };

        let source = if kind == DependencyRequirementKind::ValueDependencies {
            input.values()
        } else {
            input.storage()
        };

        project_requirements(source, subject.projections())
    })
}

/// The meaning of a subject while substituting a symbolic result call.
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum RequirementMappingRole {
    /// An ordinary requirement, including storage keeping a supplied value alive.
    Requirement,
    /// Data carried by a supplied value, independently of its implicit receiver loan.
    InputValue,
}

pub(crate) fn map_requirements<E>(
    requirements: &[DependencyRequirement],
    map: &mut impl FnMut(
        &DependencySubject,
        DependencyRequirementKind,
        RequirementMappingRole,
    )
        -> Result<Vec<DependencyRequirement>, E>,
) -> Result<Vec<DependencyRequirement>, E> {
    map_requirements_at_depth(requirements, 0, RequirementMappingRole::Requirement, map)
}

fn map_requirements_at_depth<E>(
    requirements: &[DependencyRequirement],
    depth: u32,
    role: RequirementMappingRole,
    map: &mut impl FnMut(&DependencySubject, DependencyRequirementKind, RequirementMappingRole)
        -> Result<Vec<DependencyRequirement>, E>,
) -> Result<Vec<DependencyRequirement>, E> {
    super::equations::map_requirements(requirements, depth, &mut |requirement, depth| {
        if let DependencyRequirement::Guarded(guarded) = requirement {
            // Value dependencies do not prove that the caller's current storage
            // has the callee's entry discriminant. Retain the possible dependency
            // without exporting that unproved condition into the caller's roots.
            return Some(map_requirements_at_depth(guarded.requirements(), depth, role, map));
        }

        if let DependencyRequirement::ResultCall { output, callable, requirement, inputs } = requirement {
            let mapped = inputs.iter().map(|input| {
                Ok(DependencyCallInput::new(
                    input.root(),
                    map_requirements_at_depth(input.values(), depth, RequirementMappingRole::InputValue, map)?,
                    map_requirements_at_depth(input.storage(), depth, RequirementMappingRole::Requirement, map)?,
                ))
            }).collect::<Result<Vec<_>, E>>();

            return Some(mapped.map(|inputs| vec![DependencyRequirement::result_call(
                *callable, *requirement, inputs, *output,
            )]));
        }

        let DependencyRequirement::Direct { subject, kind } = requirement else {
            return None;
        };

        Some(
            map(subject, *kind, role)
                .map(|mapped| super::equations::shift_variables(&mapped, i64::from(depth))
                    .expect("mapped dependency variables retain their checked enclosing equation depth")),
        )
    })
}

fn project_requirements(
    requirements: &[DependencyRequirement],
    projections: &[bray_symbols::DependencyProjection],
) -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError> {
    map_requirements(requirements, &mut |subject, kind, _| {
        Ok(vec![DependencyRequirement::direct(
            super::result::normalized_subject(
                subject.subject_root(),
                subject.projections().iter().chain(projections).copied(),
            ),
            kind,
        )])
    })
}

#[cfg(test)]
mod tests {
    use super::map_requirements;
    use bray_symbols::{
        DependencyGuard, DependencyRequirement, DependencyRequirementKind, DependencySubject,
        DependencySubjectRoot, SymbolOrdinal,
    };

    #[test]
    fn mapped_guards_keep_the_enclosing_equation_depth() {
        let receiver = DependencySubject::root(DependencySubjectRoot::Receiver);
        let input = DependencyRequirement::direct(receiver.clone(), DependencyRequirementKind::ValueDependencies);
        let equation = DependencyRequirement::fixed_point(
            [vec![input.clone()]],
            [DependencyRequirement::guarded(DependencyGuard::NullablePresent(receiver), [input])],
        );
        let mapped = map_requirements(&[equation], &mut |_, _, _| {
            Ok::<_, std::convert::Infallible>(vec![DependencyRequirement::variable(0, SymbolOrdinal::new(0))])
        }).expect("mapping preserves the caller's outer variable binding");
        let outer = DependencyRequirement::variable(1, SymbolOrdinal::new(0));

        assert_eq!(mapped, vec![DependencyRequirement::fixed_point([vec![outer.clone()]], [outer])]);
    }
}
