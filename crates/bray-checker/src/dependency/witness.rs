use bray_bound_tree::{BoundCallableTarget, SelectedArgument, SelectedCall};
use bray_symbols::{
    DependencyCallInput, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

pub(super) fn deferred_result<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    requirement: Option<bray_symbols::ImplementationRequirementKey>,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let BoundCallableTarget::Declaration(instance) = call.target() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
    };

    let callable = request
        .semantic_values()
        .intern_callable_instance(instance)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(DependencyContractTemplateData::new([
        DependencyRequirement::result_call(callable, requirement, call_inputs(call)),
    ]))
}

fn call_inputs(call: &SelectedCall) -> Vec<DependencyCallInput> {
    let roots = call
        .arguments()
        .iter()
        .map(|argument| match argument {
            SelectedArgument::Explicit { ordinal, .. }
            | SelectedArgument::Default { ordinal, .. } => {
                DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal))
            }
        })
        .chain(call.receiver().map(|_| DependencySubjectRoot::Receiver));

    roots
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
        .collect()
}

pub(super) fn instantiate_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    template: DependencyContractTemplateId,
    key: super::parameters::ResultKey,
) -> Result<std::sync::Arc<DependencyContractTemplateData>, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();

    let template = values
        .substitute_dependency_contract(template, key.0.substitution())
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let template = match key.1 {
        Some((context, subject)) => values
            .substitute_contextual_self_in_dependency_contract(template, context, subject)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?,
        None => template,
    };

    values
        .dependency_contract_template_data(template)
        .map_err(|error| CheckerInfrastructureError::SemanticValueStore(error).into())
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
    active: std::collections::BTreeSet<super::parameters::ResultKey>,
    results: std::collections::BTreeMap<super::parameters::ResultKey, Vec<DependencyRequirement>>,
    changed: bool,
    equations: Vec<EquationFrame>,
    scope: Option<usize>,
    parameters: super::parameters::ResultParameters,
}

pub(super) fn resolve<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    requirements: &[DependencyRequirement],
) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
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
        let result = resolver.requirements(requirements)?;

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
        let (selected, contextual_self) = key;

        let key = self.parameters.key(self.request, key)?;

        if !self.active.insert(key) {
            // Recursive calls read the preceding approximation while this result is recomputed.
            return Ok(self.results.get(&key).cloned().unwrap_or_default());
        }

        let request = self.request;

        let template = request
            .context()
            .callable_result_dependencies(selected.definition().callable_symbol())?;

        let template = instantiate_template(request, template, (selected, contextual_self))?;

        let result =
            DependencyContractTemplateData::new(self.requirements(template.requirements())?);

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
                break super::equations::shift_variables(&evaluated, -1)
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;
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

        let invalid = || {
            CheckerInfrastructureError::SemanticValueStore(
                bray_symbols::SemanticValueStoreError::InvalidDependencyVariable {
                    depth,
                    ordinal: ordinal.raw(),
                },
            )
        };

        let scope = selected.ok_or_else(invalid)?;

        // Keep the definition alive while recursive evaluation mutates the frame stack.
        let definition = self.equations[scope]
            .definitions
            .get(usize::try_from(ordinal.raw()).map_err(|_| invalid())?)
            .cloned()
            .ok_or_else(invalid)?;

        if !self.equations[scope].active.insert(ordinal.raw()) {
            return super::equations::shift_variables(
                self.equations[scope]
                    .values
                    .get(&ordinal.raw())
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                i64::from(depth),
            )
            .map_err(|error| CheckerInfrastructureError::SemanticValueStore(error).into());
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

        super::equations::shift_variables(&value, i64::from(depth))
            .map_err(|error| CheckerInfrastructureError::SemanticValueStore(error).into())
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
                DependencyRequirement::FixedPoint {
                    definitions,
                    result: roots,
                } => result.extend(self.fixed_point(definitions, roots)?),
                DependencyRequirement::Variable { depth, ordinal } => {
                    result.extend(self.variable(*depth, *ordinal)?)
                }
                DependencyRequirement::ResultCall {
                    callable,
                    requirement,
                    inputs,
                } => {
                    let key = match requirement {
                        Some(requirement) => {
                            let Some((selected, context)) = request
                                .context()
                                .result_witness_callable(*callable, *requirement)?
                            else {
                                // The enclosing contract retains the unresolved selection.
                                result.push(item.clone());
                                continue;
                            };

                            (selected, Some((context, requirement.subject())))
                        }
                        None => {
                            let selected = request
                                .semantic_values()
                                .callable_instance_data(*callable)
                                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                            match request
                                .semantic_values()
                                .require_concrete_substitution(selected.substitution())
                            {
                                Ok(_) => {}
                                Err(bray_symbols::SemanticValueStoreError::OpenSubstitution) => {
                                    result.push(item.clone());

                                    continue;
                                }
                                Err(error) => {
                                    return Err(CheckerInfrastructureError::SemanticValueStore(
                                        error,
                                    )
                                    .into());
                                }
                            }

                            (*selected, None)
                        }
                    };

                    let resolved = self.selected_result(key)?;

                    let mapped = map_call_inputs(&resolved, inputs)
                        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                    result.extend(self.requirements(&mapped)?);
                }
                DependencyRequirement::Guarded(guarded) => {
                    result.push(DependencyRequirement::guarded(
                        guarded.guard().clone(),
                        self.requirements(guarded.requirements())?,
                    ))
                }
                DependencyRequirement::Direct { .. } => result.push(item.clone()),
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
    map_requirements(requirements, &mut |subject, kind| {
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

pub(super) fn map_requirements(
    requirements: &[DependencyRequirement],
    map: &mut impl FnMut(
        &DependencySubject,
        DependencyRequirementKind,
    )
        -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError>,
) -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError> {
    super::equations::map_requirements(requirements, 0, &mut |requirement, depth| {
        let DependencyRequirement::Direct { subject, kind } = requirement else {
            return None;
        };

        Some(
            map(subject, *kind)
                .and_then(|mapped| super::equations::shift_variables(&mapped, i64::from(depth))),
        )
    })
}

fn project_requirements(
    requirements: &[DependencyRequirement],
    projections: &[bray_symbols::DependencyProjection],
) -> Result<Vec<DependencyRequirement>, bray_symbols::SemanticValueStoreError> {
    map_requirements(requirements, &mut |subject, kind| {
        Ok(vec![DependencyRequirement::direct(
            super::result::normalized_subject(
                subject.subject_root(),
                subject.projections().iter().chain(projections).copied(),
            ),
            kind,
        )])
    })
}
