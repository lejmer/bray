use bray_bound_tree::{
    BoundUnit, BoundUnitId, BoundUnitKind, CheckedControlFlowFacts, CheckedStorageFacts,
};
use bray_ir::{MirTargetFacts, MirUnitBuilder, MirUnitKind};
use bray_symbols::AvailableCompilerKnownSymbols;

/// A validated borrowed view of the completed checked HIR required by lowering.
///
/// Construction requires the canonical bound unit and every checked fact domain consumed by
/// source-unit lowering. All facts must describe that exact unit and semantic category.
#[derive(Clone)]
pub struct LoweringInput<'unit> {
    unit: &'unit BoundUnit,
    control_flow: &'unit CheckedControlFlowFacts,
    storage: &'unit CheckedStorageFacts,
    available_compiler_known_symbols: &'unit AvailableCompilerKnownSymbols,
    unit_kind: MirUnitKind,
    target: MirTargetFacts,
}

impl<'unit> LoweringInput<'unit> {
    /// Validates that every supplied semantic fact belongs to the exact bound unit.
    pub fn try_new(
        unit: &'unit BoundUnit,
        control_flow: &'unit CheckedControlFlowFacts,
        storage: &'unit CheckedStorageFacts,
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

        if storage.unit() != unit.unit() {
            return Err(LoweringInputError::ForeignStorage {
                expected: unit.unit(),
                actual: storage.unit(),
            });
        }

        if storage.kind() != unit.key().kind() {
            return Err(LoweringInputError::StorageKindMismatch {
                expected: unit.key().kind(),
                actual: storage.kind(),
            });
        }

        if matches!(unit_kind, MirUnitKind::ExecutableHost(_)) {
            return Err(LoweringInputError::ExecutableHostRequiresSyntheticInput);
        }

        Ok(Self {
            unit,
            control_flow,
            storage,
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

    /// Returns exact storage identities, relationships, accesses, and borrow capabilities.
    pub const fn storage(&self) -> &'unit CheckedStorageFacts {
        self.storage
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

    /// Begins MIR construction for this validated source unit.
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
    /// Storage facts belong to another compilation-local bound unit.
    ForeignStorage {
        /// The canonical bound unit requested for lowering.
        expected: BoundUnitId,
        /// The unit named by the supplied storage facts.
        actual: BoundUnitId,
    },
    /// Storage facts describe another semantic unit category.
    StorageKindMismatch {
        /// The category carried by the canonical bound-unit key.
        expected: BoundUnitKind,
        /// The category named by the supplied storage facts.
        actual: BoundUnitKind,
    },
    /// A compiler-generated executable host was supplied through a source-unit lowering input.
    ExecutableHostRequiresSyntheticInput,
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundUnit, BoundUnitId, CheckedControlFlowFacts, CheckedStorageFacts,
        CheckedStorageFactsBuilder, ControlCompletion,
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
        let storage = storage_facts(&unit);

        let input = match LoweringInput::try_new(
            &unit,
            &control_flow,
            &storage,
            available_compiler_known_symbols(),
            bray_ir::MirUnitKind::Synchronous,
            test_mir_target(),
        ) {
            Ok(input) => input,
            Err(error) => panic!("matching lowering input must validate: {error:?}"),
        };

        assert!(std::ptr::eq(input.unit(), &unit));
        assert!(std::ptr::eq(input.control_flow(), &control_flow));
        assert!(std::ptr::eq(input.storage(), &storage));

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
        let storage = storage_facts(&unit);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &foreign,
                &storage,
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
                &storage,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ControlFlowKindMismatch {
                expected: unit.key().kind(),
                actual: bray_bound_tree::BoundUnitKind::RuntimeDefault,
            },
        );

        let foreign_unit = test_bound_unit(5);
        let foreign_storage = storage_facts(&foreign_unit);

        assert_input_error(
            LoweringInput::try_new(
                &unit,
                &CheckedControlFlowFacts::new(
                    unit.unit(),
                    unit.key().kind(),
                    ControlCompletion::default(),
                ),
                &foreign_storage,
                available_compiler_known_symbols(),
                bray_ir::MirUnitKind::Synchronous,
                test_mir_target(),
            ),
            LoweringInputError::ForeignStorage {
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

    fn storage_facts(unit: &BoundUnit) -> CheckedStorageFacts {
        match CheckedStorageFactsBuilder::new(unit.unit(), unit.key().kind())
            .finish(unit.view(), unit.root().node())
        {
            Ok(facts) => facts,
            Err(error) => panic!("test storage facts must be complete: {error:?}"),
        }
    }
}
