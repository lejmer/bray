use bray_bound_tree::{
    BoundBlockId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
    BoundUnitId, BoundUnitKind, CheckedAsyncFacts, CheckedBodyBehavior, CheckedControlFlowFacts,
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedLiteralValues, CheckedPatternFacts,
    CheckedRefinementFacts, CheckedSemanticSelections, LivenessFacts, StorageAccessPlan,
    StorageAccessPurpose, StorageAccessRoot, StorageFlowFacts, StorageOperationDecision,
    StoragePlan,
};
use bray_ir::{MirTargetFacts, MirUnitBuilder, MirUnitKind};
use bray_symbols::AvailableCompilerKnownSymbols;

/// A validated borrowed view of the completed checked HIR required by lowering.
///
/// The view keeps the canonical bound unit and its associated semantic facts
/// separate. Adding another required checker domain extends this input rather than creating a
/// copied or progressively wrapped bound-tree representation.
#[derive(Clone)]
pub struct LoweringInput<'unit> {
    unit: &'unit BoundUnit,
    control_flow: &'unit CheckedControlFlowFacts,
    expression_types: &'unit CheckedExpressionTypes,
    pattern_facts: &'unit CheckedPatternFacts,
    semantic_selections: &'unit CheckedSemanticSelections,
    literal_values: &'unit CheckedLiteralValues,
    storage_plan: &'unit StoragePlan,
    liveness: &'unit LivenessFacts,
    refinements: &'unit CheckedRefinementFacts,
    storage_flow: &'unit StorageFlowFacts,
    dependency_contracts: &'unit CheckedDependencyContracts,
    async_facts: &'unit CheckedAsyncFacts,
    body_behavior: &'unit CheckedBodyBehavior,
    available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
    unit_kind: MirUnitKind,
    target: MirTargetFacts,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic fact belongs to the exact bound unit.
    #[expect(
        clippy::too_many_arguments,
        reason = "each canonical lowering fact remains an independently requestable input"
    )]
    pub fn try_new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlowFacts,
        expression_types: &'unit CheckedExpressionTypes,
        pattern_facts: &'unit CheckedPatternFacts,
        semantic_selections: &'unit CheckedSemanticSelections,
        literal_values: &'unit CheckedLiteralValues,
        storage_plan: &'unit StoragePlan,
        liveness: &'unit LivenessFacts,
        refinements: &'unit CheckedRefinementFacts,
        storage_flow: &'unit StorageFlowFacts,
        dependency_contracts: &'unit CheckedDependencyContracts,
        async_facts: &'unit CheckedAsyncFacts,
        body_behavior: &'unit CheckedBodyBehavior,
        available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
        unit_kind: MirUnitKind,
        target: MirTargetFacts,
    ) -> Result<Self, LoweringInputError> {
        validate_fact_owner(
            unit,
            control_flow.unit(),
            control_flow.kind(),
            LoweringFactKind::ControlFlow,
        )?;

        validate_fact_owner(
            unit,
            expression_types.unit(),
            expression_types.kind(),
            LoweringFactKind::ExpressionTypes,
        )?;

        validate_fact_owner(
            unit,
            pattern_facts.unit(),
            pattern_facts.kind(),
            LoweringFactKind::PatternFacts,
        )?;

        validate_fact_owner(
            unit,
            semantic_selections.unit(),
            semantic_selections.kind(),
            LoweringFactKind::SemanticSelections,
        )?;

        validate_fact_owner(
            unit,
            literal_values.unit(),
            literal_values.kind(),
            LoweringFactKind::LiteralValues,
        )?;

        validate_fact_owner(
            unit,
            storage_plan.unit(),
            storage_plan.kind(),
            LoweringFactKind::StoragePlan,
        )?;

        validate_fact_owner(
            unit,
            liveness.unit(),
            liveness.kind(),
            LoweringFactKind::Liveness,
        )?;

        validate_fact_owner(
            unit,
            refinements.unit(),
            refinements.kind(),
            LoweringFactKind::Refinements,
        )?;

        validate_fact_owner(
            unit,
            storage_flow.unit(),
            storage_flow.kind(),
            LoweringFactKind::StorageFlow,
        )?;

        validate_fact_owner(
            unit,
            dependency_contracts.unit(),
            dependency_contracts.kind(),
            LoweringFactKind::DependencyContracts,
        )?;

        validate_fact_owner(
            unit,
            async_facts.unit(),
            async_facts.kind(),
            LoweringFactKind::Async,
        )?;

        validate_fact_owner(
            unit,
            body_behavior.unit(),
            body_behavior.kind(),
            LoweringFactKind::BodyBehavior,
        )?;

        validate_literal_target(literal_values, &target)?;
        validate_semantic_completeness(unit, expression_types, semantic_selections)?;
        validate_pattern_completeness(unit, pattern_facts)?;
        validate_storage_facts(unit, storage_plan, storage_flow)?;

        if matches!(unit_kind, MirUnitKind::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            control_flow,
            expression_types,
            pattern_facts,
            semantic_selections,
            literal_values,
            storage_plan,
            liveness,
            refinements,
            storage_flow,
            dependency_contracts,
            async_facts,
            body_behavior,
            available_compiler_known_symbols,
            unit_kind,
            target,
        })
    }

    /// Returns the canonical checked source-shaped semantic unit.
    pub const fn unit(&self) -> &'unit BoundUnit {
        self.unit
    }

    /// Returns the durable control-flow facts established for the unit.
    pub const fn control_flow(&self) -> &'unit CheckedControlFlowFacts {
        self.control_flow
    }

    /// Returns final types for every bound expression occurrence.
    pub const fn expression_types(&self) -> &'unit CheckedExpressionTypes {
        self.expression_types
    }

    /// Returns checked pattern operations, binding types, and match coverage.
    pub const fn pattern_facts(&self) -> &'unit CheckedPatternFacts {
        self.pattern_facts
    }

    /// Returns exact semantic choices for the unit.
    pub const fn semantic_selections(&self) -> &'unit CheckedSemanticSelections {
        self.semantic_selections
    }

    /// Returns source literals adapted to their final checked types.
    pub const fn literal_values(&self) -> &'unit CheckedLiteralValues {
        self.literal_values
    }

    /// Returns exact storage identities, relationships, and evaluated accesses.
    pub const fn storage_plan(&self) -> &'unit StoragePlan {
        self.storage_plan
    }

    /// Returns durable last-use and lexical lifetime decisions.
    pub const fn liveness(&self) -> &'unit LivenessFacts {
        self.liveness
    }

    /// Returns flow-sensitive facts available at checked operation occurrences.
    pub const fn refinements(&self) -> &'unit CheckedRefinementFacts {
        self.refinements
    }

    /// Returns checked ownership, movement, and borrow decisions.
    pub const fn storage_flow(&self) -> &'unit StorageFlowFacts {
        self.storage_flow
    }

    /// Returns normalized dependency contracts for unit-local semantic occurrences.
    pub const fn dependency_contracts(&self) -> &'unit CheckedDependencyContracts {
        self.dependency_contracts
    }

    /// Returns async frame, suspension, task, and cleanup decisions.
    pub const fn async_facts(&self) -> &'unit CheckedAsyncFacts {
        self.async_facts
    }

    /// Returns normalized effects, capabilities, and lifecycle obligations.
    pub const fn body_behavior(&self) -> &'unit CheckedBodyBehavior {
        self.body_behavior
    }

    /// Returns target-available compiler-known identities and behavior roles.
    pub const fn available_compiler_known_symbols(&self) -> &'unit AvailableCompilerKnownSymbols {
        self.available_compiler_known_symbols
    }

    /// Returns the MIR representation category selected for this source unit.
    pub const fn unit_kind(&self) -> &MirUnitKind {
        &self.unit_kind
    }

    /// Returns target facts selected for lowering this unit.
    pub const fn target(&self) -> &MirTargetFacts {
        &self.target
    }

    /// Transfers this validated input into the canonical source-unit MIR builder.
    pub fn into_mir_builder(self) -> MirUnitBuilder {
        MirUnitBuilder::for_bound(self.unit.identity(), self.unit_kind, self.target)
    }
}

/// A semantic fact required by source-unit lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringFactKind {
    /// Control-flow facts.
    ControlFlow,
    /// Final expression types.
    ExpressionTypes,
    /// Pattern operations, binding types, and match coverage.
    PatternFacts,
    /// Selected callable and operation targets.
    SemanticSelections,
    /// Source-literal values.
    LiteralValues,
    /// Storage identities and occurrence-specific access plans.
    StoragePlan,
    /// Last-use and lexical lifetime decisions.
    Liveness,
    /// Flow-sensitive semantic refinements.
    Refinements,
    /// Checked storage-operation decisions.
    StorageFlow,
    /// Instantiated dependency contracts.
    DependencyContracts,
    /// Async frame, suspension, task, and cleanup decisions.
    Async,
    /// Effects, capabilities, execution requirements, and lifecycle obligations.
    BodyBehavior,
}

/// A contract violation that prevents a bound unit from entering lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringInputError {
    /// A required semantic fact belongs to another bound unit.
    ForeignFact {
        /// The required fact category.
        fact: LoweringFactKind,
        /// The bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied control-flow facts.
        actual: BoundUnitId,
    },
    /// A required semantic fact describes another unit category.
    FactKindMismatch {
        /// The required fact category.
        fact: LoweringFactKind,
        /// The category carried by the bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied fact.
        actual: BoundUnitKind,
    },
    /// A successfully typed expression lacks the semantic choice required by lowering.
    MissingSemanticSelection(BoundExpressionId),
    /// A bound expression has no final checked type.
    MissingExpressionType(BoundExpressionId),
    /// Pattern facts do not cover the bound patterns, bindings, and matches exactly.
    InvalidPatternFacts,
    /// A checked storage operation does not match the canonical storage plan.
    InvalidStorageOperation(BoundExpressionId),
    /// Checked storage operations do not cover every canonical access plan exactly once.
    StorageOperationCountMismatch {
        /// The number of canonical access plans.
        expected: usize,
        /// The number of checked operation decisions.
        actual: usize,
    },
    /// A scope-exit storage decision references an unknown scope, identity, access, or borrow.
    InvalidStorageExit(BoundBlockId),
    /// Literal adaptation used a machine-sized integer width from another target.
    LiteralTargetWidthMismatch {
        /// The width required by the lowering target.
        expected: std::num::NonZeroU16,
        /// The width used while adapting source literals.
        actual: std::num::NonZeroU16,
    },
    /// A compiler-generated executable host was supplied through a source-unit lowering input.
    ExecutableHostRequiresSyntheticInput,
}

fn validate_literal_target(
    literals: &CheckedLiteralValues,
    target: &MirTargetFacts,
) -> Result<(), LoweringInputError> {
    let expected = target.machine().pointer_width_bits();
    let actual = literals.target_integer_width_bits();

    if actual != expected {
        return Err(LoweringInputError::LiteralTargetWidthMismatch { expected, actual });
    }

    Ok(())
}

fn validate_semantic_completeness(
    unit: &BoundUnit,
    types: &CheckedExpressionTypes,
    selections: &CheckedSemanticSelections,
) -> Result<(), LoweringInputError> {
    for (expression, node) in unit.tree().expressions() {
        let Some(result) = types.expression(expression) else {
            return Err(LoweringInputError::MissingExpressionType(expression));
        };

        if result.is_recovered() {
            continue;
        }

        if requires_semantic_selection(node) && selections.expression(expression).is_none() {
            return Err(LoweringInputError::MissingSemanticSelection(expression));
        }
    }

    Ok(())
}

fn validate_pattern_completeness(
    unit: &BoundUnit,
    facts: &CheckedPatternFacts,
) -> Result<(), LoweringInputError> {
    let patterns_match = unit
        .tree()
        .patterns()
        .map(|(pattern, _)| pattern)
        .eq(facts.patterns().iter().map(|entry| entry.pattern()));

    let bindings_match = unit
        .local_symbols()
        .bindings()
        .iter()
        .map(bray_symbols::LocalBindingSymbol::id)
        .eq(facts.binding_types().iter().map(|entry| entry.binding()));

    let matches_match = unit
        .tree()
        .expressions()
        .filter_map(|(expression, node)| {
            matches!(node, BoundExpression::Match(_)).then_some(expression)
        })
        .eq(facts.matches().iter().map(|entry| entry.expression()));

    if !patterns_match || !bindings_match || !matches_match {
        return Err(LoweringInputError::InvalidPatternFacts);
    }

    Ok(())
}

fn validate_storage_facts(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlowFacts,
) -> Result<(), LoweringInputError> {
    let plans = storage.access_plans();
    let decisions = flow.operations();

    if plans.len() != decisions.len() {
        return Err(LoweringInputError::StorageOperationCountMismatch {
            expected: plans.len(),
            actual: decisions.len(),
        });
    }

    for (plan, decision) in plans.iter().copied().zip(decisions.iter().copied()) {
        let plan_matches = plan.expression() == decision.expression()
            && plan.access() == decision.access()
            && plan.purpose().matches_checked(decision.purpose());

        let has_expression = unit.view().expression(decision.expression()).is_some();

        let has_access = storage.access(decision.access()).is_some();

        let borrow_matches = storage_borrow_matches(storage, plan, decision);

        if !plan_matches || !has_expression || !has_access || !borrow_matches {
            return Err(LoweringInputError::InvalidStorageOperation(
                decision.expression(),
            ));
        }
    }

    for exit in flow.exits() {
        let has_scope = unit.view().block(exit.scope()).is_some();

        let has_identities = exit
            .initialized()
            .iter()
            .all(|identity| storage.identity(*identity).is_some());

        let has_accesses = exit
            .moved()
            .iter()
            .all(|access| storage.access(*access).is_some());

        let has_borrows = exit
            .active_borrows()
            .iter()
            .all(|borrow| storage.borrow_capability(*borrow).is_some());

        if !has_scope || !has_identities || !has_accesses || !has_borrows {
            return Err(LoweringInputError::InvalidStorageExit(exit.scope()));
        }
    }

    Ok(())
}

fn storage_borrow_matches(
    storage: &StoragePlan,
    plan: StorageAccessPlan,
    decision: StorageOperationDecision,
) -> bool {
    let StorageAccessPurpose::Borrow(kind) = plan.purpose() else {
        return decision.borrow().is_none();
    };

    let Some(borrow) = decision.borrow() else {
        return false;
    };

    let Some(capability) = storage.borrow_capability(borrow) else {
        return false;
    };

    if capability.kind() != kind {
        return false;
    }

    let created_by_expression =
        capability.expression() == Some(plan.expression()) && capability.access() == plan.access();

    let inherited_from_access = storage
        .access(plan.access())
        .is_some_and(|access| access.root() == StorageAccessRoot::Borrow(borrow));

    created_by_expression || inherited_from_access
}

const fn requires_semantic_selection(expression: &BoundExpression) -> bool {
    match expression {
        BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Call(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_)
        | BoundExpression::For(_)
        | BoundExpression::Generator(_)
        | BoundExpression::MemberAccess(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::TraitQualifiedMember(_)
        | BoundExpression::PatternReference(_) => true,
        BoundExpression::Structured(expression) => matches!(
            expression.kind(),
            BoundStructuredExpressionKind::ElementIndex
                | BoundStructuredExpressionKind::SliceIndex
                | BoundStructuredExpressionKind::TypeFormConstruction
                | BoundStructuredExpressionKind::BooleanAllFold
                | BoundStructuredExpressionKind::BooleanAnyFold
        ),
        BoundExpression::Block(_)
        | BoundExpression::Literal(_)
        | BoundExpression::Name(_)
        | BoundExpression::UnresolvedReference(_)
        | BoundExpression::Assignment(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::AnonymousCallable(_)
        | BoundExpression::Await(_)
        | BoundExpression::ControlTransfer(_)
        | BoundExpression::Match(_)
        | BoundExpression::Error(_) => false,
    }
}

fn validate_fact_owner(
    unit: &BoundUnit,
    actual_unit: BoundUnitId,
    actual_kind: BoundUnitKind,
    fact: LoweringFactKind,
) -> Result<(), LoweringInputError> {
    if actual_unit != unit.unit() {
        return Err(LoweringInputError::ForeignFact {
            fact,
            expected: unit.unit(),
            actual: actual_unit,
        });
    }

    if actual_kind != unit.key().kind() {
        return Err(LoweringInputError::FactKindMismatch {
            fact,
            expected: unit.key().kind(),
            actual: actual_kind,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU16;

    use bray_bound_tree::{
        BorrowCapabilityOrigin, BoundConversionExpression, BoundExpression, BoundExpressionId,
        BoundStructuredExpression, BoundStructuredExpressionKind, BoundUnit, BoundUnitId,
        BoundUnitRoot, CheckedAsyncFacts, CheckedBodyBehavior, CheckedControlFlowFacts,
        CheckedDependencyContracts, CheckedExpressionTypes, CheckedLiteralValues,
        CheckedPatternFacts, CheckedRefinementFacts, CheckedSemanticSelections, ControlCompletion,
        ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus, LivenessFacts,
        PlannedBorrowCapability, StorageAccess, StorageAccessPurpose, StorageAccessRoot,
        StorageFlowFacts, StorageIdentity, StorageOperationDecision, StorageOperationStatus,
        StoragePlanBuilder,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{BorrowKind, CurrentRunCancellation, SemanticValueStore, TypeData, TypeId};
    use bray_testing::{test_bound_unit, test_expression_unit, test_mir_target};

    use super::{LoweringFactKind, LoweringInput, LoweringInputError};

    #[test]
    fn input_borrows_the_canonical_unit_and_matching_side_facts() {
        let unit = test_bound_unit(4);

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let facts = empty_expression_facts(&unit);

        let input = match LoweringInput::try_new(
            &unit,
            &control_flow,
            &facts.types,
            &facts.patterns,
            &facts.selections,
            &facts.literals,
            &facts.storage,
            &facts.liveness,
            &facts.refinements,
            &facts.storage_flow,
            &facts.dependencies,
            &facts.async_facts,
            &facts.behavior,
            available_compiler_known_symbols(),
            bray_ir::MirUnitKind::Synchronous,
            test_mir_target(),
        ) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        };

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));
        assert!(std::ptr::eq(input.expression_types(), &facts.types));
        assert!(std::ptr::eq(input.pattern_facts(), &facts.patterns));
        assert!(std::ptr::eq(input.semantic_selections(), &facts.selections));
        assert!(std::ptr::eq(input.literal_values(), &facts.literals));
        assert!(std::ptr::eq(input.storage_plan(), &facts.storage));
        assert!(std::ptr::eq(input.liveness(), &facts.liveness));
        assert!(std::ptr::eq(input.refinements(), &facts.refinements));
        assert!(std::ptr::eq(input.storage_flow(), &facts.storage_flow));

        assert!(std::ptr::eq(
            input.dependency_contracts(),
            &facts.dependencies
        ));

        assert!(std::ptr::eq(input.async_facts(), &facts.async_facts));
        assert!(std::ptr::eq(input.body_behavior(), &facts.behavior));

        assert!(std::ptr::eq(
            input.available_compiler_known_symbols(),
            available_compiler_known_symbols()
        ));
    }

    #[test]
    fn input_rejects_foreign_and_wrong_category_side_facts() {
        let unit = test_bound_unit(4);
        let facts = empty_expression_facts(&unit);

        let foreign = CheckedControlFlowFacts::new(
            BoundUnitId::new(5),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &foreign,
                &facts.types,
                &facts.patterns,
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.liveness,
                &facts.refinements,
                &facts.storage_flow,
                &facts.dependencies,
                &facts.async_facts,
                &facts.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ForeignFact {
                fact: LoweringFactKind::ControlFlow,
                expected: BoundUnitId::new(4),
                actual: BoundUnitId::new(5),
            },
        );

        let wrong_kind = CheckedControlFlowFacts::new(
            unit.unit(),
            bray_bound_tree::BoundUnitKind::RuntimeDefault,
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &wrong_kind,
                &facts.types,
                &facts.patterns,
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.liveness,
                &facts.refinements,
                &facts.storage_flow,
                &facts.dependencies,
                &facts.async_facts,
                &facts.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::FactKindMismatch {
                fact: LoweringFactKind::ControlFlow,
                expected: unit.key().kind(),
                actual: bray_bound_tree::BoundUnitKind::RuntimeDefault,
            },
        );
    }

    #[test]
    fn input_rejects_expression_facts_from_another_unit() {
        let unit = test_bound_unit(4);
        let foreign_unit = test_bound_unit(5);
        let local = empty_expression_facts(&unit);
        let foreign = empty_expression_facts(&foreign_unit);

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &foreign.types,
                &local.patterns,
                &local.selections,
                &local.literals,
                &local.storage,
                &local.liveness,
                &local.refinements,
                &local.storage_flow,
                &local.dependencies,
                &local.async_facts,
                &local.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ForeignFact {
                fact: LoweringFactKind::ExpressionTypes,
                expected: unit.unit(),
                actual: foreign_unit.unit(),
            },
        );

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &local.types,
                &foreign.patterns,
                &local.selections,
                &local.literals,
                &local.storage,
                &local.liveness,
                &local.refinements,
                &local.storage_flow,
                &local.dependencies,
                &local.async_facts,
                &local.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ForeignFact {
                fact: LoweringFactKind::PatternFacts,
                expected: unit.unit(),
                actual: foreign_unit.unit(),
            },
        );
    }

    #[test]
    fn input_rejects_literal_values_adapted_for_another_target_width() {
        let unit = test_bound_unit(8);

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let expected = test_mir_target().machine().pointer_width_bits();

        let actual = if expected.get() == 32 {
            NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN)
        } else {
            NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN)
        };

        let facts = empty_expression_facts_with_width(&unit, actual);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &facts.types,
                &facts.patterns,
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.liveness,
                &facts.refinements,
                &facts.storage_flow,
                &facts.dependencies,
                &facts.async_facts,
                &facts.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::LiteralTargetWidthMismatch { expected, actual },
        );
    }

    #[test]
    fn input_rejects_bound_expressions_without_final_types() {
        let unit = test_expression_unit(6, |tree, origin| {
            match tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Unit,
                [],
                [],
                [],
                None,
                false,
            ))) {
                Ok(expression) => expression,
                Err(error) => panic!("test unit expression must fit: {error:?}"),
            }
        });

        let BoundUnitRoot::Expression(expression) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let facts = empty_expression_facts(&unit);

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &facts.types,
                &facts.patterns,
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.liveness,
                &facts.refinements,
                &facts.storage_flow,
                &facts.dependencies,
                &facts.async_facts,
                &facts.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::MissingExpressionType(expression),
        );
    }

    #[test]
    fn input_rejects_valid_expressions_without_required_semantic_selections() {
        let unit = test_expression_unit(6, |tree, origin| {
            let unit_expression = match tree.push_expression(BoundExpression::Structured(
                BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Unit,
                    [],
                    [],
                    [],
                    None,
                    false,
                ),
            )) {
                Ok(expression) => expression,
                Err(error) => panic!("test unit expression must fit: {error:?}"),
            };

            match tree.push_expression(BoundExpression::Conversion(BoundConversionExpression::new(
                origin,
                unit_expression,
                origin.source_anchor().syntax(),
                None,
                None,
                false,
            ))) {
                Ok(expression) => expression,
                Err(error) => panic!("test conversion must fit: {error:?}"),
            }
        });

        let BoundUnitRoot::Expression(conversion) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let Some(BoundExpression::Conversion(source)) = unit.view().expression(conversion) else {
            panic!("test root must be a conversion");
        };

        let values = semantic_values();

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [
                ExpressionTypeEntry::new(source.operand(), result),
                ExpressionTypeEntry::new(conversion, result),
            ],
        );

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [],
        )
        .unwrap_or_else(|error| panic!("literal-free values must validate: {error:?}"));

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let (storage, storage_flow) = empty_storage_facts(&unit);

        let patterns = CheckedPatternFacts::new(unit.unit(), unit.key().kind(), [], [], []);
        let ancillary = empty_expression_facts(&unit);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &types,
                &patterns,
                &selections,
                &literals,
                &storage,
                &ancillary.liveness,
                &ancillary.refinements,
                &storage_flow,
                &ancillary.dependencies,
                &ancillary.async_facts,
                &ancillary.behavior,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::MissingSemanticSelection(conversion),
        );
    }

    #[test]
    fn input_rejects_storage_flow_that_disagrees_with_the_storage_plan() {
        let (unit, expression, reached_type) = storage_expression_unit(7);

        let Some(bound) = unit.view().expression(expression) else {
            panic!("test expression must remain available");
        };

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test identity must validate: {error:?}"));

        let access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                reached_type,
                bound.origin().source_anchor(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test access must validate: {error:?}"));

        storage
            .plan_access(expression, StorageAccessPurpose::Read, access)
            .unwrap_or_else(|error| panic!("test access plan must validate: {error:?}"));

        let storage = storage.finish();

        let incomplete_flow =
            StorageFlowFacts::try_new(unit.unit(), unit.key().kind(), [], [], false)
                .unwrap_or_else(|error| panic!("incomplete test flow must validate: {error:?}"));

        assert_eq!(
            super::validate_storage_facts(&unit, &storage, &incomplete_flow),
            Err(LoweringInputError::StorageOperationCountMismatch {
                expected: 1,
                actual: 0,
            })
        );

        let flow = StorageFlowFacts::try_new(
            unit.unit(),
            unit.key().kind(),
            [StorageOperationDecision::new(
                expression,
                StorageAccessPurpose::Write,
                access,
                None,
                StorageOperationStatus::Valid,
            )],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must validate: {error:?}"));

        assert_eq!(
            super::validate_storage_facts(&unit, &storage, &flow),
            Err(LoweringInputError::InvalidStorageOperation(expression))
        );
    }

    #[test]
    fn input_rejects_a_borrow_decision_with_the_wrong_capability() {
        let (unit, expression, reached_type) = storage_expression_unit(8);

        let Some(bound) = unit.view().expression(expression) else {
            panic!("test expression must remain available");
        };

        let mut storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let identity = storage
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap_or_else(|error| panic!("test identity must validate: {error:?}"));

        let access = storage
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(identity),
                [],
                reached_type,
                bound.origin().source_anchor(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test access must validate: {error:?}"));

        let wrong_capability = storage
            .push_borrow_capability(PlannedBorrowCapability::new(
                BorrowCapabilityOrigin::Expression(expression),
                BorrowKind::Mutable,
                access,
                None,
                bound.origin().source_anchor(),
                false,
            ))
            .unwrap_or_else(|error| panic!("test borrow capability must validate: {error:?}"));

        storage
            .plan_access(
                expression,
                StorageAccessPurpose::Borrow(BorrowKind::Shared),
                access,
            )
            .unwrap_or_else(|error| panic!("test access plan must validate: {error:?}"));

        let storage = storage.finish();

        let flow = StorageFlowFacts::try_new(
            unit.unit(),
            unit.key().kind(),
            [StorageOperationDecision::new(
                expression,
                StorageAccessPurpose::Borrow(BorrowKind::Shared),
                access,
                Some(wrong_capability),
                StorageOperationStatus::Valid,
            )],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("test storage flow must validate: {error:?}"));

        assert_eq!(
            super::validate_storage_facts(&unit, &storage, &flow),
            Err(LoweringInputError::InvalidStorageOperation(expression))
        );
    }

    fn storage_expression_unit(id: u32) -> (BoundUnit, BoundExpressionId, TypeId) {
        let unit = test_expression_unit(id, |tree, origin| {
            tree.push_expression(BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Unit,
                [],
                [],
                [],
                None,
                false,
            )))
            .unwrap_or_else(|error| panic!("test expression must fit: {error:?}"))
        });

        let BoundUnitRoot::Expression(expression) = unit.root() else {
            panic!("test expression unit must retain its root");
        };

        let reached_type = semantic_values()
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

        (unit, expression, reached_type)
    }

    #[test]
    fn input_is_safe_to_share_between_lowering_workers() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<LoweringInput<'static>>();
    }

    fn assert_input_error(
        result: Result<LoweringInput<'_>, LoweringInputError>,
        expected: LoweringInputError,
    ) {
        let error = match result {
            Ok(_) => panic!("invalid lowering input must be rejected"),
            Err(error) => error,
        };

        assert_eq!(error, expected);
    }

    struct ExpressionFacts {
        types: CheckedExpressionTypes,
        patterns: CheckedPatternFacts,
        selections: CheckedSemanticSelections,
        literals: CheckedLiteralValues,
        storage: bray_bound_tree::StoragePlan,
        liveness: LivenessFacts,
        refinements: CheckedRefinementFacts,
        storage_flow: StorageFlowFacts,
        dependencies: CheckedDependencyContracts,
        async_facts: CheckedAsyncFacts,
        behavior: CheckedBodyBehavior,
    }

    fn empty_expression_facts(unit: &BoundUnit) -> ExpressionFacts {
        empty_expression_facts_with_width(unit, test_mir_target().machine().pointer_width_bits())
    }

    fn empty_expression_facts_with_width(
        unit: &BoundUnit,
        target_integer_width_bits: NonZeroU16,
    ) -> ExpressionFacts {
        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);
        let patterns = CheckedPatternFacts::new(unit.unit(), unit.key().kind(), [], [], []);

        let selections = CheckedSemanticSelections::try_new(unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let values = semantic_values();

        let literals =
            CheckedLiteralValues::try_new(unit, &types, &values, target_integer_width_bits, [])
                .unwrap_or_else(|error| panic!("empty literal values must validate: {error:?}"));

        let (storage, storage_flow) = empty_storage_facts(unit);

        let liveness = LivenessFacts::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
            .unwrap_or_else(|error| panic!("empty liveness facts must validate: {error:?}"));

        let refinements =
            CheckedRefinementFacts::try_new(unit.unit(), unit.key().kind(), [], false)
                .unwrap_or_else(|error| panic!("empty refinement facts must validate: {error:?}"));

        let dependencies = CheckedDependencyContracts::try_new(unit, &storage, [], [], [], false)
            .unwrap_or_else(|error| panic!("empty dependency contracts must validate: {error:?}"));

        let async_facts =
            CheckedAsyncFacts::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
                .unwrap_or_else(|error| panic!("empty async facts must validate: {error:?}"));

        let behavior = CheckedBodyBehavior::new(
            unit.unit(),
            unit.key().kind(),
            CurrentRunCancellation::NotEntered,
            false,
        );

        ExpressionFacts {
            types,
            patterns,
            selections,
            literals,
            storage,
            liveness,
            refinements,
            storage_flow,
            dependencies,
            async_facts,
            behavior,
        }
    }

    fn empty_storage_facts(unit: &BoundUnit) -> (bray_bound_tree::StoragePlan, StorageFlowFacts) {
        let storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind()).finish();

        let storage_flow = StorageFlowFacts::try_new(unit.unit(), unit.key().kind(), [], [], false)
            .unwrap_or_else(|error| panic!("empty storage flow must validate: {error:?}"));

        (storage, storage_flow)
    }

    fn semantic_values() -> SemanticValueStore {
        SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must initialize: {error:?}"))
    }
}
