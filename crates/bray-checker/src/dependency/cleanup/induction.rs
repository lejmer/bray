use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bray_symbols::{
    CallableContractsQuery, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, GenericArgument, GenericParameterSymbolId, GenericSubstitutionData,
    TypeData,
};

use crate::asynchronous::CleanupShapeResolver;
use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView,
};

use super::super::parameters::ResultKey;
use super::key::CleanupKey;

/// A conditional proof for every substitution of the basis's remaining type parameters.
/// Each concrete use must discharge its assumptions before sharing the basis identity.
#[derive(Clone)]
pub(super) struct CleanupScheme {
    pub(super) basis: CleanupKey,
    pub(super) assumptions: DependencyContractTemplateId,
}

pub(super) fn matches_basis<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    basis: CleanupKey,
    selected: CleanupKey,
) -> bool {
    if basis.owner() != selected.owner()
        || !match (basis.contextual_self(), selected.contextual_self()) {
            (None, None) => true,
            (Some((context, pattern)), Some((actual_context, actual))) => {
                context == actual_context
                    && (pattern == actual
                        || *request.semantic_values().type_data(pattern)
                            == TypeData::ContextualSelf(context))
            }
            _ => false,
        }
    {
        return false;
    }

    let values = request.semantic_values();

    let pattern = values.generic_substitution_data(basis.substitution());
    let actual = values.generic_substitution_data(selected.substitution());

    if pattern.owner() != actual.owner() || pattern.bindings().len() != actual.bindings().len() {
        return false;
    }

    pattern
        .bindings()
        .iter()
        .zip(actual.bindings())
        .all(|(binding, actual)| {
            let wildcard = match (binding.parameter(), binding.argument()) {
                (GenericParameterSymbolId::Type(parameter), GenericArgument::Type(ty)) => {
                    *values.type_data(ty) == TypeData::TypeParameter(parameter)
                }
                _ => false,
            };

            binding.parameter() == actual.parameter()
                && (wildcard || actual.argument() == binding.argument())
        })
}

pub(super) fn generalize<C, F>(
    request: CheckerUnitView<'_, C>,
    selected: CleanupKey,
    maximum_depth: usize,
    proofs: &mut BTreeMap<CleanupKey, Option<CleanupScheme>>,
    completed: &mut F,
) -> Result<Option<CleanupScheme>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableContractsQuery> + ?Sized,
    F: FnMut(
        bray_bound_tree::LifecycleCallable,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
{
    let values = request.semantic_values();

    let data = values.generic_substitution_data(selected.substitution());

    let mut arguments = data
        .bindings()
        .iter()
        .map(|binding| binding.argument())
        .collect::<Vec<_>>();

    let mut scheme = None;

    for (index, binding) in data.bindings().iter().enumerate() {
        let GenericParameterSymbolId::Type(parameter) = binding.parameter() else {
            continue;
        };

        let original = arguments[index];

        let formal = values
            .intern_type(TypeData::TypeParameter(parameter))
            .expect("selected generic type parameter belongs to the semantic store");

        arguments[index] = GenericArgument::Type(formal);

        let substitution = GenericSubstitutionData::try_new(
            data.owner(),
            data.bindings().iter().map(|binding| binding.parameter()),
            arguments.iter().copied(),
        )
        .expect("cleanup abstraction preserves generic argument kinds");

        let substitution = values
            .intern_generic_substitution(substitution)
            .expect("cleanup abstraction retains established semantic identities");

        let basis = selected.with_substitution(substitution);

        if !proofs.contains_key(&basis) {
            let result = prove(request, basis, maximum_depth, completed)?;

            proofs.insert(basis, result);
        }

        if let Some(proven) = proofs
            .get(&basis)
            .expect("candidate proof was evaluated")
            .clone()
        {
            scheme = Some(proven);
        } else {
            arguments[index] = original;
        }
    }

    if let Some((context, _)) = selected.contextual_self() {
        let subject = values
            .intern_type(TypeData::ContextualSelf(context))
            .expect("cleanup abstraction retains its selected Self context");

        let basis = scheme
            .as_ref()
            .map_or(selected, |scheme| scheme.basis)
            .with_contextual_self(Some((context, subject)));

        if !proofs.contains_key(&basis) {
            let result = prove(request, basis, maximum_depth, completed)?;

            proofs.insert(basis, result);
        }

        if let Some(proven) = proofs
            .get(&basis)
            .expect("Self candidate proof was evaluated")
            .clone()
        {
            scheme = Some(proven);
        }
    }

    Ok(scheme)
}

fn prove<C, F>(
    request: CheckerUnitView<'_, C>,
    basis: CleanupKey,
    maximum_depth: usize,
    completed: &mut F,
) -> Result<Option<CleanupScheme>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableContractsQuery> + ?Sized,
    F: FnMut(
        bray_bound_tree::LifecycleCallable,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
{
    let mut cleanup = CleanupShapeResolver::new(request);
    let mut assumptions = BTreeSet::new();
    let mut cycles = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut pending = VecDeque::from([(basis, 0_usize)]);

    while let Some((key, depth)) = pending.pop_front() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        if depth != 0 && basis.owner() == key.owner() {
            if !matches_basis(request, basis, key) {
                return Ok(None);
            }

            cycles.insert(key);
            continue;
        }

        if !seen.insert(key) {
            continue;
        }

        if depth >= maximum_depth {
            return Ok(None);
        }

        let template = key.template(request)?;

        let Some(calls) = expand(
            request,
            &mut cleanup,
            template.synchronous_cleanup(),
            &mut assumptions,
            true,
            completed,
        )?
        else {
            return Ok(None);
        };

        pending.extend(calls.into_iter().map(|key| (key, depth.saturating_add(1))));
    }

    let template = request
        .semantic_values()
        .intern_dependency_contract_template(
            DependencyContractTemplateData::new([])
                .with_synchronous_cleanup(assumptions.iter().cloned()),
        )
        .expect("cleanup proof assumptions retain established semantic values");

    // Each backedge must preserve every precondition of the root, including those
    // discovered in transitive callees. This closes the proof under substitution.
    for cycle in cycles {
        let transformed = cycle.instantiate(request, template)?;

        let Some(calls) = expand(
            request,
            &mut cleanup,
            transformed.synchronous_cleanup(),
            &mut assumptions,
            false,
            completed,
        )?
        else {
            return Ok(None);
        };

        let mut pending = calls
            .into_iter()
            .map(|key| (key, 0_usize))
            .collect::<VecDeque<_>>();

        let mut seen = BTreeSet::new();

        while let Some((key, depth)) = pending.pop_front() {
            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            if !seen.insert(key) {
                continue;
            }

            if depth >= maximum_depth {
                return Ok(None);
            }

            let template = key.template(request)?;

            let Some(calls) = expand(
                request,
                &mut cleanup,
                template.synchronous_cleanup(),
                &mut assumptions,
                false,
                completed,
            )?
            else {
                return Ok(None);
            };

            pending.extend(calls.into_iter().map(|key| (key, depth.saturating_add(1))));
        }
    }

    if cleanup.diagnostics.has_errors() {
        return Ok(None);
    }

    Ok(Some(CleanupScheme {
        basis,
        assumptions: template,
    }))
}

fn expand<C, F>(
    request: CheckerUnitView<'_, C>,
    cleanup: &mut CleanupShapeResolver<'_, C>,
    requirements: &[DependencyRequirement],
    assumptions: &mut BTreeSet<DependencyRequirement>,
    collect: bool,
    completed: &mut F,
) -> Result<Option<Vec<CleanupKey>>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableContractsQuery> + ?Sized,
    F: FnMut(
        bray_bound_tree::LifecycleCallable,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
{
    let mut pending = requirements.to_vec();
    let mut seen = BTreeSet::new();
    let mut calls = Vec::new();

    while let Some(requirement) = pending.pop() {
        if !seen.insert(requirement.clone()) {
            continue;
        }

        if !collect && assumptions.contains(&requirement) {
            continue;
        }

        match requirement {
            DependencyRequirement::SynchronousCleanup(ty)
            | DependencyRequirement::FinalizerBodyCleanup(ty) => {
                let ty = crate::type_normalization::normalize_type_valued_members(
                    request.context(),
                    ty,
                    &mut cleanup.diagnostics,
                )?;

                let finalizer =
                    matches!(requirement, DependencyRequirement::FinalizerBodyCleanup(_));

                let leaf = if finalizer {
                    DependencyRequirement::FinalizerBodyCleanup(ty)
                } else {
                    DependencyRequirement::SynchronousCleanup(ty)
                };

                if matches!(
                    request.semantic_values().type_data(ty).as_ref(),
                    TypeData::TypeParameter(_)
                        | TypeData::ContextualSelf(_)
                        | TypeData::TypeValuedMemberProjection { .. }
                ) {
                    if collect {
                        assumptions.insert(leaf);
                    } else if !assumptions.contains(&leaf) {
                        return Ok(None);
                    }

                    continue;
                }

                if finalizer {
                    if let Some(lifecycle) = cleanup.pending_finalizer(ty, completed)? {
                        calls.push(CleanupKey::Callable(ResultKey {
                            callable: lifecycle.callable,
                            contextual_self: None,
                            output: bray_symbols::DependencyCallOutput::SynchronousCleanup,
                        }));
                    }

                    continue;
                }

                if !cleanup.cleanup_is_synchronous(ty, completed, &mut |requirement| {
                    pending.push(requirement);
                })? {
                    return Ok(None);
                }
            }
            DependencyRequirement::DefaultCleanup {
                provider,
                substitution,
                contextual_self,
            } => {
                calls.push(CleanupKey::Default {
                    provider,
                    substitution,
                    contextual_self,
                });
            }
            DependencyRequirement::ResultCall {
                callable,
                requirement: witness,
                ..
            } => {
                if let Some(key) = super::admission::selected_key(request, callable, witness)? {
                    calls.push(key);
                } else if collect {
                    assumptions.insert(requirement);
                } else {
                    return Ok(None);
                }
            }
            _ => panic!("cleanup proof retains only type predicates and selected callee edges"),
        }
    }

    calls
        .into_iter()
        .map(|key| key.normalize(request, &mut cleanup.diagnostics))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}
