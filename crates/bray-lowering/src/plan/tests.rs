use bray_bound_tree::{
    AnyBoundNodeId, AsyncCleanupPhases, AsyncScopeExitPlan, AsyncStorageExitDecision,
    AsyncStorageExitDisposition, AsyncStorageExitRecoveryCause, BoundAwaitExpression, BoundBlock,
    BoundBlockId, BoundBlockItem, BoundDependencyContract, BoundDependencySubject, BoundExpression,
    BoundExpressionId, BoundNodeOrigin, BoundStructuredExpression, BoundStructuredExpressionKind,
    BoundTreeBuilder, BoundUnit, CheckedAsync, CheckedDependencyContracts, CheckedExpressionTypes,
    CheckedSemanticSelections, LiveAcrossSuspension, Liveness, StorageAccess, StorageAccessId,
    StorageAccessRoot, StorageExitDecision, StorageFlow, StorageIdentity, StorageIdentityId,
    StoragePlan, StoragePlanBuilder,
};
use bray_symbols::testing::available_compiler_known_symbols;
use bray_symbols::{SemanticValueStore, TypeData};
use bray_testing::test_runtime_default_unit;

use super::{
    LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind, VerifiedLoweringPlans,
};

struct ScopeExitFixture {
    unit: BoundUnit,
    scope: BoundBlockId,
    exit: AnyBoundNodeId,
    storage: StoragePlan,
    liveness: Liveness,
    flow: StorageFlow,
    dependencies: CheckedDependencyContracts,
    selections: CheckedSemanticSelections,
    first: StorageIdentityId,
    second: StorageIdentityId,
    first_access: StorageAccessId,
    second_access: StorageAccessId,
}

impl ScopeExitFixture {
    fn new() -> Self {
        let mut expressions = None;
        let mut scope = None;

        let unit = test_runtime_default_unit(93, |tree, origin| {
            let first = push_unit_expression(tree, origin);
            let second = push_unit_expression(tree, origin);

            let block = tree
                .push_block(BoundBlock::new(
                    origin,
                    [
                        BoundBlockItem::Expression(first),
                        BoundBlockItem::Expression(second),
                    ],
                    false,
                ))
                .unwrap_or_else(|error| panic!("test block must fit: {error:?}"));

            expressions = Some([first, second]);
            scope = Some(block);

            tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Unit,
                [],
                [block],
                [],
                None,
                false,
            )))
            .unwrap_or_else(|error| panic!("test root expression must fit: {error:?}"))
        });

        let [first_expression, second_expression] =
            expressions.unwrap_or_else(|| panic!("test expressions must be captured"));

        let scope = scope.unwrap_or_else(|| panic!("test scope must be captured"));
        let exit = AnyBoundNodeId::Expression(second_expression);

        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let first = storage
            .push_identity(StorageIdentity::Temporary(first_expression))
            .unwrap_or_else(|error| panic!("first test identity must fit: {error:?}"));

        let second = storage
            .push_identity(StorageIdentity::Temporary(second_expression))
            .unwrap_or_else(|error| panic!("second test identity must fit: {error:?}"));

        let source = unit
            .view()
            .expression(first_expression)
            .map(BoundExpression::origin)
            .map(BoundNodeOrigin::source_anchor)
            .unwrap_or_else(|| panic!("test source anchor must exist"));

        let first_access = push_access(&mut storage, first, ty, source);
        let second_access = push_access(&mut storage, second, ty, source);
        let storage = storage.finish();

        let flow = StorageFlow::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [],
            [StorageExitDecision::new(
                scope,
                exit,
                [first, second],
                [],
                [],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must build: {error:?}"));

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("test liveness must build: {error:?}"));

        let expressions = unit
            .tree()
            .expressions()
            .map(|(expression, _)| (expression, BoundDependencyContract::new([])));

        let accesses = storage
            .access_entries()
            .map(|(access, _)| (access, BoundDependencyContract::new([])));

        let dependencies = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            expressions,
            [],
            accesses,
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test dependencies must build: {error:?}"));

        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("test selections must build: {error:?}"));

        Self {
            unit,
            scope,
            exit,
            storage,
            liveness,
            flow,
            dependencies,
            selections,
            first,
            second,
            first_access,
            second_access,
        }
    }

    fn plan(
        &self,
        storage: impl IntoIterator<Item = AsyncStorageExitDecision>,
        cancellation: impl IntoIterator<Item = StorageAccessId>,
        lifecycle: impl IntoIterator<Item = StorageAccessId>,
        recovered: bool,
    ) -> AsyncScopeExitPlan {
        AsyncScopeExitPlan::new(
            self.scope,
            self.exit,
            storage,
            cancellation,
            lifecycle,
            [],
            recovered,
        )
    }

    fn analysis(
        &self,
        exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
        recovered: bool,
    ) -> CheckedAsync {
        CheckedAsync::try_new(
            self.unit.unit(),
            self.unit.key().kind(),
            [],
            [],
            [],
            exits,
            recovered,
        )
        .unwrap_or_else(|error| panic!("test async analysis must build: {error:?}"))
    }

    fn verify<'analysis>(
        &'analysis self,
        analysis: &'analysis CheckedAsync,
    ) -> Result<VerifiedLoweringPlans<'analysis>, LoweringPlanFailure> {
        self.verify_with_flow(&self.flow, analysis)
    }

    fn verify_with_flow<'analysis>(
        &'analysis self,
        flow: &'analysis StorageFlow,
        analysis: &'analysis CheckedAsync,
    ) -> Result<VerifiedLoweringPlans<'analysis>, LoweringPlanFailure> {
        VerifiedLoweringPlans::try_new(
            &self.unit,
            &self.storage,
            &self.liveness,
            flow,
            &self.dependencies,
            &self.selections,
            available_compiler_known_symbols(),
            analysis,
        )
    }

    fn complete_decisions(&self) -> [AsyncStorageExitDecision; 2] {
        [
            AsyncStorageExitDecision::new(self.second, AsyncStorageExitDisposition::NoCleanup),
            AsyncStorageExitDecision::new(self.first, AsyncStorageExitDisposition::NoCleanup),
        ]
    }
}

#[test]
fn complete_scope_exit_plans_publish_direct_lookup() {
    let fixture = ScopeExitFixture::new();

    let analysis = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        false,
    );

    let plans = fixture
        .verify(&analysis)
        .unwrap_or_else(|error| panic!("complete plans must verify: {error:?}"));

    assert_eq!(
        plans.cleanup_plans(&[fixture.scope], 0, fixture.exit).len(),
        1
    );

    assert!(!plans.scope_has_cleanup(fixture.scope, fixture.exit));
}

#[test]
fn aggregate_recovery_flags_do_not_replace_component_verification() {
    let fixture = ScopeExitFixture::new();

    let analysis = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        true,
    );

    assert!(fixture.verify(&analysis).is_ok());
}

#[test]
fn suspension_verification_rejects_an_omitted_live_subject() {
    let mut expressions = None;

    let unit = test_runtime_default_unit(94, |tree, origin| {
        let operand = push_unit_expression(tree, origin);

        let suspension = tree
            .push_expression(BoundExpression::Await(BoundAwaitExpression::pending(
                origin, operand, false,
            )))
            .unwrap_or_else(|error| panic!("test suspension must fit: {error:?}"));

        expressions = Some((operand, suspension));

        suspension
    });

    let (operand, suspension) =
        expressions.unwrap_or_else(|| panic!("test suspension must be captured"));

    let values = SemanticValueStore::try_new()
        .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

    let ty = values
        .intern_type(TypeData::tuple([]))
        .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

    let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

    let identity = storage
        .push_identity(StorageIdentity::Temporary(operand))
        .unwrap_or_else(|error| panic!("test identity must fit: {error:?}"));

    let source = unit
        .view()
        .expression(operand)
        .map(BoundExpression::origin)
        .map(BoundNodeOrigin::source_anchor)
        .unwrap_or_else(|| panic!("test source anchor must exist"));

    let access = push_access(&mut storage, identity, ty, source);
    let storage = storage.finish();

    let contract = BoundDependencyContract::new([]);

    let dependencies = CheckedDependencyContracts::try_new(
        &unit,
        &storage,
        [(operand, contract.clone()), (suspension, contract.clone())],
        [],
        [(access, contract)],
        [],
        false,
    )
    .unwrap_or_else(|error| panic!("test dependencies must build: {error:?}"));

    let liveness = Liveness::try_new(
        unit.unit(),
        unit.key().kind(),
        [],
        [],
        [LiveAcrossSuspension::new(
            suspension,
            BoundDependencySubject::Storage(identity),
        )],
        [],
        false,
    )
    .unwrap_or_else(|error| panic!("test liveness must build: {error:?}"));

    let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
        .unwrap_or_else(|error| panic!("test flow must build: {error:?}"));

    let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

    let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
        .unwrap_or_else(|error| panic!("test selections must build: {error:?}"));

    let analysis = CheckedAsync::try_new(
        unit.unit(),
        unit.key().kind(),
        [],
        [bray_bound_tree::AsyncSuspensionPoint::new(
            suspension,
            bray_bound_tree::AsyncSuspensionKind::Await { operand },
            None,
            [],
            [],
            false,
        )],
        [],
        [],
        false,
    )
    .unwrap_or_else(|error| panic!("test async analysis must build: {error:?}"));

    let result = VerifiedLoweringPlans::try_new(
        &unit,
        &storage,
        &liveness,
        &flow,
        &dependencies,
        &selections,
        available_compiler_known_symbols(),
        &analysis,
    );

    assert_plan_failure(
        result,
        LoweringPlanKind::Suspension,
        LoweringPlanFailureCause::Contradictory,
        None,
    );
}

#[test]
fn exact_recovered_scope_exit_inputs_are_rejected() {
    let fixture = ScopeExitFixture::new();

    let recovered_plan = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], true)],
        true,
    );

    assert_plan_failure(
        fixture.verify(&recovered_plan),
        LoweringPlanKind::ScopeExit,
        LoweringPlanFailureCause::Recovered,
        None,
    );

    let recovered_flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first, fixture.second],
            [],
            [],
            [],
            true,
        )],
        true,
    )
    .unwrap_or_else(|error| panic!("recovered test flow must build: {error:?}"));

    let complete = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        true,
    );

    assert_plan_failure(
        fixture.verify_with_flow(&recovered_flow, &complete),
        LoweringPlanKind::ScopeExit,
        LoweringPlanFailureCause::Recovered,
        None,
    );
}

#[test]
fn scope_exit_verification_rejects_missing_and_duplicate_rows() {
    let fixture = ScopeExitFixture::new();
    let missing = fixture.analysis([], false);

    assert_plan_failure(
        fixture.verify(&missing),
        LoweringPlanKind::ScopeExit,
        LoweringPlanFailureCause::Missing,
        None,
    );

    let first = fixture.plan(fixture.complete_decisions(), [], [], false);
    let duplicate = fixture.analysis([first.clone(), first], false);

    assert_plan_failure(
        fixture.verify(&duplicate),
        LoweringPlanKind::ScopeExit,
        LoweringPlanFailureCause::Duplicate,
        None,
    );
}

#[test]
fn storage_dispositions_are_exhaustive_unique_and_ordered() {
    let fixture = ScopeExitFixture::new();

    let missing = fixture.analysis(
        [fixture.plan(
            [AsyncStorageExitDecision::new(
                fixture.second,
                AsyncStorageExitDisposition::NoCleanup,
            )],
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify(&missing),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Missing,
        Some(fixture.first),
    );

    let duplicate = fixture.analysis(
        [fixture.plan(
            [
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
            ],
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify(&duplicate),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Duplicate,
        Some(fixture.second),
    );

    let reversed = fixture.analysis(
        [fixture.plan(
            fixture.complete_decisions().into_iter().rev(),
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify(&reversed),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::OutOfOrder,
        Some(fixture.first),
    );
}

#[test]
fn recovered_and_contradictory_storage_dispositions_are_rejected() {
    let fixture = ScopeExitFixture::new();

    let recovered = fixture.analysis(
        [fixture.plan(
            [
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                    ),
                ),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
            ],
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify(&recovered),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Recovered,
        Some(fixture.second),
    );

    let contradictory = fixture.analysis(
        [fixture.plan(
            [
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::Cleanup {
                        access: fixture.second_access,
                        phases: AsyncCleanupPhases::CancellationThenLifecycle,
                    },
                ),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::Cleanup {
                        access: fixture.first_access,
                        phases: AsyncCleanupPhases::Lifecycle,
                    },
                ),
            ],
            [fixture.second_access],
            [fixture.first_access, fixture.second_access],
            false,
        )],
        false,
    );

    let Err(error) = fixture.verify(&contradictory) else {
        panic!("contradictory lifecycle phase must fail verification");
    };

    assert_eq!(error.kind(), LoweringPlanKind::LifecyclePhase);
    assert_eq!(error.cause(), LoweringPlanFailureCause::Contradictory);
    assert_eq!(error.access(), Some(fixture.first_access));

    let falsely_moved = fixture.analysis(
        [fixture.plan(
            [
                AsyncStorageExitDecision::new(fixture.second, AsyncStorageExitDisposition::Moved),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
            ],
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify(&falsely_moved),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Contradictory,
        Some(fixture.second),
    );
}

#[test]
fn scope_exit_verification_rejects_storage_state_outside_the_exit_set() {
    let fixture = ScopeExitFixture::new();

    let flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first],
            [],
            [fixture.second],
            [],
            false,
        )],
        false,
    )
    .unwrap_or_else(|error| panic!("test storage flow must build: {error:?}"));

    let analysis = fixture.analysis(
        [fixture.plan(
            [AsyncStorageExitDecision::new(
                fixture.first,
                AsyncStorageExitDisposition::NoCleanup,
            )],
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify_with_flow(&flow, &analysis),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Unexpected,
        Some(fixture.second),
    );
}

fn assert_plan_failure(
    result: Result<VerifiedLoweringPlans<'_>, LoweringPlanFailure>,
    kind: LoweringPlanKind,
    cause: LoweringPlanFailureCause,
    storage: Option<StorageIdentityId>,
) {
    let Err(error) = result else {
        panic!("invalid plan must fail verification");
    };

    assert_eq!(error.kind(), kind);
    assert_eq!(error.cause(), cause);
    assert_eq!(error.storage(), storage);
}

fn push_unit_expression(tree: &mut BoundTreeBuilder, origin: BoundNodeOrigin) -> BoundExpressionId {
    tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
        origin,
        BoundStructuredExpressionKind::Unit,
        [],
        [],
        [],
        None,
        false,
    )))
    .unwrap_or_else(|error| panic!("test unit expression must fit: {error:?}"))
}

fn push_access(
    storage: &mut StoragePlanBuilder,
    identity: StorageIdentityId,
    ty: bray_symbols::TypeId,
    source: bray_bound_tree::BoundSourceAnchor,
) -> StorageAccessId {
    storage
        .push_access(StorageAccess::new(
            StorageAccessRoot::Storage(identity),
            [],
            ty,
            source,
            false,
        ))
        .unwrap_or_else(|error| panic!("test storage access must fit: {error:?}"))
}
