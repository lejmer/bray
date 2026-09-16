use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, AsyncScopeExitPlan, AsyncStorageExitDisposition, AsyncSuspensionPoint,
    AsyncTaskOperationKind, BoundBlockId, BoundDependencySubject, BoundExpressionId, BoundUnit,
    BoundUnitId, BoundUnitKind, CheckedAsync, CheckedBodyBehavior, CheckedControlFlow,
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedLiteralValues, CheckedPatterns,
    CheckedRefinements, CheckedSemanticSelections, Liveness, StorageAccessId, StorageExitPoint,
    StorageFlow, StorageIdentityId, StoragePlan, StorageReplacementPlan, StorageReplacementState,
};
use bray_ir::{MirTargetContract, MirUnitBuilder, MirUnitKind};
use bray_symbols::{AvailableCompilerKnownSymbols, ConstantValueId, SemanticValueStore};

use crate::result::requires_mir;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum ScopeExitCleanupStatus {
    /// Checked control flow proves that this syntactic completion cannot run.
    Unreachable,
    /// The reachable exit performs no cleanup.
    NoCleanup,
    /// The reachable exit performs cancellation or lifecycle cleanup.
    Cleanup,
}

/// A borrowed view of the checked HIR and direct lookup state required by lowering.
///
/// The checker publishes the semantic analyses before this object is constructed. Lowering keeps
/// only the indexes needed by its hot lookup paths instead of certifying those analyses again.
#[derive(Clone)]
pub struct LoweringInput<'unit> {
    unit: &'unit BoundUnit,
    control_flow: &'unit CheckedControlFlow,
    expression_types: &'unit CheckedExpressionTypes,
    patterns: &'unit CheckedPatterns,
    literal_values: &'unit CheckedLiteralValues,
    refinements: &'unit CheckedRefinements,
    storage: &'unit StoragePlan,
    liveness: &'unit Liveness,
    storage_flow: &'unit StorageFlow,
    dependencies: &'unit CheckedDependencyContracts,
    selections: &'unit CheckedSemanticSelections,
    symbols: &'unit AvailableCompilerKnownSymbols,
    async_analysis: &'unit CheckedAsync,
    suspensions: BTreeMap<BoundExpressionId, usize>,
    task_operations: BTreeMap<BoundExpressionId, AsyncTaskOperationKind>,
    scope_exits: BTreeMap<(BoundBlockId, AnyBoundNodeId), usize>,
    lifecycle_storage: BTreeSet<StorageIdentityId>,
    replacements: BTreeMap<BoundExpressionId, usize>,
    completed: BTreeSet<(AnyBoundNodeId, StorageAccessId)>,
    body_behavior: &'unit CheckedBodyBehavior,
    semantic_values: &'unit SemanticValueStore,
    constant_reference_values: &'unit [(BoundExpressionId, ConstantValueId)],
    runtime_calls: &'unit [(
        bray_symbols::CallableDefinitionId,
        bray_runtime_interface::RuntimeAbiRole,
    )],
    native_static_templates: &'unit [bray_symbols::StaticInstanceTemplateId],
    static_owner: Option<(bray_symbols::StaticReferenceSelection, bray_symbols::TypeId)>,
    unit_kind: MirUnitKind,
    target: MirTargetContract,
}

impl<'unit> LoweringInput<'unit> {
    /// Publishes checked semantic inputs and the indexes used directly by lowering.
    #[expect(
        clippy::too_many_arguments,
        reason = "the lowering boundary consumes each independently published checker result"
    )]
    pub fn new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlow,
        expression_types: &'unit CheckedExpressionTypes,
        patterns: &'unit CheckedPatterns,
        literal_values: &'unit CheckedLiteralValues,
        refinements: &'unit CheckedRefinements,
        storage: &'unit StoragePlan,
        liveness: &'unit Liveness,
        storage_flow: &'unit StorageFlow,
        dependencies: &'unit CheckedDependencyContracts,
        selections: &'unit CheckedSemanticSelections,
        symbols: &'unit AvailableCompilerKnownSymbols,
        async_analysis: &'unit CheckedAsync,
        completed: BTreeSet<(AnyBoundNodeId, StorageAccessId)>,
        body_behavior: &'unit CheckedBodyBehavior,
        semantic_values: &'unit SemanticValueStore,
        constant_reference_values: &'unit [(BoundExpressionId, ConstantValueId)],
        unit_kind: MirUnitKind,
        target: MirTargetContract,
    ) -> Self {
        assert_input_owner(
            unit,
            "control-flow",
            control_flow.unit(),
            control_flow.kind(),
        );

        assert_input_owner(
            unit,
            "expression types",
            expression_types.unit(),
            expression_types.kind(),
        );

        assert_input_owner(unit, "patterns", patterns.unit(), patterns.kind());

        assert_input_owner(
            unit,
            "literal values",
            literal_values.unit(),
            literal_values.kind(),
        );

        assert_input_owner(unit, "refinements", refinements.unit(), refinements.kind());
        assert_input_owner(unit, "storage plan", storage.unit(), storage.kind());
        assert_input_owner(unit, "liveness", liveness.unit(), liveness.kind());

        assert_input_owner(
            unit,
            "storage flow",
            storage_flow.unit(),
            storage_flow.kind(),
        );

        assert_input_owner(
            unit,
            "dependency contracts",
            dependencies.unit(),
            dependencies.kind(),
        );

        assert_input_owner(
            unit,
            "semantic selections",
            selections.unit(),
            selections.kind(),
        );

        assert_input_owner(
            unit,
            "async analysis",
            async_analysis.unit(),
            async_analysis.kind(),
        );

        assert_input_owner(
            unit,
            "body behavior",
            body_behavior.unit(),
            body_behavior.kind(),
        );

        assert!(
            requires_mir(unit.key()),
            "lowering input received a compile-time-only unit {:?}",
            unit.key()
        );

        assert_eq!(
            literal_values.target_integer_width_bits(),
            target.machine().pointer_width_bits(),
            "checked literal values must use the selected lowering target width"
        );

        assert!(
            !matches!(unit_kind, MirUnitKind::ExecutableHost(_)),
            "source lowering input cannot use an executable-host unit kind"
        );

        let mut suspensions = BTreeMap::new();

        for (index, suspension) in async_analysis.suspensions().iter().enumerate() {
            assert!(
                suspensions.insert(suspension.expression(), index).is_none(),
                "checked async analysis contains duplicate suspension for {:?}",
                suspension.expression()
            );
        }

        let mut task_operations = BTreeMap::new();

        for operation in async_analysis.task_operations() {
            assert!(
                task_operations
                    .insert(operation.expression(), operation.kind())
                    .is_none(),
                "checked async analysis contains duplicate task operation for {:?}",
                operation.expression()
            );
        }

        let mut scope_exits = BTreeMap::new();
        let mut lifecycle_storage = BTreeSet::new();

        for (index, plan) in async_analysis.scope_exits().iter().enumerate() {
            assert!(
                scope_exits
                    .insert((plan.scope(), plan.exit()), index)
                    .is_none(),
                "checked async analysis contains duplicate scope-exit plan for {:?} at {:?}",
                plan.scope(),
                plan.exit()
            );

            for decision in plan.storage() {
                if let AsyncStorageExitDisposition::Cleanup { phases, .. } = decision.disposition()
                    && phases.includes_lifecycle()
                {
                    lifecycle_storage.insert(decision.identity());
                }
            }
        }

        let mut replacements = BTreeMap::new();

        for (index, replacement) in async_analysis.replacements().iter().enumerate() {
            assert!(
                replacements
                    .insert(replacement.expression(), index)
                    .is_none(),
                "checked async analysis contains duplicate replacement for {:?}",
                replacement.expression()
            );
        }

        Self {
            unit,
            control_flow,
            expression_types,
            patterns,
            literal_values,
            refinements,
            storage,
            liveness,
            storage_flow,
            dependencies,
            selections,
            symbols,
            async_analysis,
            suspensions,
            task_operations,
            scope_exits,
            lifecycle_storage,
            replacements,
            completed,
            body_behavior,
            semantic_values,
            constant_reference_values,
            runtime_calls: &[],
            native_static_templates: &[],
            static_owner: None,
            unit_kind,
            target,
        }
    }

    /// Returns the canonical checked source-shaped semantic unit.
    pub const fn unit(&self) -> &'unit BoundUnit {
        self.unit
    }

    /// Returns the durable control-flow analysis established for the unit.
    pub const fn control_flow(&self) -> &'unit CheckedControlFlow {
        self.control_flow
    }

    /// Returns final types for every bound expression occurrence.
    pub const fn expression_types(&self) -> &'unit CheckedExpressionTypes {
        self.expression_types
    }

    /// Returns checked pattern operations, binding types, and match coverage.
    pub const fn patterns(&self) -> &'unit CheckedPatterns {
        self.patterns
    }

    /// Returns exact semantic choices for the unit.
    pub const fn semantic_selections(&self) -> &'unit CheckedSemanticSelections {
        self.selections
    }

    /// Returns source literals adapted to their final checked types.
    pub const fn literal_values(&self) -> &'unit CheckedLiteralValues {
        self.literal_values
    }

    /// Returns exact storage identities, relationships, and evaluated accesses.
    pub const fn storage_plan(&self) -> &'unit StoragePlan {
        self.storage
    }

    /// Returns durable last-use and lexical lifetime decisions.
    pub const fn liveness(&self) -> &'unit Liveness {
        self.liveness
    }

    /// Returns flow-sensitive analysis available at checked operation occurrences.
    pub const fn refinements(&self) -> &'unit CheckedRefinements {
        self.refinements
    }

    /// Returns checked ownership, movement, and borrow decisions.
    pub const fn storage_flow(&self) -> &'unit StorageFlow {
        self.storage_flow
    }

    /// Returns normalized dependency contracts for unit-local semantic occurrences.
    pub const fn dependency_contracts(&self) -> &'unit CheckedDependencyContracts {
        self.dependencies
    }

    /// Returns normalized effects, capabilities, and lifecycle obligations.
    pub const fn body_behavior(&self) -> &'unit CheckedBodyBehavior {
        self.body_behavior
    }

    /// Returns the canonical semantic values referenced by checked analysis.
    pub const fn semantic_values(&self) -> &'unit SemanticValueStore {
        self.semantic_values
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub const fn available_compiler_known_symbols(&self) -> &'unit AvailableCompilerKnownSymbols {
        self.symbols
    }

    pub(crate) fn finalizer_is_complete(
        &self,
        node: AnyBoundNodeId,
        access: StorageAccessId,
    ) -> bool {
        self.completed.contains(&(node, access))
    }

    /// Returns the checked old-value cleanup for one evaluated assignment.
    pub fn replacement(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&'unit StorageReplacementPlan> {
        self.replacements
            .get(&expression)
            .and_then(|index| self.async_analysis.replacements().get(*index))
    }

    /// Returns frame dependencies in the order published by checked async analysis.
    pub fn frame_dependencies(&self) -> &[BoundDependencySubject] {
        self.async_analysis.frame_dependencies()
    }

    /// Returns the checked suspension plan for one suspending expression.
    pub fn suspension(&self, expression: BoundExpressionId) -> Option<&AsyncSuspensionPoint> {
        self.suspensions
            .get(&expression)
            .and_then(|index| self.async_analysis.suspensions().get(*index))
    }

    /// Returns the checked task operation for one selected call.
    pub fn task_operation(&self, expression: BoundExpressionId) -> Option<AsyncTaskOperationKind> {
        self.task_operations.get(&expression).copied()
    }

    /// Returns checked scope-exit plans for active scopes in cleanup order.
    pub(crate) fn cleanup_plans(
        &self,
        active_scopes: &[BoundBlockId],
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Vec<AsyncScopeExitPlan> {
        let scopes = active_scopes.get(scope_depth..).unwrap_or_else(|| {
            panic!(
                "lowering cleanup contract violated: scope depth {scope_depth} exceeds {} active scopes for exit {exit:?}",
                active_scopes.len()
            )
        });

        scopes
            .iter()
            .rev()
            .map(|scope| {
                self.scope_exits
                    .get(&(*scope, exit))
                    .and_then(|index| self.async_analysis.scope_exits().get(*index))
                    // Lowering mutates its builder while retaining these shared immutable plans.
                    .cloned()
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering cleanup contract violated: missing scope-exit plan for scope {scope:?} and exit {exit:?}"
                        )
                    })
            })
            .collect()
    }

    /// Returns the checked cleanup status for one scope exit.
    pub(crate) fn scope_cleanup_status(
        &self,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> ScopeExitCleanupStatus {
        let Some(index) = self.scope_exits.get(&(scope, exit)) else {
            let point = StorageExitPoint::new(scope, exit);

            if self
                .storage_flow
                .reachable_exits()
                .binary_search(&point)
                .is_ok()
            {
                panic!(
                    "lowering cleanup contract violated: missing scope-exit plan for reachable scope {scope:?} and exit {exit:?}"
                );
            }

            return ScopeExitCleanupStatus::Unreachable;
        };

        let plan = self
            .async_analysis
            .scope_exits()
            .get(*index)
            .unwrap_or_else(|| {
                panic!(
                    "lowering cleanup contract violated: scope-exit index for scope {scope:?} and exit {exit:?} is absent"
                )
            });

        if !plan.has_cleanup() {
            ScopeExitCleanupStatus::NoCleanup
        } else {
            ScopeExitCleanupStatus::Cleanup
        }
    }

    /// Returns storage occurrences whose cleanup depends on runtime initialization state.
    pub(crate) fn initialization_guards(&self) -> impl Iterator<Item = StorageAccessId> + '_ {
        self.async_analysis
            .scope_exits()
            .iter()
            .flat_map(|exit| {
                exit.storage()
                    .iter()
                    .filter_map(|decision| match decision.disposition() {
                        AsyncStorageExitDisposition::Cleanup { access, .. } => Some(access),
                        _ => None,
                    })
            })
            .chain(
                self.storage_flow
                    .replacements()
                    .iter()
                    .filter_map(|decision| {
                        if decision.state() != StorageReplacementState::Conditional {
                            return None;
                        }

                        self.storage
                            .root_identity(decision.access())
                            .and_then(|identity| self.storage.root_access(identity))
                    }),
            )
    }

    /// Returns the checked represented-part partition borrowed from async analysis.
    pub(crate) fn cleanup_parts(
        &self,
        identity: StorageIdentityId,
    ) -> Option<&'unit [bray_bound_tree::StorageCleanupPart]> {
        self.async_analysis
            .storage_requirements()
            .iter()
            .find(|requirement| requirement.identity() == identity)
            .and_then(bray_bound_tree::AsyncStorageRequirement::parts)
    }

    pub(crate) fn cleanup_type(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Option<&bray_bound_tree::StorageCleanupType> {
        self.async_analysis
            .cleanup_types()
            .iter()
            .find(|shape| shape.ty() == ty)
    }

    /// Returns whether one storage identity participates in any lifecycle phase.
    pub(crate) fn requires_lifecycle_storage(&self, storage: StorageIdentityId) -> bool {
        self.lifecycle_storage.contains(&storage)
    }

    /// Adds the open self realization carried by one static initializer template.
    pub fn with_static_owner(
        mut self,
        reference: bray_symbols::StaticReferenceSelection,
        ty: bray_symbols::TypeId,
    ) -> Self {
        self.static_owner = Some((reference, ty));

        self
    }

    /// Adds explicit runtime imports resolved from trusted source bindings.
    pub fn with_runtime_calls(
        mut self,
        calls: &'unit [(
            bray_symbols::CallableDefinitionId,
            bray_runtime_interface::RuntimeAbiRole,
        )],
    ) -> Self {
        self.runtime_calls = calls;

        self
    }

    /// Returns the runtime role of an explicitly bound imported callable.
    pub(crate) fn runtime_call(
        &self,
        declaration: bray_symbols::CallableDefinitionId,
    ) -> Option<bray_runtime_interface::RuntimeAbiRole> {
        self.runtime_calls
            .iter()
            .find_map(|(candidate, role)| (*candidate == declaration).then_some(*role))
    }

    /// Adds declarations whose source references produce provider-owned native addresses.
    pub fn with_native_static_templates(
        mut self,
        templates: &'unit [bray_symbols::StaticInstanceTemplateId],
    ) -> Self {
        self.native_static_templates = templates;

        self
    }

    /// Returns whether one selected static reference produces a native storage address.
    pub fn is_native_static(&self, template: bray_symbols::StaticInstanceTemplateId) -> bool {
        self.native_static_templates.contains(&template)
    }

    /// Returns the open self realization carried by a static initializer template.
    pub const fn static_owner(
        &self,
    ) -> Option<&(bray_symbols::StaticReferenceSelection, bray_symbols::TypeId)> {
        self.static_owner.as_ref()
    }

    /// Returns the closed value reached by one constant reference occurrence.
    pub fn constant_reference_value(
        &self,
        expression: BoundExpressionId,
    ) -> Option<ConstantValueId> {
        self.constant_reference_values
            .binary_search_by_key(&expression, |(expression, _)| *expression)
            .ok()
            .map(|index| self.constant_reference_values[index].1)
    }

    /// Returns the MIR representation category selected for this source unit.
    pub const fn unit_kind(&self) -> &MirUnitKind {
        &self.unit_kind
    }

    /// Returns target analysis selected for lowering this unit.
    pub const fn target(&self) -> &MirTargetContract {
        &self.target
    }

    /// Creates the canonical source-unit MIR builder for this input.
    pub fn mir_builder(&self) -> MirUnitBuilder {
        // The lowerer retains its checked input while the builder owns the immutable target analysis.
        MirUnitBuilder::for_bound(
            self.unit.identity(),
            self.unit_kind.clone(),
            self.target.clone(),
        )
    }
}

fn assert_input_owner(
    unit: &BoundUnit,
    input_name: &str,
    actual_unit: BoundUnitId,
    actual_kind: BoundUnitKind,
) {
    assert_eq!(
        actual_unit,
        unit.unit(),
        "checked {input_name} belongs to a different bound unit"
    );

    assert_eq!(
        actual_kind,
        unit.key().kind(),
        "checked {input_name} belongs to a different bound-unit category"
    );
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        AsyncScopeExitPlan, AsyncSuspensionKind, AsyncSuspensionPoint, BoundBlock,
        BoundBlockExpression, BoundBlockItem, BoundDependencyContract, BoundExpression,
        BoundExpressionId, BoundNodeOrigin, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit, BoundUnitRoot, CheckedAsync,
        CheckedBodyBehavior, CheckedControlFlow, CheckedDependencyContracts,
        CheckedExpressionTypes, CheckedLiteralValues, CheckedPatterns, CheckedRefinements,
        CheckedSemanticSelections, ControlCompletion, Liveness, StorageExitDecision,
        StorageExitPoint, StorageFlow, StoragePlanBuilder,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{CurrentRunCancellation, SemanticValueStore};
    use bray_testing::{test_bound_unit, test_mir_target, test_runtime_default_unit};

    use super::{LoweringInput, ScopeExitCleanupStatus};

    #[test]
    fn input_borrows_the_canonical_unit_and_matching_side_analysis() {
        let unit = test_bound_unit(4);

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let analysis = empty_expression_inputs(&unit);
        let input = lowering_input(&unit, &control_flow, (&analysis).into());

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));
        assert!(std::ptr::eq(input.expression_types(), &analysis.types));
        assert!(std::ptr::eq(input.patterns(), &analysis.patterns));

        assert!(std::ptr::eq(
            input.semantic_selections(),
            &analysis.selections
        ));

        assert!(std::ptr::eq(input.literal_values(), &analysis.literals));
        assert!(std::ptr::eq(input.storage_plan(), &analysis.storage));
        assert!(std::ptr::eq(input.liveness(), &analysis.liveness));
        assert!(std::ptr::eq(input.refinements(), &analysis.refinements));
        assert!(std::ptr::eq(input.storage_flow(), &analysis.storage_flow));

        assert!(std::ptr::eq(
            input.dependency_contracts(),
            &analysis.dependencies
        ));

        assert!(input.frame_dependencies().is_empty());
        assert!(std::ptr::eq(input.body_behavior(), &analysis.behavior));
        assert!(std::ptr::eq(input.semantic_values(), &analysis.values));

        assert!(std::ptr::eq(
            input.available_compiler_known_symbols(),
            available_compiler_known_symbols()
        ));
    }

    #[test]
    #[should_panic(expected = "missing scope-exit plan")]
    fn input_preserves_scope_cleanup_lookup_and_reachability() {
        let mut scope = None;
        let mut exit = None;
        let mut unreachable_exit = None;

        let unit = test_runtime_default_unit(27, |tree, origin| {
            let expression = push_unit_expression(tree, origin);

            let block = tree
                .push_block(BoundBlock::new(
                    origin,
                    [BoundBlockItem::Expression(expression)],
                    false,
                ))
                .unwrap_or_else(|error| panic!("test block must fit: {error:?}"));

            let block_expression = tree
                .push_expression(BoundExpression::Block(BoundBlockExpression::new(
                    origin, block, None, false,
                )))
                .unwrap_or_else(|error| panic!("test block expression must fit: {error:?}"));

            scope = Some(block);
            exit = Some(expression.into());
            unreachable_exit = Some(block_expression.into());

            block_expression
        });

        let scope = scope.unwrap_or_else(|| panic!("test scope must be captured"));
        let exit = exit.unwrap_or_else(|| panic!("test exit must be captured"));

        let unreachable_exit =
            unreachable_exit.unwrap_or_else(|| panic!("test unreachable exit must be captured"));

        let kind = unit.key().kind();
        let storage = StoragePlanBuilder::new(unit.unit(), kind).finish();

        let storage_flow = StorageFlow::try_new(
            unit.unit(),
            kind,
            [],
            [],
            [StorageExitPoint::new(scope, exit)],
            [StorageExitDecision::new(
                scope,
                exit,
                [],
                [],
                [],
                [],
                [],
                [],
                false,
            )],
            false,
        )
        .unwrap_or_else(|error| panic!("storage exit must build: {error:?}"));

        let mut analysis = empty_expression_inputs(&unit);
        analysis.storage = storage;
        analysis.storage_flow = storage_flow;

        analysis.async_analysis = CheckedAsync::try_new(
            unit.unit(),
            kind,
            [],
            [],
            [],
            [],
            [],
            [AsyncScopeExitPlan::new(scope, exit, [], [], [], [], false)],
            false,
        )
        .unwrap_or_else(|error| panic!("matching async exit must build: {error:?}"));

        let control_flow = CheckedControlFlow::new(unit.unit(), kind, ControlCompletion::default());

        let input = lowering_input(&unit, &control_flow, (&analysis).into());

        assert_eq!(
            input.scope_cleanup_status(scope, exit),
            ScopeExitCleanupStatus::NoCleanup
        );

        assert_eq!(input.cleanup_plans(&[scope], 0, exit).len(), 1);

        assert_eq!(
            input.scope_cleanup_status(scope, unreachable_exit),
            ScopeExitCleanupStatus::Unreachable
        );

        input.cleanup_plans(&[scope], 0, unreachable_exit);
    }

    #[test]
    #[should_panic(expected = "duplicate suspension")]
    fn input_asserts_duplicate_suspension_indexes() {
        let unit = test_runtime_default_unit(28, push_unit_expression);

        let BoundUnitRoot::Expression(expression) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let point =
            AsyncSuspensionPoint::new(expression, AsyncSuspensionKind::Yield, None, [], [], false);

        let mut analysis = empty_expression_inputs(&unit);

        analysis.async_analysis = CheckedAsync::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [point.clone(), point],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("duplicate suspension analysis must build: {error:?}"));

        let control_flow =
            CheckedControlFlow::new(unit.unit(), unit.key().kind(), ControlCompletion::default());

        let _ = lowering_input(&unit, &control_flow, (&analysis).into());
    }

    #[test]
    fn input_is_safe_to_share_between_lowering_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<LoweringInput<'static>>();
    }

    fn push_unit_expression(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> BoundExpressionId {
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

    struct ExpressionInputs {
        types: CheckedExpressionTypes,
        patterns: CheckedPatterns,
        selections: CheckedSemanticSelections,
        literals: CheckedLiteralValues,
        storage: bray_bound_tree::StoragePlan,
        liveness: Liveness,
        refinements: CheckedRefinements,
        storage_flow: StorageFlow,
        dependencies: CheckedDependencyContracts,
        async_analysis: CheckedAsync,
        behavior: CheckedBodyBehavior,
        values: SemanticValueStore,
    }

    #[derive(Clone, Copy)]
    struct ExpressionInputReferences<'inputs> {
        types: &'inputs CheckedExpressionTypes,
        patterns: &'inputs CheckedPatterns,
        selections: &'inputs CheckedSemanticSelections,
        literals: &'inputs CheckedLiteralValues,
        storage: &'inputs bray_bound_tree::StoragePlan,
        liveness: &'inputs Liveness,
        refinements: &'inputs CheckedRefinements,
        storage_flow: &'inputs StorageFlow,
        dependencies: &'inputs CheckedDependencyContracts,
        async_analysis: &'inputs CheckedAsync,
        behavior: &'inputs CheckedBodyBehavior,
        values: &'inputs SemanticValueStore,
    }

    impl<'inputs> From<&'inputs ExpressionInputs> for ExpressionInputReferences<'inputs> {
        fn from(analysis: &'inputs ExpressionInputs) -> Self {
            Self {
                types: &analysis.types,
                patterns: &analysis.patterns,
                selections: &analysis.selections,
                literals: &analysis.literals,
                storage: &analysis.storage,
                liveness: &analysis.liveness,
                refinements: &analysis.refinements,
                storage_flow: &analysis.storage_flow,
                dependencies: &analysis.dependencies,
                async_analysis: &analysis.async_analysis,
                behavior: &analysis.behavior,
                values: &analysis.values,
            }
        }
    }

    fn lowering_input<'inputs>(
        unit: &'inputs BoundUnit,
        control_flow: &'inputs CheckedControlFlow,
        analysis: ExpressionInputReferences<'inputs>,
    ) -> LoweringInput<'inputs> {
        LoweringInput::new(
            unit,
            control_flow,
            analysis.types,
            analysis.patterns,
            analysis.literals,
            analysis.refinements,
            analysis.storage,
            analysis.liveness,
            analysis.storage_flow,
            analysis.dependencies,
            analysis.selections,
            available_compiler_known_symbols(),
            analysis.async_analysis,
            std::collections::BTreeSet::new(),
            analysis.behavior,
            analysis.values,
            &[],
            bray_ir::MirUnitKind::Synchronous,
            test_mir_target(),
        )
    }

    fn empty_expression_inputs(unit: &BoundUnit) -> ExpressionInputs {
        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);
        let patterns = CheckedPatterns::new(unit.unit(), unit.key().kind(), [], [], []);

        let selections = CheckedSemanticSelections::try_new(unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let values = semantic_values();

        let literals = CheckedLiteralValues::try_new(
            unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [],
        )
        .unwrap_or_else(|error| panic!("empty literal values must validate: {error:?}"));

        let storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind()).finish();

        let storage_flow =
            StorageFlow::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
                .unwrap_or_else(|error| panic!("empty storage flow must validate: {error:?}"));

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("empty liveness must validate: {error:?}"));

        let refinements = CheckedRefinements::try_new(unit.unit(), unit.key().kind(), [], false)
            .unwrap_or_else(|error| panic!("empty refinements must validate: {error:?}"));

        let dependencies = CheckedDependencyContracts::try_new(
            unit,
            &storage,
            unit.tree()
                .expressions()
                .map(|(expression, _)| (expression, BoundDependencyContract::new([]))),
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("empty dependency contracts must validate: {error:?}"));

        let async_analysis = CheckedAsync::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [],
            [],
            [],
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("empty async analysis must validate: {error:?}"));

        let behavior = CheckedBodyBehavior::new(
            unit.unit(),
            unit.key().kind(),
            CurrentRunCancellation::NotEntered,
            false,
        );

        ExpressionInputs {
            types,
            patterns,
            selections,
            literals,
            storage,
            liveness,
            refinements,
            storage_flow,
            dependencies,
            async_analysis,
            behavior,
            values,
        }
    }

    fn semantic_values() -> SemanticValueStore {
        SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must initialize: {error:?}"))
    }
}
