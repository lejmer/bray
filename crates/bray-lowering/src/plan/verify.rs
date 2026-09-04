use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, AsyncScopeExitPlan, AsyncSuspensionPoint, AsyncTaskOperationKind, BoundBlockId,
    BoundDependencySubject, BoundExpressionId, BoundUnit, BoundUnitId, BoundUnitKind, CheckedAsync,
    CheckedDependencyContracts, CheckedSemanticSelections, Liveness, StorageFlow,
    StorageIdentityId, StoragePlan,
};
use bray_symbols::AvailableCompilerKnownSymbols;

use super::expression::{verify_suspensions, verify_task_operations};
use super::scope::verify_scope_exits;
use super::{LoweringPlanFailure, LoweringPlanFailureCause};

/// Complete checked async, cleanup, and task plans safe for MIR lowering.
#[derive(Clone)]
pub struct VerifiedLoweringPlans<'unit> {
    storage: &'unit StoragePlan,
    liveness: &'unit Liveness,
    flow: &'unit StorageFlow,
    dependencies: &'unit CheckedDependencyContracts,
    selections: &'unit CheckedSemanticSelections,
    symbols: &'unit AvailableCompilerKnownSymbols,
    analysis: &'unit CheckedAsync,
    suspensions: BTreeMap<BoundExpressionId, usize>,
    task_operations: BTreeMap<BoundExpressionId, AsyncTaskOperationKind>,
    scope_exits: BTreeMap<(BoundBlockId, AnyBoundNodeId), usize>,
    lifecycle_storage: BTreeSet<StorageIdentityId>,
}

impl<'unit> VerifiedLoweringPlans<'unit> {
    /// Verifies partial checked analyses and publishes one lowering-ready plan set.
    #[expect(
        clippy::too_many_arguments,
        reason = "verification compares independently published semantic inputs"
    )]
    pub fn try_new(
        unit: &BoundUnit,
        storage: &'unit StoragePlan,
        liveness: &'unit Liveness,
        flow: &'unit StorageFlow,
        dependencies: &'unit CheckedDependencyContracts,
        selections: &'unit CheckedSemanticSelections,
        symbols: &'unit AvailableCompilerKnownSymbols,
        analysis: &'unit CheckedAsync,
    ) -> Result<Self, LoweringPlanFailure> {
        if storage.unit() != unit.unit()
            || storage.kind() != unit.key().kind()
            || liveness.unit() != unit.unit()
            || liveness.kind() != unit.key().kind()
            || flow.unit() != unit.unit()
            || flow.kind() != unit.key().kind()
            || dependencies.unit() != unit.unit()
            || dependencies.kind() != unit.key().kind()
            || selections.unit() != unit.unit()
            || selections.kind() != unit.key().kind()
            || analysis.unit() != unit.unit()
            || analysis.kind() != unit.key().kind()
        {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Unexpected,
            ));
        }

        if !dependencies.is_complete_for(unit, storage) {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Missing,
            ));
        }

        if !analysis
            .frame_dependencies()
            .iter()
            .all(|subject| dependency_subject_exists(unit, storage, *subject))
        {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Unexpected,
            ));
        }

        let suspensions = verify_suspensions(
            unit,
            storage,
            liveness,
            dependencies,
            selections,
            symbols,
            analysis,
        )?;

        let task_operations = verify_task_operations(unit, selections, symbols, analysis)?;

        let retained = analysis
            .suspensions()
            .iter()
            .flat_map(|suspension| suspension.retained_subjects().iter().copied())
            .collect::<BTreeSet<_>>();

        if !analysis.frame_dependencies().iter().copied().eq(retained) {
            return Err(LoweringPlanFailure::analysis(
                LoweringPlanFailureCause::Contradictory,
            ));
        }

        let (scope_exits, lifecycle_storage) = verify_scope_exits(unit, storage, flow, analysis)?;

        Ok(Self {
            storage,
            liveness,
            flow,
            dependencies,
            selections,
            symbols,
            analysis,
            suspensions,
            task_operations,
            scope_exits,
            lifecycle_storage,
        })
    }

    /// Returns the bound unit whose plan set was verified.
    pub const fn unit(&self) -> BoundUnitId {
        self.analysis.unit()
    }

    /// Returns the semantic unit category whose plan set was verified.
    pub const fn kind(&self) -> BoundUnitKind {
        self.analysis.kind()
    }

    pub(crate) const fn storage_plan(&self) -> &'unit StoragePlan {
        self.storage
    }

    pub(crate) const fn storage_flow(&self) -> &'unit StorageFlow {
        self.flow
    }

    pub(crate) const fn liveness(&self) -> &'unit Liveness {
        self.liveness
    }

    pub(crate) const fn dependency_contracts(&self) -> &'unit CheckedDependencyContracts {
        self.dependencies
    }

    pub(crate) const fn semantic_selections(&self) -> &'unit CheckedSemanticSelections {
        self.selections
    }

    pub(crate) const fn available_compiler_known_symbols(
        &self,
    ) -> &'unit AvailableCompilerKnownSymbols {
        self.symbols
    }

    /// Returns frame dependencies after complete-plan verification.
    pub fn frame_dependencies(&self) -> &[BoundDependencySubject] {
        self.analysis.frame_dependencies()
    }

    /// Returns the verified suspension plan for one suspending expression.
    pub fn suspension(&self, expression: BoundExpressionId) -> Option<&AsyncSuspensionPoint> {
        self.suspensions
            .get(&expression)
            .and_then(|index| self.analysis.suspensions().get(*index))
    }

    /// Returns the verified task operation for one selected call.
    pub fn task_operation(&self, expression: BoundExpressionId) -> Option<AsyncTaskOperationKind> {
        self.task_operations.get(&expression).copied()
    }

    /// Returns verified scope-exit plans for active scopes in cleanup order.
    pub fn cleanup_plans(
        &self,
        active_scopes: &[BoundBlockId],
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Vec<AsyncScopeExitPlan> {
        active_scopes
            .get(scope_depth..)
            .unwrap_or_default()
            .iter()
            .rev()
            .filter_map(|scope| {
                self.scope_exits
                    .get(&(*scope, exit))
                    .and_then(|index| self.analysis.scope_exits().get(*index))
                    // Lowering mutates its builder while retaining these shared immutable plans.
                    .cloned()
            })
            .collect()
    }

    /// Returns whether a verified exit plan performs cleanup for one scope.
    pub fn scope_has_cleanup(&self, scope: BoundBlockId, exit: AnyBoundNodeId) -> bool {
        self.scope_exits
            .get(&(scope, exit))
            .and_then(|index| self.analysis.scope_exits().get(*index))
            .is_some_and(|plan| {
                !plan.cancellation_broadcast().is_empty() || !plan.lifecycle_resolution().is_empty()
            })
    }

    /// Returns whether one storage identity participates in any lifecycle phase.
    pub fn requires_lifecycle_storage(&self, storage: StorageIdentityId) -> bool {
        self.lifecycle_storage.contains(&storage)
    }
}

pub(crate) fn dependency_subject_exists(
    unit: &BoundUnit,
    storage: &StoragePlan,
    subject: BoundDependencySubject,
) -> bool {
    match subject {
        BoundDependencySubject::Storage(identity) => storage.identity(identity).is_some(),
        BoundDependencySubject::StorageAccess(access) => storage.access(access).is_some(),
        BoundDependencySubject::BorrowCapability(borrow) => {
            storage.borrow_capability(borrow).is_some()
        }
        BoundDependencySubject::ScopedCapability(capability) => capability.unit() == unit.unit(),
        BoundDependencySubject::LifecycleObligation(obligation) => obligation.unit() == unit.unit(),
        BoundDependencySubject::ImplementationWitness(_)
        | BoundDependencySubject::ProductStatic(_)
        | BoundDependencySubject::ExactThreadStatic(_) => true,
    }
}
