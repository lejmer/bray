use bray_bound_tree::{BoundUnit, BoundUnitId, BoundUnitKind, CheckedControlFlowFacts};
use bray_ir::{MirUnitBuilder, MirUnitExecution};
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
    available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
    execution: MirUnitExecution,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic fact belongs to the exact bound unit.
    pub fn try_new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlowFacts,
        available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
        execution: MirUnitExecution,
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

        if matches!(execution, MirUnitExecution::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            control_flow,
            available_compiler_known_symbols,
            execution,
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

    /// Returns target-available compiler-known identities and behavior roles.
    pub const fn available_compiler_known_symbols(&self) -> &'unit AvailableCompilerKnownSymbols {
        self.available_compiler_known_symbols
    }

    /// Returns the checked execution representation selected for this source unit.
    pub const fn execution(&self) -> &MirUnitExecution {
        &self.execution
    }

    /// Transfers this validated input into the canonical source-unit MIR builder.
    pub fn into_mir_builder(self) -> MirUnitBuilder {
        MirUnitBuilder::for_bound(self.unit.identity(), self.execution)
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
    /// A compiler-generated executable host was supplied through a source-unit lowering input.
    ExecutableHostRequiresSyntheticInput,
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, CheckedControlFlowFacts, ControlCompletion};
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_testing::test_bound_unit;

    use super::{LoweringInput, LoweringInputError};

    #[test]
    fn input_borrows_the_canonical_unit_and_matching_side_facts() {
        let unit = test_bound_unit(4);

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::default(),
        );

        let input = match LoweringInput::try_new(
            &unit,
            &control_flow,
            available_compiler_known_symbols(),
            bray_ir::MirUnitExecution::Synchronous,
        ) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        };

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));

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

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &foreign,
                available_compiler_known_symbols(),
                bray_ir::MirUnitExecution::Synchronous,
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
                available_compiler_known_symbols(),
                bray_ir::MirUnitExecution::Synchronous,
            ),
            LoweringInputError::ControlFlowKindMismatch {
                expected: unit.key().kind(),
                actual: bray_bound_tree::BoundUnitKind::RuntimeDefault,
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
}
