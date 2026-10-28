use std::collections::{BTreeMap, BTreeSet, VecDeque};

use bray_diagnostics::{DiagnosticKind, DiagnosticResult};
use bray_symbols::{
    CallableContractsQuery, DependencyCallOutput, DependencyRequirement, GenericArgument,
    GenericSubstitutionData,
};

use crate::asynchronous::CleanupShapeResolver;
use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView,
};

use super::super::parameters::ResultKey;
use super::key::CleanupKey;

pub(crate) fn check_synchronous_cleanup<C, F>(
    request: CheckerUnitView<'_, C>,
    mut cleanup: CleanupShapeResolver<'_, C>,
    requirements: &[DependencyRequirement],
    completed: &mut F,
) -> Result<DiagnosticResult<()>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableContractsQuery> + ?Sized,
    F: FnMut(
        bray_bound_tree::LifecycleCallable,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
{
    let maximum_depth = request.context().maximum_recursion_depth();
    let mut exact = BTreeSet::new();
    let mut checked = BTreeSet::new();
    let mut schemes = Vec::<super::induction::CleanupScheme>::new();
    let mut proofs = BTreeMap::new();

    let Some(roots) = stage_requirements(request, &mut cleanup, requirements, completed)? else {
        return Ok(DiagnosticResult::new((), cleanup.diagnostics));
    };

    let mut pending = roots
        .into_iter()
        .map(|key| (key, 1_usize))
        .collect::<VecDeque<_>>();

    while let Some((selected, depth)) = pending.pop_front() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        if !exact.insert(selected) {
            continue;
        }

        let template = selected.template(request)?;

        let Some(calls) = stage_requirements(
            request,
            &mut cleanup,
            template.synchronous_cleanup(),
            completed,
        )?
        else {
            break;
        };

        let values = request.semantic_values();

        let is_open = !values.substitution_is_concrete(selected.substitution())
            || selected
                .contextual_self()
                .is_some_and(|(_, subject)| !values.type_is_concrete(subject));

        let key = if is_open {
            partial_key(request, selected)?
        } else {
            if !schemes
                .iter()
                .any(|scheme| super::induction::matches_basis(request, scheme.basis, selected))
                && let Some(scheme) = super::induction::generalize(
                    request,
                    selected,
                    maximum_depth,
                    &mut proofs,
                    completed,
                )?
            {
                schemes.push(scheme);
            }

            let scheme = schemes
                .iter()
                .find(|scheme| super::induction::matches_basis(request, scheme.basis, selected));

            if let Some(scheme) = scheme {
                let assumptions = selected.instantiate(request, scheme.assumptions)?;

                let Some(obligations) = stage_requirements(
                    request,
                    &mut cleanup,
                    assumptions.synchronous_cleanup(),
                    completed,
                )?
                else {
                    break;
                };

                pending.extend(
                    obligations
                        .into_iter()
                        .map(|key| (key, depth.saturating_add(1))),
                );

                scheme.basis
            } else {
                selected
            }
        };

        if !checked.insert((is_open, key)) {
            continue;
        }

        if depth > maximum_depth {
            let owner = request
                .symbols()
                .runtime_default_subject(selected.owner())
                .unwrap_or(selected.owner());

            crate::diagnostic::report_cleanup_recursion_limit(
                request,
                Some(owner),
                depth,
                maximum_depth,
                &mut cleanup.diagnostics,
            );

            break;
        }

        pending.extend(calls.into_iter().map(|key| (key, depth.saturating_add(1))));
    }

    Ok(DiagnosticResult::new((), cleanup.diagnostics))
}

fn stage_requirements<C, F>(
    request: CheckerUnitView<'_, C>,
    cleanup: &mut CleanupShapeResolver<'_, C>,
    requirements: &[DependencyRequirement],
    completed: &mut F,
) -> Result<Option<Vec<CleanupKey>>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableContractsQuery> + ?Sized,
    F: FnMut(
        bray_bound_tree::LifecycleCallable,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>>,
{
    let mut dependencies = Vec::new();
    let mut calls = Vec::new();

    for requirement in requirements {
        match requirement {
            DependencyRequirement::SynchronousCleanup(ty) => {
                let ty = crate::type_normalization::normalize_type_valued_members(
                    request.context(),
                    *ty,
                    &mut cleanup.diagnostics,
                )?;

                if !request.semantic_values().type_is_concrete(ty) {
                    continue;
                }

                let synchronous =
                    cleanup.cleanup_is_synchronous(ty, completed, &mut |requirement| {
                        dependencies.push(requirement);
                    })?;

                if !synchronous {
                    if !cleanup.diagnostics.has_errors() {
                        crate::lifecycle::report_finalization(
                            request,
                            request.unit().root().into(),
                            ty,
                            DiagnosticKind::CheckingAsyncFinalizationOutsideAsyncCallable,
                            &mut cleanup.diagnostics,
                        )?;
                    }

                    return Ok(None);
                }
            }
            DependencyRequirement::FinalizerBodyCleanup(ty) => {
                let ty = crate::type_normalization::normalize_type_valued_members(
                    request.context(),
                    *ty,
                    &mut cleanup.diagnostics,
                )?;

                if !request.semantic_values().type_is_concrete(ty) {
                    continue;
                }

                if let Some(lifecycle) = cleanup.pending_finalizer(ty, completed)? {
                    calls.push(CleanupKey::Callable(ResultKey {
                        callable: lifecycle.callable,
                        contextual_self: None,
                        output: DependencyCallOutput::SynchronousCleanup,
                    }));
                }
            }
            DependencyRequirement::DefaultCleanup {
                provider,
                substitution,
                contextual_self,
            } => {
                calls.push(CleanupKey::Default {
                    provider: *provider,
                    substitution: *substitution,
                    contextual_self: *contextual_self,
                });
            }
            DependencyRequirement::ResultCall {
                output,
                callable,
                requirement,
                inputs,
            } => {
                assert_eq!(
                    *output,
                    DependencyCallOutput::SynchronousCleanup,
                    "cleanup output retains only cleanup call edges"
                );

                assert!(
                    inputs.is_empty(),
                    "type cleanup does not retain value argument dependencies"
                );

                if let Some(key) = selected_key(request, *callable, *requirement)? {
                    calls.push(key);
                }
            }
            _ => panic!("cleanup output contains only selected type obligations and callee edges"),
        }
    }

    for dependency in dependencies {
        let DependencyRequirement::ResultCall {
            callable,
            requirement,
            ..
        } = dependency
        else {
            panic!("a concrete cleanup type retains no unresolved type assumptions");
        };

        if let Some(key) = selected_key(request, callable, requirement)? {
            calls.push(key);
        }
    }

    calls
        .into_iter()
        .map(|key| key.normalize(request, &mut cleanup.diagnostics))
        .collect::<Result<Vec<_>, _>>()
        .map(Some)
}

fn partial_key<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    key: CleanupKey,
) -> Result<CleanupKey, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();

    let data = values.generic_substitution_data(key.substitution());

    let retained = data
        .bindings()
        .iter()
        .filter(|binding| match binding.argument() {
            GenericArgument::Type(ty) => values.type_is_concrete(ty),
            GenericArgument::Constant(term) => values.constant_term_is_closed(term),
        })
        .collect::<Vec<_>>();

    let substitution = GenericSubstitutionData::try_new(
        data.owner(),
        retained.iter().map(|binding| binding.parameter()),
        retained.iter().map(|binding| binding.argument()),
    )
    .expect("partial cleanup identity preserves selected binding kinds");

    let substitution = values
        .intern_generic_substitution(substitution)
        .expect("partial cleanup identity retains established semantic values");

    let contextual_self = key.contextual_self().map(|(context, subject)| {
        let subject = if values.type_is_concrete(subject) {
            subject
        } else {
            values
                .intern_type(bray_symbols::TypeData::ContextualSelf(context))
                .expect("partial cleanup preserves its selected Self context")
        };

        (context, subject)
    });

    let key = key.with_substitution(substitution);

    Ok(if key.contextual_self().is_some() {
        key.with_contextual_self(contextual_self)
    } else {
        key
    })
}

pub(super) fn selected_key<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    callable: bray_symbols::CallableInstanceId,
    requirement: Option<bray_symbols::ImplementationRequirementKey>,
) -> Result<Option<CleanupKey>, CheckerQueryError<C::UpstreamError>> {
    let (callable, contextual_self) = match requirement {
        Some(requirement) => {
            let Some((selected, context, _)) =
                request.context().witness_callable(callable, requirement)?
            else {
                return Ok(None);
            };

            (selected, Some((context, requirement.subject())))
        }
        None => (
            *request.semantic_values().callable_instance_data(callable),
            None,
        ),
    };

    Ok(Some(CleanupKey::Callable(ResultKey {
        callable,
        contextual_self,
        output: DependencyCallOutput::SynchronousCleanup,
    })))
}
