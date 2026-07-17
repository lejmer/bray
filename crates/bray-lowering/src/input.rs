use bray_bound_tree::{
    BoundUnit, BoundUnitId, BoundUnitKind, CheckedControlFlowFacts, CheckedExpressionFacts,
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
    expressions: &'unit CheckedExpressionFacts,
    available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
    unit_kind: MirUnitKind,
    target: MirTargetFacts,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic fact belongs to the exact bound unit.
    pub fn try_new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlowFacts,
        expressions: &'unit CheckedExpressionFacts,
        available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
        unit_kind: MirUnitKind,
        target: MirTargetFacts,
    ) -> Result<Self, LoweringInputError> {
        if control_flow.unit() != unit.unit() {
            return Err(LoweringInputError::ForeignControlFlow {
                expected: unit.unit(),
                actual: control_flow.unit(),
            });
        }

        if control_flow.kind() != unit.key().kind() {
            return Err(LoweringInputError::ControlFlowKindMismatch {
                expected: unit.key().kind(),
                actual: control_flow.kind(),
            });
        }

        if expressions.unit() != unit.unit() {
            return Err(LoweringInputError::ForeignExpressionFacts {
                expected: unit.unit(),
                actual: expressions.unit(),
            });
        }

        if expressions.kind() != unit.key().kind() {
            return Err(LoweringInputError::ExpressionFactKindMismatch {
                expected: unit.key().kind(),
                actual: expressions.kind(),
            });
        }

        if matches!(unit_kind, MirUnitKind::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            control_flow,
            expressions,
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

    /// Returns complete checked value-producing expression and invocation facts.
    pub const fn expressions(&self) -> &'unit CheckedExpressionFacts {
        self.expressions
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

/// A contract violation that prevents a bound unit from entering lowering.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweringInputError {
    /// Control-flow facts belong to another compilation-local bound unit.
    ForeignControlFlow {
        /// The canonical bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied control-flow facts.
        actual: BoundUnitId,
    },
    /// Control-flow facts describe another semantic unit category.
    ControlFlowKindMismatch {
        /// The category carried by the canonical bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied control-flow facts.
        actual: BoundUnitKind,
    },
    /// Checked expression facts belong to another compilation-local bound unit.
    ForeignExpressionFacts {
        /// The canonical bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied expression facts.
        actual: BoundUnitId,
    },
    /// Checked expression facts describe another semantic unit category.
    ExpressionFactKindMismatch {
        /// The category carried by the canonical bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied expression facts.
        actual: BoundUnitKind,
    },
    /// A compiler-generated executable host was supplied through a source-unit lowering input.
    ExecutableHostRequiresSyntheticInput,
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundUnit, BoundUnitId, CheckedControlFlowFacts, CheckedExpressionFactInput,
        CheckedExpressionFacts, ControlCompletion,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_testing::{test_bound_unit, test_mir_target};

    use super::{LoweringInput, LoweringInputError};

    #[test]
    fn input_borrows_the_canonical_unit_and_matching_side_facts() {
        let unit = test_bound_unit(4);

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );
        let expressions = empty_expression_facts(&unit);

        let input = match LoweringInput::try_new(
            &unit,
            &control_flow,
            &expressions,
            available_compiler_known_symbols(),
            bray_ir::MirUnitKind::Synchronous,
            test_mir_target(),
        ) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        };

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));
        assert!(std::ptr::eq(input.expressions(), &expressions));

        assert!(std::ptr::eq(
            input.available_compiler_known_symbols(),
            available_compiler_known_symbols()
        ));
    }

    #[test]
    fn input_rejects_foreign_and_wrong_category_side_facts() {
        let unit = test_bound_unit(4);

        let foreign = CheckedControlFlowFacts::new(
            BoundUnitId::new(5),
            unit.key().kind(),
            ControlCompletion::default(),
        );
        let expressions = empty_expression_facts(&unit);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &foreign,
                &expressions,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ForeignControlFlow {
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
                &expressions,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ControlFlowKindMismatch {
                expected: unit.key().kind(),
                actual: bray_bound_tree::BoundUnitKind::RuntimeDefault,
            },
        );
    }

    #[test]
    fn input_rejects_expression_facts_from_another_unit() {
        let unit = test_bound_unit(4);
        let foreign_unit = test_bound_unit(5);
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );
        let foreign = empty_expression_facts(&foreign_unit);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &control_flow,
                &foreign,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ForeignExpressionFacts {
                expected: BoundUnitId::new(4),
                actual: BoundUnitId::new(5),
            },
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

    fn empty_expression_facts(unit: &BoundUnit) -> CheckedExpressionFacts {
        match CheckedExpressionFacts::try_new(unit, CheckedExpressionFactInput::new([])) {
            Ok(facts) => facts,
            Err(error) => panic!("empty test unit must accept empty expression facts: {error:?}"),
        }
    }
}
