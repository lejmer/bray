use bray_bound_tree::{
    AnyBoundNodeId, AsyncCleanupPhases, AsyncScopeExitPlan, AsyncStorageCleanupRequirement,
    AsyncStorageExitDecision, AsyncStorageExitDisposition, AsyncStorageExitRecoveryCause,
    AsyncStorageRequirement, BoundAwaitExpression, BoundBlock, BoundBlockId, BoundBlockItem,
    BoundDependencyContract, BoundDependencyRequirement, BoundDependencyRequirementKind,
    BoundDependencySubject, BoundExpression, BoundExpressionId, BoundNodeOrigin,
    BoundStructuredExpression, BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit,
    CheckedAsync, CheckedDependencyContracts, CheckedExpressionTypes, CheckedSemanticSelections,
    LiveAcrossSuspension, Liveness, StorageAccess, StorageAccessId, StorageAccessRoot,
    StorageExitDecision, StorageExitPoint, StorageFlow, StorageIdentity, StorageIdentityId,
    StoragePlan, StoragePlanBuilder, StorageProjection,
};
use bray_symbols::testing::available_compiler_known_symbols;
use bray_symbols::{SemanticValueStore, SymbolOrdinal, TypeData};
use bray_testing::test_runtime_default_unit;

use super::{
    CleanupPlanLookupError, LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind,
    ScopeExitCleanupStatus, VerifiedLoweringPlans,
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
    second_projected_access: StorageAccessId,
    intermediate: StorageIdentityId,
    intermediate_access: StorageAccessId,
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

        let second_projected_access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(second),
                [StorageProjection::TupleElement(SymbolOrdinal::new(0))],
                ty,
                source,
                false,
            ))
            .unwrap_or_else(|error| panic!("projected test access must fit: {error:?}"));

        let intermediate_expression = unit.tree().expressions().last().unwrap().0;

        let intermediate = storage
            .push_identity(StorageIdentity::Temporary(intermediate_expression))
            .unwrap();

        let intermediate_access = push_access(&mut storage, intermediate, ty, source);
        let storage = storage.finish();

        let flow = StorageFlow::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [],
            [StorageExitPoint::new(scope, exit)],
            [StorageExitDecision::new(
                scope,
                exit,
                [first, second],
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
            second_projected_access,
            intermediate,
            intermediate_access,
        }
    }

    fn plan(
        &self,
        storage: impl IntoIterator<Item = AsyncStorageExitDecision>,
        cancellation: impl IntoIterator<Item = StorageAccessId>,
        lifecycle: impl IntoIterator<Item = StorageAccessId>,
        recovered: bool,
    ) -> AsyncScopeExitPlan {
        self.plan_with_moved(storage, cancellation, lifecycle, [], recovered)
    }

    fn plan_with_moved(
        &self,
        storage: impl IntoIterator<Item = AsyncStorageExitDecision>,
        cancellation: impl IntoIterator<Item = StorageAccessId>,
        lifecycle: impl IntoIterator<Item = StorageAccessId>,
        moved: impl IntoIterator<Item = StorageAccessId>,
        recovered: bool,
    ) -> AsyncScopeExitPlan {
        AsyncScopeExitPlan::new(
            self.scope,
            self.exit,
            storage,
            cancellation,
            lifecycle,
            moved,
            recovered,
        )
    }

    fn analysis(
        &self,
        exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
        recovered: bool,
    ) -> CheckedAsync {
        self.analysis_with_requirements(self.complete_requirements(), exits, recovered)
    }

    fn analysis_with_requirements(
        &self,
        requirements: impl IntoIterator<Item = AsyncStorageRequirement>,
        exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
        recovered: bool,
    ) -> CheckedAsync {
        CheckedAsync::try_new(
            self.unit.unit(),
            self.unit.key().kind(),
            [],
            [],
            [],
            requirements,
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

    fn complete_requirements(&self) -> [AsyncStorageRequirement; 2] {
        [
            AsyncStorageRequirement::new(
                self.first,
                Some(self.scope),
                false,
                AsyncStorageCleanupRequirement::None,
            ),
            AsyncStorageRequirement::new(
                self.second,
                Some(self.scope),
                false,
                AsyncStorageCleanupRequirement::None,
            ),
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
        plans
            .cleanup_plans(&[fixture.scope], 0, fixture.exit)
            .map(|plans| plans.len()),
        Ok(1)
    );

    assert_eq!(
        plans.scope_cleanup_status(fixture.scope, fixture.exit),
        Ok(ScopeExitCleanupStatus::NoCleanup)
    );

    assert_eq!(
        plans.cleanup_plans(&[fixture.scope], 2, fixture.exit),
        Err(CleanupPlanLookupError::InvalidScopeDepth {
            scope_depth: 2,
            active_scope_count: 1,
            exit: fixture.exit,
        })
    );

    assert_eq!(
        plans.cleanup_plans(&[fixture.scope], 0, fixture.scope.into()),
        Err(CleanupPlanLookupError::MissingScopeExit {
            scope: fixture.scope,
            exit: fixture.scope.into(),
        })
    );

    assert_eq!(
        plans.scope_cleanup_status(fixture.scope, fixture.scope.into()),
        Ok(ScopeExitCleanupStatus::Unreachable)
    );
}

#[test]
fn storage_recovery_causes_survive_summary_recovery() {
    let fixture = ScopeExitFixture::new();

    for cause in [
        AsyncStorageExitRecoveryCause::UnavailableRootAccess,
        AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
        AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
        AsyncStorageExitRecoveryCause::UnavailableCleanupOrder,
    ] {
        let mut requirements = fixture.complete_requirements();

        requirements[1] = AsyncStorageRequirement::new(
            fixture.second,
            Some(fixture.scope),
            false,
            AsyncStorageCleanupRequirement::Recovered(cause),
        );

        let analysis = fixture.analysis_with_requirements(
            requirements,
            [fixture.plan(fixture.complete_decisions(), [], [], true)],
            true,
        );

        assert_plan_failure(
            fixture.verify(&analysis),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::StorageRecovery(cause),
            Some(fixture.second),
        );

        let analysis = fixture.analysis(
            [fixture.plan(
                [
                    AsyncStorageExitDecision::new(
                        fixture.second,
                        AsyncStorageExitDisposition::Recovered(cause),
                    ),
                    AsyncStorageExitDecision::new(
                        fixture.first,
                        AsyncStorageExitDisposition::NoCleanup,
                    ),
                ],
                [],
                [],
                true,
            )],
            true,
        );

        assert_plan_failure(
            fixture.verify(&analysis),
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::StorageRecovery(cause),
            Some(fixture.second),
        );
    }
}

#[test]
fn lifecycle_order_respects_guarded_dependencies_and_rejects_cycles() {
    let mut fixture = ScopeExitFixture::new();

    for cyclic in [false, true] {
        fixture.dependencies = CheckedDependencyContracts::try_new(
            &fixture.unit,
            &fixture.storage,
            fixture
                .unit
                .tree()
                .expressions()
                .map(|(id, _)| (id, BoundDependencyContract::new([]))),
            [],
            fixture.storage.access_entries().map(|(access, _)| {
                let target = if access == fixture.first_access {
                    Some(fixture.intermediate)
                } else if access == fixture.intermediate_access {
                    Some(fixture.second)
                } else if cyclic && access == fixture.second_access {
                    Some(fixture.first)
                } else {
                    None
                };

                let requirements = target.map(|identity| {
                    BoundDependencyRequirement::guarded(
                        bray_bound_tree::BoundDependencyGuard::NullablePresent(access),
                        [BoundDependencyRequirement::direct(
                            BoundDependencySubject::Storage(identity),
                            BoundDependencyRequirementKind::StorageAlive,
                        )],
                    )
                });

                (access, BoundDependencyContract::new(requirements))
            }),
            [],
            false,
        )
        .unwrap();

        let requirements = [fixture.first, fixture.second].map(|identity| {
            AsyncStorageRequirement::new(
                identity,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(
                    AsyncCleanupPhases::CancellationThenLifecycle,
                ),
            )
        });

        let decisions = [
            (fixture.second, fixture.second_access),
            (fixture.first, fixture.first_access),
        ]
        .map(|(identity, access)| {
            AsyncStorageExitDecision::new(
                identity,
                AsyncStorageExitDisposition::Cleanup {
                    access,
                    phases: AsyncCleanupPhases::CancellationThenLifecycle,
                },
            )
        });

        let reversed = fixture.analysis_with_requirements(
            requirements,
            [fixture.plan(
                decisions,
                [fixture.second_access, fixture.first_access],
                [fixture.second_access, fixture.first_access],
                false,
            )],
            false,
        );

        assert!(fixture.verify(&reversed).is_err());

        let ordered = fixture.analysis_with_requirements(
            requirements,
            [fixture.plan(
                decisions,
                [fixture.second_access, fixture.first_access],
                [fixture.first_access, fixture.second_access],
                false,
            )],
            false,
        );

        if cyclic {
            assert_plan_failure(
                fixture.verify(&ordered),
                LoweringPlanKind::LifecyclePhase,
                LoweringPlanFailureCause::OutOfOrder,
                None,
            );
        } else {
            assert!(fixture.verify(&ordered).is_ok());
        }
    }
}

#[test]
fn aggregate_recovery_flags_are_rejected() {
    let fixture = ScopeExitFixture::new();

    let analysis = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        true,
    );

    assert_plan_failure(
        fixture.verify(&analysis),
        LoweringPlanKind::Analysis,
        LoweringPlanFailureCause::Recovered,
        None,
    );
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

    let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
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
fn suspension_verification_rejects_a_substituted_dependency_contract() {
    let mut expressions = None;

    let unit = test_runtime_default_unit(95, |tree, origin| {
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
    let empty = BoundDependencyContract::new([]);

    let retained = BoundDependencyContract::new([BoundDependencyRequirement::direct(
        BoundDependencySubject::Storage(identity),
        BoundDependencyRequirementKind::StorageAlive,
    )]);

    let dependencies = CheckedDependencyContracts::try_new(
        &unit,
        &storage,
        [(operand, empty.clone()), (suspension, empty)],
        [
            (operand, BoundDependencyContract::new([])),
            (suspension, retained),
        ],
        [(access, BoundDependencyContract::new([]))],
        [],
        false,
    )
    .unwrap_or_else(|error| panic!("test dependencies must build: {error:?}"));

    let substituted = dependencies
        .deferred_expression(suspension)
        .unwrap_or_else(|| panic!("substituted contract must exist"));

    let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
        .unwrap_or_else(|error| panic!("test liveness must build: {error:?}"));

    let flow = StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
        .unwrap_or_else(|error| panic!("test flow must build: {error:?}"));

    let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

    let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
        .unwrap_or_else(|error| panic!("test selections must build: {error:?}"));

    let analysis = CheckedAsync::try_new(
        unit.unit(),
        unit.key().kind(),
        [BoundDependencySubject::Storage(identity)],
        [bray_bound_tree::AsyncSuspensionPoint::new(
            suspension,
            bray_bound_tree::AsyncSuspensionKind::Await { operand },
            Some(substituted),
            [],
            [BoundDependencySubject::Storage(identity)],
            false,
        )],
        [],
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
        false,
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
        [StorageExitPoint::new(fixture.scope, fixture.exit)],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first, fixture.second],
            [fixture.first, fixture.second],
            [],
            [],
            [],
            true,
        )],
        false,
    )
    .unwrap_or_else(|error| panic!("recovered test flow must build: {error:?}"));

    let complete = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        false,
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
        LoweringPlanFailureCause::StorageRecovery(
            AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
        ),
        Some(fixture.second),
    );

    let contradictory = fixture.analysis_with_requirements(
        [
            AsyncStorageRequirement::new(
                fixture.first,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            ),
            AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(
                    AsyncCleanupPhases::CancellationThenLifecycle,
                ),
            ),
        ],
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
        [StorageExitPoint::new(fixture.scope, fixture.exit)],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first],
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

#[test]
fn trivial_partially_initialized_storage_has_no_cleanup() {
    let fixture = ScopeExitFixture::new();

    let flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [StorageExitPoint::new(fixture.scope, fixture.exit)],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first, fixture.second],
            [fixture.first],
            [],
            [],
            [],
            false,
        )],
        false,
    )
    .unwrap_or_else(|error| panic!("partial test flow must build: {error:?}"));

    let analysis = fixture.analysis(
        [fixture.plan(
            [
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

    fixture
        .verify_with_flow(&flow, &analysis)
        .unwrap_or_else(|error| panic!("trivial partial storage must verify: {error:?}"));
}

#[test]
fn nontrivial_partially_initialized_storage_cannot_reach_lowering() {
    let fixture = ScopeExitFixture::new();

    let flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [StorageExitPoint::new(fixture.scope, fixture.exit)],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first, fixture.second],
            [fixture.first],
            [],
            [],
            [],
            false,
        )],
        false,
    )
    .unwrap_or_else(|error| panic!("partial test flow must build: {error:?}"));

    let analysis = fixture.analysis_with_requirements(
        [
            AsyncStorageRequirement::new(
                fixture.first,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::None,
            ),
            AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            ),
        ],
        [fixture.plan(
            [
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                    ),
                ),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
            ],
            [],
            [],
            true,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify_with_flow(&flow, &analysis),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::StorageRecovery(
            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
        ),
        Some(fixture.second),
    );
}

#[test]
fn fully_moved_storage_needs_no_cleanup_even_when_not_initialized() {
    let fixture = ScopeExitFixture::new();

    let flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [StorageExitPoint::new(fixture.scope, fixture.exit)],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first, fixture.second],
            [fixture.first],
            [fixture.second_access],
            [fixture.second],
            [],
            false,
        )],
        false,
    )
    .unwrap_or_else(|error| panic!("moved test flow must build: {error:?}"));

    let analysis = fixture.analysis_with_requirements(
        [
            AsyncStorageRequirement::new(
                fixture.first,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::None,
            ),
            AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            ),
        ],
        [fixture.plan_with_moved(
            [
                AsyncStorageExitDecision::new(fixture.second, AsyncStorageExitDisposition::Moved),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
            ],
            [],
            [],
            [fixture.second_access],
            false,
        )],
        false,
    );

    fixture
        .verify_with_flow(&flow, &analysis)
        .unwrap_or_else(|error| panic!("fully moved storage must verify: {error:?}"));
}

#[test]
fn nontrivial_partially_moved_storage_cannot_reach_lowering() {
    let fixture = ScopeExitFixture::new();

    let flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [StorageExitPoint::new(fixture.scope, fixture.exit)],
        [StorageExitDecision::new(
            fixture.scope,
            fixture.exit,
            [fixture.first, fixture.second],
            [fixture.first, fixture.second],
            [fixture.second_projected_access],
            [],
            [],
            false,
        )],
        false,
    )
    .unwrap_or_else(|error| panic!("partial-move test flow must build: {error:?}"));

    let analysis = fixture.analysis_with_requirements(
        [
            AsyncStorageRequirement::new(
                fixture.first,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::None,
            ),
            AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            ),
        ],
        [fixture.plan_with_moved(
            [
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                    ),
                ),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
            ],
            [],
            [],
            [fixture.second_projected_access],
            true,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify_with_flow(&flow, &analysis),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::StorageRecovery(
            AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
        ),
        Some(fixture.second),
    );
}

#[test]
fn storage_requirements_are_checked_independently_from_exit_dispositions() {
    let fixture = ScopeExitFixture::new();

    let fabricated = fixture.analysis_with_requirements(
        [
            AsyncStorageRequirement::new(
                fixture.first,
                None,
                true,
                AsyncStorageCleanupRequirement::None,
            ),
            AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            ),
        ],
        [fixture.plan(
            [
                AsyncStorageExitDecision::new(
                    fixture.second,
                    AsyncStorageExitDisposition::NoCleanup,
                ),
                AsyncStorageExitDecision::new(
                    fixture.first,
                    AsyncStorageExitDisposition::Transferred,
                ),
            ],
            [],
            [],
            false,
        )],
        false,
    );

    assert_plan_failure(
        fixture.verify(&fabricated),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Contradictory,
        Some(fixture.first),
    );

    let suppressed_cleanup = fixture.analysis_with_requirements(
        [
            AsyncStorageRequirement::new(
                fixture.first,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::None,
            ),
            AsyncStorageRequirement::new(
                fixture.second,
                Some(fixture.scope),
                false,
                AsyncStorageCleanupRequirement::Cleanup(AsyncCleanupPhases::Lifecycle),
            ),
        ],
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        false,
    );

    assert_plan_failure(
        fixture.verify(&suppressed_cleanup),
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::Contradictory,
        Some(fixture.second),
    );
}

#[test]
fn scope_exit_verification_rejects_rows_outside_reachable_exit_set() {
    let fixture = ScopeExitFixture::new();

    let flow = StorageFlow::try_new(
        fixture.unit.unit(),
        fixture.unit.key().kind(),
        [],
        [],
        [],
        [fixture.flow.exits()[0].clone()],
        false,
    )
    .unwrap_or_else(|error| panic!("dead-exit test flow must build: {error:?}"));

    let analysis = fixture.analysis(
        [fixture.plan(fixture.complete_decisions(), [], [], false)],
        false,
    );

    assert_plan_failure(
        fixture.verify_with_flow(&flow, &analysis),
        LoweringPlanKind::ScopeExit,
        LoweringPlanFailureCause::Unexpected,
        None,
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
