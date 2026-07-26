use bray_bound_tree::{
    BoundBlockId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit,
    BoundUnitId, BoundUnitKind, CheckedControlFlowFacts, CheckedExpressionTypes,
    CheckedLiteralValues, CheckedSemanticSelections, StorageFlowFacts, StoragePlan,
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
    semantic_selections: &'unit CheckedSemanticSelections,
    literal_values: &'unit CheckedLiteralValues,
    storage_plan: &'unit StoragePlan,
    storage_flow: &'unit StorageFlowFacts,
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
        semantic_selections: &'unit CheckedSemanticSelections,
        literal_values: &'unit CheckedLiteralValues,
        storage_plan: &'unit StoragePlan,
        storage_flow: &'unit StorageFlowFacts,
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
            storage_flow.unit(),
            storage_flow.kind(),
            LoweringFactKind::StorageFlow,
        )?;

        validate_literal_target(literal_values, &target)?;
        validate_semantic_completeness(unit, expression_types, semantic_selections)?;
        validate_storage_facts(unit, storage_plan, storage_flow)?;

        if matches!(unit_kind, MirUnitKind::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            control_flow,
            expression_types,
            semantic_selections,
            literal_values,
            storage_plan,
            storage_flow,
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

    /// Returns exact callable and operation choices for the unit.
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

    /// Returns checked ownership, movement, and borrow decisions.
    pub const fn storage_flow(&self) -> &'unit StorageFlowFacts {
        self.storage_flow
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
    /// Selected callable and operation targets.
    SemanticSelections,
    /// Source-literal values.
    LiteralValues,
    /// Storage identities and occurrence-specific access plans.
    StoragePlan,
    /// Checked storage-operation decisions.
    StorageFlow,
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
    /// A checked storage operation does not match the canonical storage plan.
    InvalidStorageOperation(BoundExpressionId),
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

fn validate_storage_facts(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlowFacts,
) -> Result<(), LoweringInputError> {
    for decision in flow.operations() {
        let has_plan = storage.expression_plans(decision.expression()).any(|plan| {
            plan.access() == decision.access() && plan.purpose().matches_checked(decision.purpose())
        });

        let has_expression = unit.view().expression(decision.expression()).is_some();

        let has_access = storage.access(decision.access()).is_some();

        let has_borrow = decision
            .borrow()
            .is_none_or(|borrow| storage.borrow_capability(borrow).is_some());

        if !has_plan || !has_expression || !has_access || !has_borrow {
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

const fn requires_semantic_selection(expression: &BoundExpression) -> bool {
    match expression {
        BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Call(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_)
        | BoundExpression::MemberAccess(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::TraitQualifiedMember(_)
        | BoundExpression::PatternReference(_) => true,
        BoundExpression::Structured(expression) => matches!(
            expression.kind(),
            BoundStructuredExpressionKind::ElementIndex
                | BoundStructuredExpressionKind::SliceIndex
                | BoundStructuredExpressionKind::TypeFormConstruction
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
        | BoundExpression::For(_)
        | BoundExpression::Match(_)
        | BoundExpression::Generator(_)
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
        BoundConversionExpression, BoundExpression, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundUnit, BoundUnitId, BoundUnitRoot,
        CheckedControlFlowFacts, CheckedExpressionTypes, CheckedLiteralValues,
        CheckedSemanticSelections, ControlCompletion, ExpressionTypeEntry, ExpressionTypeResult,
        ExpressionTypeStatus, StorageAccess, StorageAccessPurpose, StorageAccessRoot,
        StorageFlowFacts, StorageIdentity, StorageOperationDecision, StorageOperationStatus,
        StoragePlanBuilder,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{SemanticValueStore, TypeData};
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
            &facts.selections,
            &facts.literals,
            &facts.storage,
            &facts.storage_flow,
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
        assert!(std::ptr::eq(input.semantic_selections(), &facts.selections));
        assert!(std::ptr::eq(input.literal_values(), &facts.literals));
        assert!(std::ptr::eq(input.storage_plan(), &facts.storage));
        assert!(std::ptr::eq(input.storage_flow(), &facts.storage_flow));

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
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.storage_flow,
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
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.storage_flow,
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
                &local.selections,
                &local.literals,
                &local.storage,
                &local.storage_flow,
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
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.storage_flow,
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
                &facts.selections,
                &facts.literals,
                &facts.storage,
                &facts.storage_flow,
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

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &types,
                &selections,
                &literals,
                &storage,
                &storage_flow,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::MissingSemanticSelection(conversion),
        );
    }

    #[test]
    fn input_rejects_storage_flow_that_disagrees_with_the_storage_plan() {
        let unit = test_expression_unit(7, |tree, origin| {
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

        let values = semantic_values();

        let reached_type = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test type must intern: {error:?}"));

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
        selections: CheckedSemanticSelections,
        literals: CheckedLiteralValues,
        storage: bray_bound_tree::StoragePlan,
        storage_flow: StorageFlowFacts,
    }

    fn empty_expression_facts(unit: &BoundUnit) -> ExpressionFacts {
        empty_expression_facts_with_width(unit, test_mir_target().machine().pointer_width_bits())
    }

    fn empty_expression_facts_with_width(
        unit: &BoundUnit,
        target_integer_width_bits: NonZeroU16,
    ) -> ExpressionFacts {
        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

        let selections = CheckedSemanticSelections::try_new(unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let values = semantic_values();

        let literals =
            CheckedLiteralValues::try_new(unit, &types, &values, target_integer_width_bits, [])
                .unwrap_or_else(|error| panic!("empty literal values must validate: {error:?}"));

        let (storage, storage_flow) = empty_storage_facts(unit);

        ExpressionFacts {
            types,
            selections,
            literals,
            storage,
            storage_flow,
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
