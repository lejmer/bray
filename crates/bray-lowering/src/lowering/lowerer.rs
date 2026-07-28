use std::collections::BTreeMap;

use bray_bound_tree::{BoundCallableBodyKind, BoundNodeOrigin, BoundUnitRoot, StorageIdentityId};
use bray_declarations::SyntaxAnchor;
use bray_ir::{
    MirBlockId, MirBlockKind, MirOperand, MirPlace, MirSourceAnchor, MirStorageId, MirUnit,
    MirUnitBuilder,
};

use super::LoweringError;
use crate::LoweringInput;

pub(super) struct YieldTarget {
    pub(super) syntax: SyntaxAnchor,
    pub(super) block: MirBlockId,
    pub(super) result_type: bray_symbols::TypeId,
}

pub(super) struct LoopTarget {
    pub(super) syntax: SyntaxAnchor,
    pub(super) continue_block: MirBlockId,
    pub(super) break_block: MirBlockId,
    pub(super) result_type: bray_symbols::TypeId,
}

pub(super) struct Lowerer<'unit> {
    pub(super) input: LoweringInput<'unit>,
    pub(super) builder: MirUnitBuilder,
    pub(super) storages: BTreeMap<StorageIdentityId, MirStorageId>,
    pub(super) yield_targets: Vec<YieldTarget>,
    pub(super) loop_targets: Vec<LoopTarget>,
}

/// Lowers one complete checked semantic unit into validated backend-independent MIR.
pub fn lower_unit(input: LoweringInput<'_>) -> Result<MirUnit, LoweringError> {
    Lowerer::new(input).lower()
}

impl<'unit> Lowerer<'unit> {
    fn new(input: LoweringInput<'unit>) -> Self {
        let builder = input.mir_builder();

        Self {
            input,
            builder,
            storages: BTreeMap::new(),
            yield_targets: Vec::new(),
            loop_targets: Vec::new(),
        }
    }

    fn lower(mut self) -> Result<MirUnit, LoweringError> {
        let root = self.input.unit().root();
        let source = self.source(BoundNodeOrigin::source(self.input.unit().key().source()));
        let entry = self.builder.push_block(source, MirBlockKind::Ordinary)?;

        let completion = match root {
            BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
                let body_id = body;

                let body = self
                    .input
                    .unit()
                    .view()
                    .callable_body(body_id)
                    .ok_or_else(|| LoweringError::MissingBoundNode(body_id.into()))?;

                let block = match body.kind() {
                    BoundCallableBodyKind::Block(block) => block,
                    BoundCallableBodyKind::Error(_) => {
                        return Err(LoweringError::RecoveredBoundNode(body_id.into()));
                    }
                };

                self.lower_block(block, entry)?
            }
            BoundUnitRoot::Expression(_) | BoundUnitRoot::ExpressionSequence(_) => {
                return Err(LoweringError::UnsupportedRoot(root));
            }
        };

        if let Some(block) = completion.block {
            self.builder.set_terminator(
                block,
                completion.source,
                bray_ir::MirTerminatorKind::Return(completion.value),
            )?;
        }

        self.builder.finish(entry).map_err(Into::into)
    }

    pub(super) fn source(
        &self,
        origin: bray_bound_tree::BoundNodeOrigin,
    ) -> bray_ir::MirSourceAnchor {
        bray_ir::MirSourceAnchor::source(origin)
    }

    pub(super) fn retained_source(source: &MirSourceAnchor) -> MirSourceAnchor {
        // Every MIR record owns provenance while sharing its immutable source-backed data.
        source.clone()
    }

    pub(super) fn retained_operand(operand: &MirOperand) -> MirOperand {
        // Independent MIR records must own the same immutable operand descriptor.
        operand.clone()
    }

    pub(super) fn retained_place(place: &MirPlace) -> MirPlace {
        // Independent MIR records must own the same immutable place descriptor.
        place.clone()
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBinaryExpression, BoundBlock, BoundBlockItem, BoundCallableBody,
        BoundControlTransferExpression, BoundControlTransferKind, BoundDependencyContract,
        BoundExpression, BoundExpressionId, BoundLiteralExpression, BoundLiteralKind,
        BoundOperator, BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitRoot, CheckedAsyncFacts,
        CheckedBodyBehavior, CheckedControlFlowFacts, CheckedDependencyContracts,
        CheckedExpressionTypes, CheckedLiteralValueEntry, CheckedLiteralValues,
        CheckedPatternFacts, CheckedRefinementFacts, CheckedSemanticSelections, ControlCompletion,
        ControlCompletionKind, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
        LivenessFacts, OperatorTarget, SelectedOperation, SemanticSelection,
        SemanticSelectionEntry, StorageFlowFacts, StoragePlanBuilder,
    };
    use bray_ir::{MirBinaryOperator, MirOperationKind, MirTerminatorKind, MirUnitKind};
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{
        ConstantValueData, ConstantValueKind, CurrentRunCancellation, IntegerConstant,
        SemanticValueStore, TypeData,
    };
    use bray_testing::{test_bound_unit, test_mir_target};

    use super::lower_unit;
    use crate::LoweringInput;

    #[test]
    fn lowering_publishes_selected_binary_operations_and_returns() {
        let fixture = lowering_fixture(80, BoundOperator::Add);
        let input = fixture.input();

        let mir = lower_unit(input)
            .unwrap_or_else(|error| panic!("checked synchronous unit must lower: {error:?}"));

        assert_eq!(mir.blocks().len(), 1);
        assert_eq!(mir.operations().len(), 1);

        assert!(matches!(
            mir.operations()[0].kind(),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Add,
                ..
            }
        ));

        assert!(matches!(
            mir.blocks()[0].terminator().kind(),
            MirTerminatorKind::Return(Some(bray_ir::MirOperand::Value(_)))
        ));
    }

    #[test]
    fn lowering_makes_short_circuit_control_flow_explicit() {
        let fixture = lowering_fixture(81, BoundOperator::LogicalAnd);
        let input = fixture.input();

        let mir = lower_unit(input)
            .unwrap_or_else(|error| panic!("checked short-circuit unit must lower: {error:?}"));

        assert_eq!(mir.blocks().len(), 3);
        assert!(mir.operations().is_empty());

        assert!(matches!(
            mir.blocks()[0].terminator().kind(),
            MirTerminatorKind::Branch { .. }
        ));

        assert!(matches!(
            mir.blocks()[1].terminator().kind(),
            MirTerminatorKind::Goto(_)
        ));

        assert!(matches!(
            mir.blocks()[2].terminator().kind(),
            MirTerminatorKind::Return(Some(bray_ir::MirOperand::Value(_)))
        ));
    }

    struct LoweringFixture {
        unit: BoundUnit,
        control_flow: CheckedControlFlowFacts,
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
        values: SemanticValueStore,
    }

    impl LoweringFixture {
        fn input(&self) -> LoweringInput<'_> {
            LoweringInput::try_new(
                &self.unit,
                &self.control_flow,
                &self.types,
                &self.patterns,
                &self.selections,
                &self.literals,
                &self.storage,
                &self.liveness,
                &self.refinements,
                &self.storage_flow,
                &self.dependencies,
                &self.async_facts,
                &self.behavior,
                &self.values,
                available_compiler_known_symbols(),
                MirUnitKind::Synchronous,
                test_mir_target(),
            )
            .unwrap_or_else(|error| panic!("test lowering input must validate: {error:?}"))
        }
    }

    fn lowering_fixture(unit_id: u32, operator: BoundOperator) -> LoweringFixture {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test expression type must intern: {error:?}"));

        let template = test_bound_unit(unit_id);
        let origin = bray_bound_tree::BoundNodeOrigin::source(template.key().source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit_id));

        let left = push_expression(
            &mut tree,
            BoundExpression::Literal(BoundLiteralExpression::new(
                origin,
                template.key().source().syntax().full_range(),
                BoundLiteralKind::Integer,
                Some(ty),
                false,
            )),
        );

        let right = push_expression(
            &mut tree,
            BoundExpression::Literal(BoundLiteralExpression::new(
                origin,
                template.key().source().syntax().full_range(),
                BoundLiteralKind::Integer,
                Some(ty),
                false,
            )),
        );

        let binary = push_expression(
            &mut tree,
            BoundExpression::Binary(BoundBinaryExpression::new(
                origin,
                operator,
                [left, right],
                Some(ty),
                false,
            )),
        );

        let return_expression = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Return,
                Some(binary),
                None,
                Some(ty),
                false,
            )),
        );

        let block = tree
            .push_block(BoundBlock::new(
                origin,
                [BoundBlockItem::Expression(return_expression)],
                false,
            ))
            .unwrap_or_else(|error| panic!("test block must fit: {error:?}"));

        let body = tree
            .push_callable_body(BoundCallableBody::block(origin, block))
            .unwrap_or_else(|error| panic!("test callable body must fit: {error:?}"));

        let unit = BoundUnit::try_new(
            template.key().clone(),
            tree.finish(),
            template.local_symbols().clone(),
            [],
            BoundUnitRoot::CallableBody(body),
        )
        .unwrap_or_else(|error| panic!("test bound unit must validate: {error:?}"));

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);
        let expressions = [left, right, binary, return_expression];

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .iter()
                .copied()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        );

        let selection = SemanticSelectionEntry::new(
            binary,
            SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(operator),
                result_type: ty,
            }),
        );

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [selection])
            .unwrap_or_else(|error| panic!("test operation selection must validate: {error:?}"));

        let left_value = constant_value(&values, ty, 1);
        let right_value = constant_value(&values, ty, 2);

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [
                CheckedLiteralValueEntry::new(left, left_value),
                CheckedLiteralValueEntry::new(right, right_value),
            ],
        )
        .unwrap_or_else(|error| panic!("test literal values must validate: {error:?}"));

        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::from_kinds([ControlCompletionKind::Return]),
        );

        let patterns = CheckedPatternFacts::new(unit.unit(), unit.key().kind(), [], [], []);
        let storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind()).finish();

        let storage_flow =
            StorageFlowFacts::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
                .unwrap_or_else(|error| panic!("empty storage flow must validate: {error:?}"));

        let liveness = LivenessFacts::try_new(unit.unit(), unit.key().kind(), [], [], [], false)
            .unwrap_or_else(|error| panic!("empty liveness must validate: {error:?}"));

        let refinements =
            CheckedRefinementFacts::try_new(unit.unit(), unit.key().kind(), [], false)
                .unwrap_or_else(|error| panic!("empty refinements must validate: {error:?}"));

        let dependencies = CheckedDependencyContracts::try_new(
            &unit,
            &storage,
            expressions
                .iter()
                .copied()
                .map(|expression| (expression, BoundDependencyContract::new([]))),
            [],
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("empty dependencies must validate: {error:?}"));

        let async_facts =
            CheckedAsyncFacts::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
                .unwrap_or_else(|error| panic!("empty async facts must validate: {error:?}"));

        let behavior = CheckedBodyBehavior::new(
            unit.unit(),
            unit.key().kind(),
            CurrentRunCancellation::NotEntered,
            false,
        );

        LoweringFixture {
            unit,
            control_flow,
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
            values,
        }
    }

    fn push_expression(
        tree: &mut BoundTreeBuilder,
        expression: BoundExpression,
    ) -> BoundExpressionId {
        tree.push_expression(expression)
            .unwrap_or_else(|error| panic!("test expression must fit: {error:?}"))
    }

    fn constant_value(
        values: &SemanticValueStore,
        ty: bray_symbols::TypeId,
        value: u64,
    ) -> bray_symbols::ConstantValueId {
        values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Integer(IntegerConstant::from_u64(value)),
            ))
            .unwrap_or_else(|error| panic!("test constant value must intern: {error:?}"))
    }
}
