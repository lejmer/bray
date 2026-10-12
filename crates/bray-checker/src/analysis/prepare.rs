use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{
    BoundControlTransferKind, BoundExpression, BoundExpressionId, BoundUnitKind,
    CheckedExpressionSemantics, CheckedPatterns, StoragePlan,
};
use bray_symbols::TypeId;

use super::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use super::model::ControlFlowGraph;
use crate::asynchronous::{CleanupShape, CleanupShapeResolver};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

/// Immutable selected control flow and cleanup shapes shared by body analysis consumers.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PreparedBodyAnalysis {
    pub(crate) graph: ControlFlowGraph,
    pub(crate) cleanup: super::guarantee::CleanupPlan,
    pub(crate) sequence_lengths: Vec<BoundExpressionId>,
    pub(crate) observed_types: BTreeMap<crate::ExecutionPlace, Option<TypeId>>,
    pub(crate) borrowed_inputs: BTreeSet<crate::ExecutionPlace>,
    pub(crate) assigned_places: BTreeSet<crate::ExecutionPlace>,
    pub(crate) scope_places: BTreeMap<
        bray_bound_tree::BoundBlockId,
        Vec<(crate::ExecutionPlace, bray_bound_tree::StorageIdentityId)>,
    >,
    pub(crate) yield_targets: BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(crate) literals: BTreeMap<BoundExpressionId, crate::ExecutionCondition>,
    kind: BoundUnitKind,
    cleanup_shapes: Arc<BTreeMap<TypeId, CleanupShape>>,
}

impl PreparedBodyAnalysis {
    pub(crate) const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    pub(crate) fn cleanup_shapes<'a, C: CheckerRequestContext + ?Sized>(
        &self,
        request: CheckerUnitView<'a, C>,
    ) -> CleanupShapeResolver<'a, C> {
        let mut resolver = CleanupShapeResolver::new(request);

        resolver.completed = Arc::clone(&self.cleanup_shapes);

        resolver
    }
}

/// Prepares selected execution once, before contextual proof and ownership flow.
pub fn prepare_body_analysis<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
) -> CheckerOutcome<PreparedBodyAnalysis, C::UpstreamError> {
    crate::unit::assert_unit_inputs(
        request,
        [
            (
                "expression semantics",
                (expressions.unit(), expressions.kind()),
            ),
            ("patterns", (patterns.unit(), patterns.kind())),
            ("storage plan", (storage.unit(), storage.kind())),
        ],
    );

    let mut cleanup_shapes = CleanupShapeResolver::new(request);

    let graph = match build_storage_control_flow_graph(
        &mut cleanup_shapes,
        request,
        storage,
        expressions.selections(),
        Some(patterns),
        Some(expressions),
    ) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
        ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let cleanup =
        match super::guarantee::CleanupPlan::prepare(&mut cleanup_shapes, request, storage) {
            Ok(result) => {
                let (cleanup, diagnostics) = result.into_parts();

                cleanup_shapes.diagnostics.add_range(diagnostics);

                cleanup
            }
            Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(crate::CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

    let mut prepared = PreparedBodyAnalysis {
        graph,
        cleanup,
        kind: request.unit().key().kind(),
        sequence_lengths: Vec::new(),
        observed_types: BTreeMap::new(),
        borrowed_inputs: BTreeSet::new(),
        assigned_places: BTreeSet::new(),
        scope_places: BTreeMap::new(),
        yield_targets: BTreeMap::new(),
        literals: crate::execution_guarantees::condition_literals(
            expressions,
            request.semantic_values(),
        ),
        cleanup_shapes: cleanup_shapes.completed,
    };

    prepared.prepare_observations(request, expressions);
    prepared.prepare_scope_places(request, storage);

    CheckerOutcome::complete(prepared, cleanup_shapes.diagnostics)
}

impl PreparedBodyAnalysis {
    fn prepare_observations<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        semantics: &CheckedExpressionSemantics,
    ) {
        let mut targets = BTreeMap::new();
        let mut yields = Vec::new();

        for (expression, node) in request.unit().tree().expressions() {
            if let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                semantics.selections().expression(expression)
                && call.implementation_hook()
                    == Some(bray_compiler_known::ImplementationHook::SequenceLength)
            {
                self.sequence_lengths.push(expression);
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
                self.assigned_places.insert(name.target().into());
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
                    self.borrowed_inputs.insert(place.clone());
                }

                while let Some(current) = ty
                    && let bray_symbols::TypeData::Borrow { target, .. } =
                        request.semantic_values().type_data(current).as_ref()
                {
                    ty = Some(*target);
                }

                self.observed_types
                    .entry(place)
                    .and_modify(|known| {
                        if *known != ty {
                            *known = None;
                        }
                    })
                    .or_insert(ty);
            }
        }

        self.yield_targets = yields
            .into_iter()
            .filter_map(|(id, target)| targets.get(&target).copied().map(|target| (id, target)))
            .collect();
    }

    fn prepare_scope_places<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) {
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
                self.scope_places.entry(scope).or_default().push((
                    crate::ExecutionPlace::from(reference),
                    identity.expect("scoped binding has a storage identity"),
                ));
            }
        }
    }
}
