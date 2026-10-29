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
    pub(crate) generator_observations: bray_bound_tree::GeneratorObservations,
    pub(crate) values: super::liveness::ValueRetention,
    pub(crate) cleanup: super::guarantee::CleanupPlan,
    pub(crate) sequence_lengths: Vec<BoundExpressionId>,
    pub(crate) has_bounds_checks: bool,
    pub(crate) observed_types: BTreeMap<crate::ExecutionPlace, Option<TypeId>>,
    pub(crate) borrowed_inputs: BTreeSet<crate::ExecutionPlace>,
    pub(crate) input_invariants: Vec<crate::ExecutionCondition>,
    pub(crate) scope_places: BTreeMap<
        bray_bound_tree::StorageScope,
        Vec<(crate::ExecutionPlace, bray_bound_tree::StorageIdentityId)>,
    >,
    pub(crate) yield_targets: BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(crate) literals: BTreeMap<BoundExpressionId, crate::ExecutionCondition>,
    pub(crate) pattern_conditions: BTreeMap<
        bray_bound_tree::BoundPatternId,
        (bray_bound_tree::BoundOperator, crate::ExecutionCondition),
    >,
    kind: BoundUnitKind,
    cleanup_shapes: Arc<BTreeMap<TypeId, CleanupShape>>,
}

impl PreparedBodyAnalysis {
    /// Returns the established generator observations available to predicate normalization.
    pub const fn generator_observations(&self) -> &bray_bound_tree::GeneratorObservations {
        &self.generator_observations
    }

    /// Returns whole-value and represented-part finalizers selected for this body.
    pub fn finalizers(
        &self,
    ) -> impl Iterator<Item = (TypeId, bray_bound_tree::LifecycleCallable)> + '_ {
        self.cleanup
            .lifecycles
            .iter()
            .filter_map(|((ty, slot), lifecycle)| {
                (*slot == bray_symbols::TypeAssociatedLifecycleSlot::Finalizer)
                    .then_some((*ty, lifecycle.call))
            })
    }

    /// Returns selected implicit lifecycle callees required by this body's summary component.
    pub fn lifecycle_callees(&self) -> impl Iterator<Item = bray_symbols::CallableSymbolId> + '_ {
        self.cleanup.callees()
    }

    /// Returns whether selected consuming-receiver cleanup can execute without suspension.
    /// Completion certification comes from its owning execution-guarantee analysis.
    pub fn destruction_is_synchronous<C, F>(
        &self,
        request: CheckerUnitView<'_, C>,
        ty: TypeId,
        completed: &mut F,
    ) -> Result<bray_diagnostics::DiagnosticResult<bool>, crate::CheckerQueryError<C::UpstreamError>>
    where
        C: CheckerRequestContext
            + crate::CheckerSemanticQueryProvider<bray_symbols::CallableContractsQuery>
            + ?Sized,
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, crate::CheckerQueryError<C::UpstreamError>>,
    {
        let mut resolver = self.cleanup_shapes(request);
        let synchronous = resolver.destruction_is_synchronous(ty, completed)?;

        Ok(bray_diagnostics::DiagnosticResult::new(
            synchronous,
            resolver.diagnostics,
        ))
    }

    /// Checks instantiated synchronous cleanup obligations using selected lifecycle contracts.
    /// Unresolved generic parts remain obligations of the enclosing callable's instances.
    pub fn check_synchronous_cleanup<C, F>(
        &self,
        request: CheckerUnitView<'_, C>,
        requirements: &[bray_symbols::DependencyRequirement],
        completed: &mut F,
    ) -> Result<bray_diagnostics::DiagnosticResult<()>, crate::CheckerQueryError<C::UpstreamError>>
    where
        C: CheckerRequestContext
            + crate::CheckerSemanticQueryProvider<bray_symbols::CallableContractsQuery>
            + ?Sized,
        F: FnMut(
            bray_bound_tree::LifecycleCallable,
        ) -> Result<bool, crate::CheckerQueryError<C::UpstreamError>>,
    {
        crate::dependency::check_synchronous_cleanup(
            request,
            self.cleanup_shapes(request),
            requirements,
            completed,
        )
    }

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

    let mut graph = match build_storage_control_flow_graph(
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

    let cleanup = match super::guarantee::CleanupPlan::prepare(
        &mut cleanup_shapes,
        request,
        storage,
        expressions,
        &graph,
    ) {
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

    graph.select_cleanup_suspensions(
        &cleanup.asynchronous_scopes(storage),
        &cleanup.asynchronous_replacements(storage),
        &cleanup.expression_cleanup_calls,
    );

    let Some(values) =
        super::liveness::ValueRetention::prepare(&graph, request.unit(), storage, &request)
    else {
        return CheckerOutcome::Cancelled;
    };

    let mut prepared = PreparedBodyAnalysis {
        generator_observations: super::generator::collect_generator_observations(
            request,
            expressions,
        ),
        values,
        graph,
        cleanup,
        kind: request.unit().key().kind(),
        sequence_lengths: Vec::new(),
        has_bounds_checks: false,
        observed_types: BTreeMap::new(),
        borrowed_inputs: BTreeSet::new(),
        input_invariants: Vec::new(),
        scope_places: BTreeMap::new(),
        yield_targets: BTreeMap::new(),
        literals: crate::execution_guarantees::condition_literals(
            expressions,
            request.semantic_values(),
        ),
        pattern_conditions: crate::execution_guarantees::pattern_conditions(
            request.unit(),
            patterns,
            request.semantic_values(),
        ),
        cleanup_shapes: cleanup_shapes.completed,
    };

    prepared.prepare_observations(request, expressions);

    match prepared.prepare_input_invariants(request) {
        Ok(()) => {}
        Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(crate::CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    }

    prepared.prepare_scope_places(request, storage);

    CheckerOutcome::complete(prepared, cleanup_shapes.diagnostics)
}

impl PreparedBodyAnalysis {
    fn prepare_observations<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        semantics: &CheckedExpressionSemantics,
    ) {
        let targets = crate::dependency::transfer_targets(request.unit());
        let mut yields = Vec::new();

        for (expression, node) in request.unit().tree().expressions() {
            self.has_bounds_checks |= matches!(
                semantics.selections().expression(expression),
                Some(bray_bound_tree::SemanticSelection::Operation(
                    bray_bound_tree::SelectedOperation::Index {
                        target: bray_bound_tree::IndexTarget::ArrayElement
                            | bray_bound_tree::IndexTarget::SliceElement
                            | bray_bound_tree::IndexTarget::ArraySlice
                            | bray_bound_tree::IndexTarget::Slice,
                        ..
                    }
                ))
            );

            if let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                semantics.selections().expression(expression)
                && call.implementation_hook()
                    == Some(bray_compiler_known::ImplementationHook::SequenceLength)
            {
                self.sequence_lengths.push(expression);
            }

            if let BoundExpression::ControlTransfer(transfer) = node
                && matches!(
                    transfer.kind(),
                    BoundControlTransferKind::Yield | BoundControlTransferKind::Break
                )
                && let Some(target) = transfer.target()
            {
                yields.push((expression, target));
            }

            if let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
                semantics.selections().expression(expression)
                && let Some(bray_bound_tree::SelectedPropagationBoundary::YieldRegion(target)) =
                    selection.boundary()
            {
                yields.push((expression, target));
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

    fn prepare_input_invariants<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
    ) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
        let mut types = BTreeMap::new();

        for (place, ty) in &self.observed_types {
            if !place.is_callable_input() {
                continue;
            }

            let Some(ty) = ty else {
                continue;
            };

            let fields = if let Some(fields) = types.get(ty) {
                *fields
            } else {
                let fields = crate::representation::raw_buffer_fields(request, *ty)?;

                types.insert(*ty, fields);

                fields
            };

            let Some(fields) = fields else {
                continue;
            };

            let (capacity, initialized) = (fields.capacity, fields.initialized);

            let field = |field| {
                crate::ExecutionCondition::Input(
                    place
                        .clone()
                        .component(bray_symbols::ConstantProjectionKind::ProductField(field)),
                )
            };

            self.input_invariants
                .push(crate::ExecutionCondition::operation(
                    bray_bound_tree::BoundOperator::LessEqual,
                    vec![field(initialized), field(capacity)],
                ));
        }

        Ok(())
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

            // Receiver expressions already expose their reached type. The stored
            // formal type retains whether the caller owns the referent.
            if let Some(identity) = identity
                && storage.storage_type(identity).is_some_and(|ty| {
                    matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    )
                })
                && let Some(place) = crate::ExecutionPlace::binding(storage, *target)
                && place.is_callable_input()
            {
                self.borrowed_inputs.insert(place);
            }

            // A borrowed input ends its local binding at return, while its caller retains
            // the reached storage and the authority carried by a returned projection.
            if identity.is_some_and(|identity| {
                matches!(
                    storage.identity(identity),
                    Some(
                        bray_bound_tree::StorageIdentity::Parameter(_)
                            | bray_bound_tree::StorageIdentity::Receiver(_)
                            | bray_bound_tree::StorageIdentity::ContractParameter(_)
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
                && let Some(place) = crate::ExecutionPlace::binding(storage, *target)
            {
                self.scope_places.entry(scope).or_default().push((
                    place,
                    identity.expect("scoped binding has a storage identity"),
                ));
            }
        }
    }
}
