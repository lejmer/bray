use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, BoundUnit, BoundUnitId,
    BoundUnitKind, CheckedControlFlowFacts, CheckedExpressionTypes, CheckedLiteralValues,
    CheckedSemanticSelections,
};
use bray_ir::{MirTargetFacts, MirUnitBuilder, MirUnitKind};
use bray_symbols::AvailableCompilerKnownSymbols;

/// Checked semantic facts required to lower one bound unit.
#[derive(Clone, Copy)]
pub struct LoweringFacts<'unit> {
    control_flow: &'unit CheckedControlFlowFacts,
    expression_types: &'unit CheckedExpressionTypes,
    semantic_selections: &'unit CheckedSemanticSelections,
    literal_values: &'unit CheckedLiteralValues,
}

impl<'unit> LoweringFacts<'unit> {
    /// Groups the checked facts for one lowering request.
    pub const fn new(
        control_flow: &'unit CheckedControlFlowFacts,
        expression_types: &'unit CheckedExpressionTypes,
        semantic_selections: &'unit CheckedSemanticSelections,
        literal_values: &'unit CheckedLiteralValues,
    ) -> Self {
        Self {
            control_flow,
            expression_types,
            semantic_selections,
            literal_values,
        }
    }
}

/// A validated borrowed view of the completed checked HIR required by lowering.
#[derive(Clone)]
pub struct LoweringInput<'unit> {
    unit: &'unit BoundUnit,
    facts: LoweringFacts<'unit>,
    available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
    unit_kind: MirUnitKind,
    target: MirTargetFacts,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic fact belongs to the exact bound unit.
    pub fn try_new(
        unit: &'unit BoundUnit,
        facts: LoweringFacts<'unit>,
        available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
        unit_kind: MirUnitKind,
        target: MirTargetFacts,
    ) -> Result<Self, LoweringInputError> {
        validate_fact_owner(
            unit,
            facts.control_flow.unit(),
            facts.control_flow.kind(),
            LoweringFactKind::ControlFlow,
        )?;
        validate_fact_owner(
            unit,
            facts.expression_types.unit(),
            facts.expression_types.kind(),
            LoweringFactKind::ExpressionTypes,
        )?;
        validate_fact_owner(
            unit,
            facts.semantic_selections.unit(),
            facts.semantic_selections.kind(),
            LoweringFactKind::SemanticSelections,
        )?;
        validate_fact_owner(
            unit,
            facts.literal_values.unit(),
            facts.literal_values.kind(),
            LoweringFactKind::LiteralValues,
        )?;
        validate_literal_target(facts.literal_values, &target)?;
        validate_semantic_completeness(unit, facts.expression_types, facts.semantic_selections)?;

        if matches!(unit_kind, MirUnitKind::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            facts,
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
        self.facts.control_flow
    }

    /// Returns final canonical types for every bound expression occurrence.
    pub const fn expression_types(&self) -> &'unit CheckedExpressionTypes {
        self.facts.expression_types
    }

    /// Returns exact callable and operation choices for the unit.
    pub const fn semantic_selections(&self) -> &'unit CheckedSemanticSelections {
        self.facts.semantic_selections
    }

    /// Returns source literals adapted to their final checked types.
    pub const fn literal_values(&self) -> &'unit CheckedLiteralValues {
        self.facts.literal_values
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

/// A durable semantic fact required by source-unit lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringFactKind {
    /// Durable control-flow facts.
    ControlFlow,
    /// Final expression types.
    ExpressionTypes,
    /// Selected callable and operation targets.
    SemanticSelections,
    /// Canonical source-literal values.
    LiteralValues,
}

/// A contract violation that prevents a bound unit from entering lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringInputError {
    /// A required semantic fact belongs to another compilation-local bound unit.
    ForeignFact {
        /// The required fact category.
        fact: LoweringFactKind,
        /// The canonical bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied fact.
        actual: BoundUnitId,
    },
    /// A required semantic fact describes another semantic unit category.
    FactKindMismatch {
        /// The required fact category.
        fact: LoweringFactKind,
        /// The category carried by the canonical bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied fact.
        actual: BoundUnitKind,
    },
    /// A successfully typed expression lacks the exact semantic choice required by lowering.
    MissingSemanticSelection(BoundExpressionId),
    /// Literal adaptation did not use a concrete machine-sized integer width.
    PortableLiteralValues,
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
    let Some(actual) = literals.target_integer_width_bits() else {
        return Err(LoweringInputError::PortableLiteralValues);
    };

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
    for entry in types.entries() {
        if entry.result().is_recovered() {
            continue;
        }

        let expression = entry.expression();

        if unit
            .view()
            .expression(expression)
            .is_some_and(requires_semantic_selection)
            && selections.expression(expression).is_none()
        {
            return Err(LoweringInputError::MissingSemanticSelection(expression));
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
        | BoundExpression::TraitQualifiedMember(_) => true,
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
        ExpressionTypeStatus,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{SemanticValueStore, TypeData};
    use bray_testing::{test_bound_unit, test_expression_unit, test_mir_target};

    use super::{LoweringFactKind, LoweringFacts, LoweringInput, LoweringInputError};

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
            lowering_facts(&control_flow, &facts),
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
                lowering_facts(&foreign, &facts),
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
                lowering_facts(&wrong_kind, &facts),
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
                LoweringFacts::new(
                    &control_flow,
                    &foreign.types,
                    &local.selections,
                    &local.literals,
                ),
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
    fn input_rejects_portable_literal_values() {
        let unit = test_bound_unit(7);
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );
        let facts = empty_expression_facts_with_width(&unit, None);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                lowering_facts(&control_flow, &facts),
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::PortableLiteralValues,
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
        let facts = empty_expression_facts_with_width(&unit, Some(actual));

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                lowering_facts(&control_flow, &facts),
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::LiteralTargetWidthMismatch { expected, actual },
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
        let values = match SemanticValueStore::try_new() {
            Ok(values) => values,
            Err(error) => panic!("test semantic values must be available: {error:?}"),
        };
        let ty = match values.intern_type(TypeData::tuple([])) {
            Ok(ty) => ty,
            Err(error) => panic!("test type must be interned: {error:?}"),
        };
        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);
        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            [
                ExpressionTypeEntry::new(source.operand(), result),
                ExpressionTypeEntry::new(conversion, result),
            ],
        );
        let selections = match CheckedSemanticSelections::try_new(&unit, &types, []) {
            Ok(selections) => selections,
            Err(error) => panic!("empty selections must be structurally valid: {error:?}"),
        };
        let literals = match CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            Some(test_mir_target().machine().pointer_width_bits()),
            [],
        ) {
            Ok(literals) => literals,
            Err(error) => panic!("literal-free values must validate: {error:?}"),
        };
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                LoweringFacts::new(&control_flow, &types, &selections, &literals),
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::MissingSemanticSelection(conversion),
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
    }

    const fn lowering_facts<'facts>(
        control_flow: &'facts CheckedControlFlowFacts,
        expressions: &'facts ExpressionFacts,
    ) -> LoweringFacts<'facts> {
        LoweringFacts::new(
            control_flow,
            &expressions.types,
            &expressions.selections,
            &expressions.literals,
        )
    }

    fn empty_expression_facts(unit: &BoundUnit) -> ExpressionFacts {
        empty_expression_facts_with_width(
            unit,
            Some(test_mir_target().machine().pointer_width_bits()),
        )
    }

    fn empty_expression_facts_with_width(
        unit: &BoundUnit,
        target_integer_width_bits: Option<NonZeroU16>,
    ) -> ExpressionFacts {
        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), []);

        let selections = match CheckedSemanticSelections::try_new(unit, &types, []) {
            Ok(selections) => selections,
            Err(error) => panic!("empty selection facts must validate: {error:?}"),
        };

        let values = match bray_symbols::SemanticValueStore::try_new() {
            Ok(values) => values,
            Err(error) => panic!("semantic value store must be available: {error:?}"),
        };

        let literals = match CheckedLiteralValues::try_new(
            unit,
            &types,
            &values,
            target_integer_width_bits,
            [],
        ) {
            Ok(literals) => literals,
            Err(error) => panic!("empty literal facts must validate: {error:?}"),
        };

        ExpressionFacts {
            types,
            selections,
            literals,
        }
    }
}
