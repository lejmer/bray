use super::model::{ExecutionFlow, ExecutionFlowDomain, ReturnedDependencies};
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpressionId, CheckedExpressionSemantics};

use crate::execution_guarantees::ExecutionCondition;
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::{FixedPointOutcome, solve_fixed_point};

pub(in crate::analysis::guarantee) fn analyze_execution_flow<
    'a,
    'view,
    C: CheckerRequestContext + ?Sized,
>(
    prepared: &'a crate::PreparedBodyAnalysis,
    excluded_edges: BTreeSet<crate::analysis::id::AnalysisEdgeId>,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    storage: &'a bray_bound_tree::StoragePlan,
    memory: &'a bray_bound_tree::CheckedMemoryOperations,
    checked_flow: &'a bray_bound_tree::StorageFlow,
    refinements: &'a std::collections::BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    disposal: &'a super::super::cleanup::Cleanup<'a>,
) -> CheckerOutcome<ExecutionFlow<'a, 'view, C>, C::UpstreamError> {
    let mut domain = match prepare_execution_flow(
        prepared,
        request,
        semantics,
        assumptions,
        storage,
        memory,
        refinements,
        contracts,
        disposal,
    ) {
        CheckerOutcome::Complete(result) => result.into_parts().0,
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    domain.excluded_edges = excluded_edges;
    domain.checked_flow = Some(checked_flow);

    let states = match solve_fixed_point(&prepared.graph, &domain, &request) {
        FixedPointOutcome::Complete(states) => states,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("execution condition flow exceeded its finite convergence bound");
        }
    };

    CheckerOutcome::without_diagnostics(ExecutionFlow { domain, states })
}

pub(crate) fn prepare_execution_flow<'a, 'view, C: CheckerRequestContext + ?Sized>(
    prepared: &'a crate::PreparedBodyAnalysis,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    storage: &'a bray_bound_tree::StoragePlan,
    memory: &'a bray_bound_tree::CheckedMemoryOperations,
    refinements: &'a std::collections::BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    disposal: &'a super::super::cleanup::Cleanup<'a>,
) -> CheckerOutcome<ExecutionFlowDomain<'a, 'view, C>, C::UpstreamError> {
    let result_dependencies = match returned_dependencies(request, semantics) {
        Ok(inputs) => inputs,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    };

    let domain = ExecutionFlowDomain {
        result_dependencies,
        sequence_lengths: &prepared.sequence_lengths,
        observed_types: &prepared.observed_types,
        borrowed_inputs: &prepared.borrowed_inputs,
        assigned_places: &prepared.assigned_places,
        scope_places: &prepared.scope_places,
        yield_targets: &prepared.yield_targets,
        graph: &prepared.graph,
        excluded_edges: BTreeSet::new(),
        request,
        semantics,
        assumptions,
        literals: &prepared.literals,
        storage,
        contracts,
        disposal,
        refinements,
        memory,
        checked_flow: None,
        spatial_predicates: request
            .trusted_contracts()
            .filter(|contracts| !contracts.requirements.is_empty() || !contracts.calls.is_empty())
            .into_iter()
            .flat_map(|_| {
                [
                    "ValidRead",
                    "ValidWrite",
                    "AlignedFor",
                    "DeviceValidRead",
                    "DeviceValidWrite",
                    "DeviceAlignedFor",
                ]
            })
            .filter_map(|name| {
                let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                    .expect("closed spatial predicate key must be valid");

                request
                    .context()
                    .available_compiler_known_symbols()
                    .provider()
                    .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                    .map(Into::into)
            })
            .collect(),
        owned_allocation: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("OwnedAllocation")
                    .expect("closed allocation predicate key must be valid"),
            )
            .map(Into::into),
        initialized_range: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("InitializedRangeAs")
                    .expect("closed initialized range predicate key must be valid"),
            )
            .map(Into::into),
    };

    CheckerOutcome::without_diagnostics(domain)
}

fn returned_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    semantics: &CheckedExpressionSemantics,
) -> Result<BTreeMap<BoundExpressionId, ReturnedDependencies>, CheckerQueryError<C::UpstreamError>>
{
    let mut inputs = BTreeMap::new();

    for selection in semantics.selections().entries() {
        let bray_bound_tree::SemanticSelection::Call(call) = selection.selection() else {
            continue;
        };

        let expression = selection.expression();

        let template = crate::dependency::call_result_template(request, call, |callable| {
            request.callable_result_dependencies(callable)
        })?;

        let mut references = Vec::new();
        let mut has_returned_reference = false;

        for relation in template.template.borrows().iter().filter(|relation| {
            relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
        }) {
            has_returned_reference = true;

            if preserves_reference_value(request, call, relation)? {
                references.push(relation.clone());
            }
        }

        let witness = request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&expression.into()))
            .is_some_and(crate::TrustedCallContract::result_guarantees_name_whole_owner);

        if witness || has_returned_reference {
            inputs.insert(
                expression,
                ReturnedDependencies {
                    inputs: crate::dependency::result_inputs(call, &template.template),
                    references,
                },
            );
        }
    }

    Ok(inputs)
}

fn preserves_reference_value<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &bray_bound_tree::SelectedCall,
    relation: &bray_symbols::DependencyBorrowRelation,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    if relation.kind() == bray_symbols::DependencyReferenceKind::Borrow {
        return Ok(true);
    }

    let mut ty = match relation.entry().subject_root() {
        bray_symbols::DependencySubjectRoot::Receiver => {
            call.receiver().map(|receiver| receiver.target_type())
        }
        bray_symbols::DependencySubjectRoot::Parameter(parameter) => {
            call.arguments().iter().find_map(|argument| match argument {
                bray_bound_tree::SelectedArgument::Explicit {
                    ordinal,
                    conversion,
                    ..
                } if *ordinal == parameter.raw() => Some(conversion.target_type()),
                _ => None,
            })
        }
        _ => None,
    };

    for projection in relation.entry().projections() {
        let Some(owner) = ty else {
            return Ok(false);
        };

        let projected = crate::storage::projected_value_type(request, owner, *projection)?;

        ty = *projected.value();
    }

    // A physical-reference relation can describe address formation from a borrow.
    // That keeps its owner dependency, but does not equate the address with the
    // borrowed value or its contents. Only an existing address forwards its value.
    Ok(ty.is_some_and(|ty| {
        crate::representation::type_representation(request, ty)
            == Some(bray_compiler_known::RepresentationRole::RawPointer)
    }))
}
