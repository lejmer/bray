use std::collections::{BTreeMap, BTreeSet};

use crate::finalization::unresolved_finalization_diagnostic as unresolved_diagnostic;
use bray_bound_tree::{
    CallableProofCandidate, CallableProofDependency, CallableProofObligation, CallableProofTarget,
    CheckedAsync,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticKind, DiagnosticResult};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::FixedPointResult;
use super::super::super::model::{
    AnalysisCallPhase, AnalysisCleanupKind, AnalysisOperationKind, AnalysisScopeExitPhase,
};
use super::super::flow::{DomainState, GuaranteeDomain};
use super::observation::{CompletionReceiver, completion_receiver, part_was_moved};

pub(in crate::analysis::guarantee) fn completion_candidates<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    result: Option<&FixedPointResult<Result<DomainState, CheckerInfrastructureError>>>,
    asynchronous: &CheckedAsync,
) -> Result<DiagnosticResult<Vec<CallableProofCandidate>>, crate::CheckerQueryError<C::UpstreamError>>
{
    let mut candidates = CompletionCandidates::default();

    for block in domain.graph.blocks() {
        if request.is_cancelled() {
            return Err(crate::CheckerQueryError::Cancelled);
        }

        let state = result
            .and_then(|result| result.state(block.id()))
            .map(|state| state.as_ref().map_err(|error| *error))
            .transpose()?;

        if state.is_some_and(|state| !state.reachable) {
            continue;
        }

        // Replay owns its observations while each completed value can invalidate later proofs.
        let mut state = state.cloned();

        for operation in block
            .operations()
            .iter()
            .filter_map(|operation| domain.graph.operation(*operation))
        {
            if let AnalysisOperationKind::Call {
                expression,
                phase: AnalysisCallPhase::Attempt,
            } = operation.kind()
                && let Some(element) = domain.input.cleanup_call(expression)
            {
                candidates.cleanup_call(request, domain, expression, element)?;
            }

            if let AnalysisOperationKind::ScopeExit {
                block: scope,
                exit,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
                kind,
            } = operation.kind()
            {
                candidates.scope_exit(
                    request,
                    domain,
                    asynchronous,
                    scope,
                    exit,
                    kind,
                    &mut state,
                )?;
            }

            if let Some(state) = &mut state {
                domain.transfer_operation(operation, state)?;
            }
        }
    }

    candidates.finish(request)
}

/// Intersects completion evidence across paths and retains each unresolved obligation's diagnostic.
#[derive(Default)]
struct CompletionCandidates {
    candidates: BTreeMap<CallableProofObligation, Option<BTreeSet<CallableProofDependency>>>,
    required: BTreeMap<CallableProofObligation, (bray_symbols::TypeId, DiagnosticKind)>,
    nested: BTreeMap<
        (
            bray_bound_tree::AnyBoundNodeId,
            Option<bray_bound_tree::StorageAccessId>,
            bray_symbols::TypeId,
        ),
        DiagnosticKind,
    >,
    diagnostics: DiagnosticBag,
}

impl CompletionCandidates {
    fn cleanup_call<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        domain: &GuaranteeDomain<'_>,
        expression: bray_bound_tree::BoundExpressionId,
        element: bray_symbols::TypeId,
    ) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
        for (ty, diagnostic) in required_cleanup_types(
            request,
            domain,
            [element],
            AnalysisCleanupKind::Ordinary,
            false,
            expression.into(),
            &mut self.candidates,
        )? {
            self.nested
                .insert((expression.into(), None, ty), diagnostic);
        }

        let execution = match request.unit().root() {
            bray_bound_tree::BoundUnitRoot::CallableBody { execution, .. }
            | bray_bound_tree::BoundUnitRoot::AnonymousCallable { execution, .. } => {
                Some(execution)
            }
            _ => None,
        };

        if execution == Some(bray_symbols::CallableExecution::Synchronous) {
            let cleanup = crate::cleanup_type_execution_with(
                request.context(),
                element,
                |_| Ok(None),
                |ty| {
                    type_completion_candidate(domain, ty, expression.into(), &mut self.candidates)
                        .map_err(crate::CheckerQueryError::Infrastructure)
                },
            )?;

            self.diagnostics.add_range(cleanup.diagnostics().clone());

            if [
                cleanup.value().finalization_execution(),
                cleanup.value().destruction_execution(),
            ]
            .contains(&Some(bray_symbols::CallableExecution::Asynchronous))
            {
                self.nested.insert(
                    (expression.into(), None, element),
                    DiagnosticKind::CheckingAsyncFinalizationInSynchronousContext,
                );
            }
        }

        Ok(())
    }

    fn scope_exit<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        domain: &GuaranteeDomain<'_>,
        asynchronous: &CheckedAsync,
        scope: bray_bound_tree::BoundBlockId,
        exit: bray_bound_tree::AnyBoundNodeId,
        kind: AnalysisCleanupKind,
        state: &mut Option<DomainState>,
    ) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
        for plan in asynchronous
            .scope_exits()
            .iter()
            .filter(|plan| plan.scope() == scope && plan.exit() == exit)
        {
            for access in plan.lifecycle_resolution() {
                for (ordinal, part) in domain.cleanup_targets(*access) {
                    if part.is_some_and(|part| part.release().is_some()) {
                        if let Some(state) = state.as_mut() {
                            domain.invalidate_cleanup_observations(
                                state,
                                *access,
                                &[],
                                bray_bound_tree::StorageExitPoint::new(scope, exit),
                            )?;
                        }

                        continue;
                    }

                    if part.is_some_and(|part| {
                        !part.phases().includes_lifecycle()
                            || part_was_moved(domain.storage, plan, *access, part)
                    }) {
                        continue;
                    }

                    let path =
                        part.map_or(&[][..], bray_bound_tree::StorageCleanupPart::projections);

                    let inherited = super::inherited::inherited_destructor_part(
                        request,
                        domain,
                        asynchronous,
                        *access,
                        path,
                    );

                    let Some(ty) = path
                        .last()
                        .map(|projection| projection.result_type())
                        .or_else(|| {
                            domain
                                .storage
                                .access(*access)
                                .map(|access| access.reached_type())
                        })
                    else {
                        continue;
                    };

                    let ordinal = match ordinal.map(u32::try_from).transpose() {
                        Ok(ordinal) => ordinal.map(bray_symbols::SymbolOrdinal::new),
                        Err(_) => {
                            if let Some(diagnostic) =
                                completion_requirement(request, domain, ty, kind, inherited)?
                            {
                                self.nested.insert((exit, Some(*access), ty), diagnostic);
                            }

                            continue;
                        }
                    };

                    let receiver = match state.as_ref() {
                        Some(state) => completion_receiver(domain, state, *access, path)?,
                        None => CompletionReceiver::Unknown,
                    };

                    let obligation = CallableProofObligation::Finalization {
                        scope,
                        exit,
                        access: *access,
                        part: ordinal,
                    };

                    if matches!(receiver, CompletionReceiver::Absent) {
                        // This candidate owns the evidence establishing payload absence.
                        let proof = state.as_ref().map(|state| state.dependencies.clone());

                        merge_candidate(&mut self.candidates, obligation, proof);

                        continue;
                    }

                    if let Some(diagnostic) =
                        completion_requirement(request, domain, ty, kind, inherited)?
                    {
                        self.required.insert(obligation, (ty, diagnostic));
                    }

                    let complete_value = !plan.is_recovered()
                        && domain.storage.is_root_access(*access)
                        && !(part.is_none()
                            && plan.moved().iter().any(|moved| {
                                domain.storage.root_identity(*moved)
                                    == domain.storage.root_identity(*access)
                            }));

                    let proof = if !complete_value {
                        None
                    } else if let (Some(state), CompletionReceiver::Value(receiver)) =
                        (state.as_ref(), receiver)
                    {
                        domain.finalizer_completion_dependencies(state, ty, receiver, exit)?
                    } else if matches!(receiver, CompletionReceiver::Unknown) {
                        type_completion_dependencies(domain, ty, exit)?
                    } else {
                        None
                    };

                    let mut synchronous = if complete_value
                        && (domain.input.finalizer(ty).is_none() || proof.is_some())
                        && domain
                            .input
                            .lifecycle(ty, bray_symbols::TypeAssociatedLifecycleSlot::Destructor)
                            .is_some()
                        && let (Some(state), CompletionReceiver::Value(receiver)) =
                            (state.as_ref(), receiver)
                    {
                        domain
                            .synchronous_destruction_dependencies(state, ty, receiver, exit)?
                            .map(|mut dependencies| {
                                dependencies.extend(proof.iter().flatten().copied());

                                dependencies
                            })
                    } else {
                        None
                    };

                    if synchronous.is_none()
                        && complete_value
                        && asynchronous.cleanup_types().iter().any(|cleanup| {
                            cleanup.ty() == ty
                                && cleanup.destruction_execution()
                                    == Some(bray_symbols::CallableExecution::Asynchronous)
                        })
                    {
                        synchronous = type_synchronous_destruction(
                            request,
                            domain,
                            ty,
                            exit,
                            &mut self.diagnostics,
                        )?;
                    }

                    if synchronous.is_none() {
                        for (ty, diagnostic) in required_cleanup_types(
                            request,
                            domain,
                            domain.input.cleanup_dependencies(ty),
                            kind,
                            inherited,
                            exit,
                            &mut self.candidates,
                        )? {
                            self.nested.insert((exit, Some(*access), ty), diagnostic);
                        }
                    }

                    // This separate obligation is certified with the selected body's proofs.
                    // Its failure rejects the caller instead of assuming synchronous cleanup.
                    merge_candidate(
                        &mut self.candidates,
                        CallableProofObligation::SynchronousDestruction {
                            scope,
                            exit,
                            access: *access,
                            part: ordinal,
                        },
                        synchronous,
                    );

                    let destruction =
                        if let (Some(state), Some(_), CompletionReceiver::Value(receiver)) =
                            (state.as_ref(), &proof, receiver)
                        {
                            domain.destruction_dependencies(
                                state,
                                ty,
                                receiver,
                                exit,
                                bray_symbols::ExecutionProperty::Pure,
                            )?
                        } else {
                            None
                        };

                    if let (Some(state), Some(destruction), Some(proof)) =
                        (state.as_mut(), &destruction, &proof)
                    {
                        state.dependencies.extend(proof.iter().copied());
                        state.dependencies.extend(destruction.iter().copied());
                    }

                    merge_candidate(&mut self.candidates, obligation, proof);

                    // Effectful or unverified destruction requires fresh observations.
                    if destruction.is_none()
                        && let Some(state) = state.as_mut()
                    {
                        domain.invalidate_cleanup_observations(
                            state,
                            *access,
                            path,
                            bray_bound_tree::StorageExitPoint::new(scope, exit),
                        )?;
                    }
                }
            }
        }

        Ok(())
    }

    fn finish<C: CheckerRequestContext + ?Sized>(
        mut self,
        request: CheckerUnitView<'_, C>,
    ) -> Result<
        DiagnosticResult<Vec<CallableProofCandidate>>,
        crate::CheckerQueryError<C::UpstreamError>,
    > {
        let mut proven = Vec::new();

        for ((exit, _, ty), kind) in self.nested {
            self.diagnostics.add(unresolved_diagnostic(
                request,
                exit,
                ty,
                kind,
                self.diagnostics.len(),
            )?);
        }

        for (obligation, dependencies) in self.candidates {
            if let Some(dependencies) = dependencies {
                proven.push(CallableProofCandidate::new(obligation, dependencies));
            } else if let Some((ty, kind)) = self.required.get(&obligation)
                && let CallableProofObligation::Finalization { exit, .. } = obligation
            {
                self.diagnostics.add(unresolved_diagnostic(
                    request,
                    exit,
                    *ty,
                    *kind,
                    self.diagnostics.len(),
                )?);
            }
        }

        Ok(DiagnosticResult::new(proven, self.diagnostics))
    }
}

fn merge_candidate(
    candidates: &mut BTreeMap<CallableProofObligation, Option<BTreeSet<CallableProofDependency>>>,
    obligation: CallableProofObligation,
    proof: Option<BTreeSet<CallableProofDependency>>,
) {
    candidates
        .entry(obligation)
        .and_modify(|previous| match (previous.as_mut(), proof.as_ref()) {
            (Some(previous), Some(proof)) => previous.extend(proof.iter().copied()),
            _ => *previous = None,
        })
        .or_insert(proof);
}

fn required_cleanup_types<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    roots: impl IntoIterator<Item = bray_symbols::TypeId>,
    kind: AnalysisCleanupKind,
    inherited: bool,
    site: bray_bound_tree::AnyBoundNodeId,
    candidates: &mut BTreeMap<CallableProofObligation, Option<BTreeSet<CallableProofDependency>>>,
) -> Result<
    BTreeMap<bray_symbols::TypeId, DiagnosticKind>,
    crate::CheckerQueryError<C::UpstreamError>,
> {
    let mut pending = roots.into_iter().collect::<Vec<_>>();
    let mut visited = BTreeSet::new();
    let mut required = BTreeMap::new();

    while let Some(ty) = pending.pop() {
        if request.is_cancelled() {
            return Err(crate::CheckerQueryError::Cancelled);
        }

        if !visited.insert(ty) {
            continue;
        }

        if !type_completion_candidate(domain, ty, site, candidates)?
            && let Some(diagnostic) = completion_requirement(request, domain, ty, kind, inherited)?
        {
            required.insert(ty, diagnostic);
        }

        pending.extend(domain.input.cleanup_dependencies(ty));
    }

    Ok(required)
}

fn type_completion_candidate(
    domain: &GuaranteeDomain<'_>,
    ty: bray_symbols::TypeId,
    site: bray_bound_tree::AnyBoundNodeId,
    candidates: &mut BTreeMap<CallableProofObligation, Option<BTreeSet<CallableProofDependency>>>,
) -> Result<bool, CheckerInfrastructureError> {
    let Some(dependencies) = type_completion_dependencies(domain, ty, site)? else {
        return Ok(false);
    };

    merge_candidate(
        candidates,
        CallableProofObligation::TypeFinalization { site, ty },
        Some(dependencies),
    );

    Ok(true)
}

fn type_completion_dependencies(
    domain: &GuaranteeDomain<'_>,
    ty: bray_symbols::TypeId,
    site: bray_bound_tree::AnyBoundNodeId,
) -> Result<Option<BTreeSet<CallableProofDependency>>, CheckerInfrastructureError> {
    let Some(finalizer) = domain.input.finalizer(ty) else {
        return Ok(None);
    };

    let receiver = domain
        .values
        .intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(
            bray_symbols::SymbolOrdinal::new(0),
        ))
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let Some(obligations) = crate::finalization::finalizer_completion_requirements(
        domain.values,
        domain.available,
        finalizer.conditions(),
        &[receiver],
        finalizer.result(),
        &[],
        &mut |condition, arguments| {
            crate::instantiate_condition(domain.values, condition, arguments)
        },
    )?
    else {
        return Ok(None);
    };

    let target = CallableProofTarget::Implicit {
        site,
        callable: finalizer.callable(),
    };

    Ok(Some(
        obligations
            .into_iter()
            .map(|obligation| CallableProofDependency::new(target, obligation))
            .collect(),
    ))
}
fn type_synchronous_destruction<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    ty: bray_symbols::TypeId,
    site: bray_bound_tree::AnyBoundNodeId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Option<BTreeSet<CallableProofDependency>>, crate::CheckerQueryError<C::UpstreamError>> {
    let mut candidates = BTreeMap::new();

    let cleanup = crate::cleanup_type_execution_with(
        request.context(),
        ty,
        |_| Ok(None),
        |ty| {
            type_completion_candidate(domain, ty, site, &mut candidates)
                .map_err(crate::CheckerQueryError::Infrastructure)
        },
    )?;

    let (cleanup, checked_diagnostics) = cleanup.into_parts();

    let valid = !checked_diagnostics.has_errors();

    diagnostics.add_range(checked_diagnostics);

    Ok((valid
        && cleanup.destruction_execution() == Some(bray_symbols::CallableExecution::Synchronous))
    .then(|| candidates.into_values().flatten().flatten().collect()))
}

fn completion_requirement<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    ty: bray_symbols::TypeId,
    kind: AnalysisCleanupKind,
    inherited: bool,
) -> Result<Option<DiagnosticKind>, crate::CheckerQueryError<C::UpstreamError>> {
    let Some(finalizer) = domain.input.finalizer(ty) else {
        return Ok(None);
    };

    let asynchronous = matches!(
        request.unit().root(),
        bray_bound_tree::BoundUnitRoot::CallableBody {
            execution: bray_symbols::CallableExecution::Asynchronous,
            ..
        } | bray_bound_tree::BoundUnitRoot::AnonymousCallable {
            execution: bray_symbols::CallableExecution::Asynchronous,
            ..
        }
    );

    if !asynchronous
        && !inherited
        && finalizer.execution() == bray_symbols::CallableExecution::Asynchronous
    {
        return Ok(Some(
            DiagnosticKind::CheckingAsyncFinalizationInSynchronousContext,
        ));
    }

    Ok((kind == AnalysisCleanupKind::Ordinary
        && crate::representation::type_representation_for_context(
            request.context(),
            finalizer.result(),
        )? == Some(bray_compiler_known::RepresentationRole::Result))
    .then_some(DiagnosticKind::CheckingUnresolvedFinalization))
}

impl GuaranteeDomain<'_> {
    pub(in crate::analysis::guarantee) fn finalizer_completion_dependencies(
        &self,
        state: &DomainState,
        ty: bray_symbols::TypeId,
        receiver: bray_symbols::ConstantTermId,
        exit: bray_bound_tree::AnyBoundNodeId,
    ) -> Result<Option<BTreeSet<CallableProofDependency>>, CheckerInfrastructureError> {
        let domain = self;

        let Some(finalizer) = domain.input.finalizer(ty) else {
            return Ok(None);
        };

        let assumptions = state.conditions.iter().copied().collect::<Vec<_>>();

        let Some(obligations) = crate::finalization::finalizer_completion_requirements(
            domain.values,
            domain.available,
            finalizer.conditions(),
            &[receiver],
            finalizer.result(),
            &assumptions,
            &mut |condition, arguments| {
                let Some(condition) =
                    crate::instantiate_condition(domain.values, condition, arguments)?
                else {
                    return Ok(None);
                };

                domain.current_observation(state, condition, None, arguments.get(1).copied())
            },
        )?
        else {
            return Ok(None);
        };

        let target = CallableProofTarget::Implicit {
            site: exit,
            callable: finalizer.callable(),
        };

        let mut dependencies = state.dependencies.clone();

        dependencies.extend(
            obligations
                .into_iter()
                .map(|obligation| CallableProofDependency::new(target, obligation)),
        );

        Ok(Some(dependencies))
    }
}
