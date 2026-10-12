use super::model::{ExecutionFlow, ExecutionFlowDomain, ReturnedDependencies};
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundControlTransferKind, BoundExpression, BoundExpressionId, CheckedExpressionSemantics,
};

use crate::execution_guarantees::ExecutionCondition;
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::{FixedPointOutcome, solve_fixed_point};
use super::super::super::model::ControlFlowGraph;

pub(in crate::analysis::guarantee) fn analyze_execution_flow<
    'a,
    'view,
    C: CheckerRequestContext + ?Sized,
>(
    graph: &'a ControlFlowGraph,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    storage: &'a bray_bound_tree::StoragePlan,
    refinements: &'a bray_bound_tree::CheckedRefinements,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    disposal: super::super::cleanup::Cleanup<'a>,
) -> CheckerOutcome<ExecutionFlow<'a, 'view, C>, C::UpstreamError> {
    let mut observed_types = BTreeMap::new();
    let mut borrowed_inputs = disposal.borrowed_inputs();
    let mut assigned_places = BTreeSet::new();
    let mut targets = BTreeMap::new();
    let mut yields = Vec::new();
    let mut sequence_lengths = Vec::new();

    let result_dependencies = match returned_dependencies(request, semantics) {
        Ok(inputs) => inputs,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    };

    for (expression, node) in request.unit().tree().expressions() {
        if let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            semantics.selections().expression(expression)
            && call.implementation_hook()
                == Some(bray_compiler_known::ImplementationHook::SequenceLength)
        {
            sequence_lengths.push(expression);
        }

        targets.insert(node.origin().source_anchor().syntax(), expression);

        if let BoundExpression::ControlTransfer(transfer) = node
            && transfer.kind() == BoundControlTransferKind::Yield
            && let Some(target) = transfer.target()
        {
            yields.push((expression, target));
        }

        if let BoundExpression::Assignment(assignment) = node
            && let Some(target) = assignment.operands().first()
            && let Some(BoundExpression::Name(name)) = request.view().expression(*target)
        {
            assigned_places.insert(name.target().into());
        }

        if let Some(place) = crate::execution_guarantees::expression_place(
            request.unit(),
            semantics,
            request.semantic_values(),
            expression,
        ) {
            let mut ty = semantics
                .types()
                .expression(expression)
                .map(|entry| entry.ty());

            if place.is_callable_input()
                && ty.is_some_and(|ty| {
                    matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    )
                })
            {
                borrowed_inputs.insert(place.clone());
            }

            while let Some(current) = ty
                && let bray_symbols::TypeData::Borrow { target, .. } =
                    request.semantic_values().type_data(current).as_ref()
            {
                ty = Some(*target);
            }

            observed_types
                .entry(place)
                .and_modify(|known| {
                    if *known != ty {
                        *known = None;
                    }
                })
                .or_insert(ty);
        }
    }

    let yield_targets = yields
        .into_iter()
        .filter_map(|(id, target)| targets.get(&target).copied().map(|target| (id, target)))
        .collect();

    let mut scope_places = BTreeMap::<_, Vec<_>>::new();

    if request.trusted_contracts().is_some_and(|contracts| {
        !contracts.requirements.is_empty()
            || !contracts.guarantees.is_empty()
            || !contracts.calls.is_empty()
    }) {
        let owners = bray_bound_tree::StorageScopeOwners::collect(request.unit())
            .expect("checked bound unit must have balanced lexical scopes");

        for (target, binding) in storage.bindings() {
            let identity = match binding {
                bray_bound_tree::StorageBinding::Identity(identity) => Some(*identity),
                bray_bound_tree::StorageBinding::Access(access) => storage.root_identity(*access),
            };

            // A borrowed input ends its local binding at return, while its caller retains
            // the reached storage and the authority carried by a returned projection.
            if identity.is_some_and(|identity| {
                matches!(
                    storage.identity(identity),
                    Some(
                        bray_bound_tree::StorageIdentity::Parameter(_)
                            | bray_bound_tree::StorageIdentity::Receiver(_)
                            | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
                    )
                ) && storage.identity_type(identity).is_some_and(|ty| {
                    matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    )
                })
            }) {
                continue;
            }

            if let Some(scope) =
                identity.and_then(|identity| owners.identity_scope(storage, identity))
                && let Some(reference) =
                    crate::execution_guarantees::storage_binding_reference(*target)
            {
                scope_places.entry(scope).or_default().push((
                    crate::ExecutionPlace::from(reference),
                    identity.expect("scoped binding has a storage identity"),
                ));
            }
        }
    }

    let domain = ExecutionFlowDomain {
        result_dependencies,
        sequence_lengths,
        observed_types,
        borrowed_inputs,
        assigned_places,
        scope_places,
        yield_targets,
        graph,
        request,
        semantics,
        assumptions,
        literals,
        storage,
        contracts,
        disposal,
        refinements,
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

    let states = match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(states) => states,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("execution condition flow exceeded its finite convergence bound");
        }
    };

    CheckerOutcome::without_diagnostics(ExecutionFlow { domain, states })
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

        let mut template = crate::dependency::call_result_template(request, call, |callable| {
            request.context().callable_result_dependencies(callable)
        })?;

        // Inference has already classified exact references. Only language borrows
        // become referent observations, while raw addresses retain value identity.
        let borrowed_results = template
            .template
            .borrows()
            .iter()
            .filter(|relation| {
                relation.exit().subject_root() == bray_symbols::DependencySubjectRoot::Result
                    && relation.kind() == bray_symbols::DependencyReferenceKind::Borrow
            })
            .cloned()
            .collect::<Vec<_>>();

        template.template = template.template.with_borrows(borrowed_results);

        let witness = request
            .trusted_contracts()
            .and_then(|contracts| contracts.calls.get(&expression.into()))
            .is_some_and(crate::TrustedCallContract::result_guarantees_name_whole_owner);

        if witness || !template.template.borrows().is_empty() {
            inputs.insert(
                expression,
                ReturnedDependencies {
                    inputs: crate::dependency::result_inputs(
                        call,
                        template.template.requirements(),
                    ),
                    template: template.template,
                },
            );
        }
    }

    Ok(inputs)
}
