use std::collections::BTreeMap;

use bray_bound_tree::{BoundCallableBodyKind, BoundNodeOrigin, BoundUnitRoot, StorageIdentityId};
use bray_declarations::SyntaxAnchor;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirFrameDescriptor, MirFrameStateFacts,
    MirFrameStateId, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor, MirStorageId,
    MirTaskTerminalState, MirTerminatorKind, MirUnit, MirUnitBuilder,
};
use bray_runtime_interface::{ProtectedFrameAbiVersions, RuntimeAbiRole};

use super::LoweringError;
use crate::LoweringInput;

#[derive(Clone)]
pub(super) enum YieldTarget {
    Result {
        syntax: SyntaxAnchor,
        block: MirBlockId,
        result_type: bray_symbols::TypeId,
        scope_depth: usize,
    },
    Generator {
        syntax: SyntaxAnchor,
        destination: MirPlace,
        element_type: bray_symbols::TypeId,
    },
}

impl YieldTarget {
    pub(super) const fn syntax(&self) -> SyntaxAnchor {
        match self {
            Self::Result { syntax, .. } | Self::Generator { syntax, .. } => *syntax,
        }
    }
}

pub(super) struct LoopTarget {
    pub(super) syntax: SyntaxAnchor,
    pub(super) continue_block: MirBlockId,
    pub(super) break_block: MirBlockId,
    pub(super) result_type: bray_symbols::TypeId,
    pub(super) scope_depth: usize,
}

pub(super) struct CatchTarget {
    pub(super) block: MirBlockId,
    pub(super) report_type: bray_symbols::TypeId,
    pub(super) scope_depth: usize,
}

pub(super) struct Lowerer<'unit> {
    pub(super) input: LoweringInput<'unit>,
    pub(super) builder: MirUnitBuilder,
    pub(super) storages: BTreeMap<StorageIdentityId, MirStorageId>,
    pub(super) active_scopes: Vec<bray_bound_tree::BoundBlockId>,
    pub(super) yield_targets: Vec<YieldTarget>,
    pub(super) loop_targets: Vec<LoopTarget>,
    pub(super) catch_targets: Vec<CatchTarget>,
    pub(super) frame_states: Vec<MirFrameStateFacts>,
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
            active_scopes: Vec::new(),
            yield_targets: Vec::new(),
            loop_targets: Vec::new(),
            catch_targets: Vec::new(),
            frame_states: Vec::new(),
        }
    }

    fn lower(mut self) -> Result<MirUnit, LoweringError> {
        let root = self.input.unit().root();
        let source = self.source(BoundNodeOrigin::source(self.input.unit().key().source()));

        let entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        if self.input.unit_kind().protected_frame().is_some() {
            self.frame_states.push(MirFrameStateFacts::new(
                MirFrameStateId::new(0),
                entry,
                self.execution_lane_requirements(),
                None,
                [],
                [],
            ));
        }

        let completion = match root {
            BoundUnitRoot::CallableBody { body, .. }
            | BoundUnitRoot::AnonymousCallable { body, .. } => {
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
            BoundUnitRoot::Expression(expression) => self.lower_expression(expression, entry)?,
            BoundUnitRoot::ExpressionSequence(block) => self.lower_block(block, entry)?,
        };

        if let Some(block) = completion.block {
            if self.input.unit_kind().protected_frame().is_some() {
                let result_type = self
                    .input
                    .expression_types()
                    .callable_result_type()
                    .ok_or(LoweringError::MissingCallableResultType)?;

                let value = completion
                    .value
                    .unwrap_or_else(|| self.unit_operand(result_type));

                self.builder.push_operation(
                    block,
                    Self::retained_source(&completion.source),
                    MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                        state: MirTaskTerminalState::Completed(value),
                        runtime: self.runtime_reference(RuntimeAbiRole::TerminalPublication),
                    }),
                    None,
                )?;

                self.builder.set_terminator(
                    block,
                    completion.source,
                    MirTerminatorKind::Return(None),
                )?;
            } else {
                self.builder.set_terminator(
                    block,
                    completion.source,
                    MirTerminatorKind::Return(completion.value),
                )?;
            }
        }

        if let Some(frame) = self.input.unit_kind().protected_frame() {
            let result_type = self
                .input
                .expression_types()
                .callable_result_type()
                .ok_or(LoweringError::MissingCallableResultType)?;

            let runtime_abi = self.input.target().runtime_abi();

            let descriptor = MirFrameDescriptor::try_new(
                frame,
                runtime_abi,
                ProtectedFrameAbiVersions::uniform(runtime_abi),
                result_type,
                self.frame_states,
            )
            .map_err(|_| LoweringError::InvalidFrameDescriptor)?;

            self.builder.set_frame_descriptor(descriptor)?;
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
        BoundArgument, BoundBinaryExpression, BoundBlock, BoundBlockItem, BoundCallExpression,
        BoundCallResult, BoundCallableBody, BoundCallableTarget, BoundControlTransferExpression,
        BoundControlTransferKind, BoundDependencyContract, BoundErrorExpression, BoundExpression,
        BoundExpressionId, BoundLiteralExpression, BoundLiteralKind, BoundOperator,
        BoundResolvedCall, BoundStructuredExpression, BoundStructuredExpressionKind,
        BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitRoot, CheckedAsyncFacts,
        CheckedBodyBehavior, CheckedControlFlowFacts, CheckedDependencyContracts,
        CheckedExpressionTypes, CheckedLiteralValueEntry, CheckedLiteralValues,
        CheckedPatternFacts, CheckedRefinementFacts, CheckedSemanticSelections, ControlCompletion,
        ControlCompletionKind, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
        LivenessFacts, OperatorTarget, SelectedArgument, SelectedCall, SelectedConversion,
        SelectedOperation, SelectedPropagation, SelectedPropagationBoundary, SemanticSelection,
        SemanticSelectionEntry, StorageFlowFacts, StoragePlanBuilder,
    };
    use bray_ir::{
        MirBinaryOperator, MirCallArgument, MirOperationKind, MirTerminatorKind, MirUnitKind,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{
        CallableAbi, CallableDefinitionId, CallableDependencyContracts, CallableInstanceData,
        CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
        CallablePhaseBehaviors, ConstantValueData, ConstantValueKind, CurrentRunCancellation,
        FunctionSymbolId, GenericOwnerId, GenericSubstitutionData, IntegerConstant,
        SemanticValueStore, SymbolId, TypeData,
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

    #[test]
    fn lowering_makes_nullable_propagation_and_early_cleanup_explicit() {
        let fixture = nullable_propagation_fixture(82);
        let input = fixture.input();

        let mir = lower_unit(input)
            .unwrap_or_else(|error| panic!("checked nullable propagation must lower: {error:?}"));

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::PatternBranch {
                predicate: bray_bound_tree::PatternPredicate::NullablePresent,
                ..
            }
        )));

        assert!(mir.blocks().iter().any(|block| matches!(
            block.terminator().kind(),
            MirTerminatorKind::Return(Some(bray_ir::MirOperand::Immediate {
                value: bray_ir::MirImmediateValue::NullableAbsent,
                ..
            }))
        )));
    }

    #[test]
    fn lowering_retains_selected_call_behavior_and_runtime_defaults() {
        let (fixture, call_type) = selected_call_fixture(83);

        let input = fixture.input();

        let mir = lower_unit(input)
            .unwrap_or_else(|error| panic!("checked selected call must lower: {error:?}"));

        let Some(call) = mir
            .operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                MirOperationKind::Call(call) => Some(call),
                _ => None,
            })
        else {
            panic!("lowered unit must contain its selected call");
        };

        assert_eq!(call.target().abi(), CallableAbi::C);
        assert_eq!(call.result(), BoundCallResult::Immediate(call_type));
        assert!(call.phase_behaviors().is_some());

        assert!(matches!(
            call.arguments(),
            [
                MirCallArgument::Explicit { ordinal: 0, .. },
                MirCallArgument::Default { ordinal: 1, .. }
            ]
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

    fn selected_call_fixture(unit_id: u32) -> (LoweringFixture, bray_symbols::TypeId) {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test expression type must intern: {error:?}"));

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(1));

        let owner = GenericOwnerId::try_new(function.into())
            .unwrap_or_else(|| panic!("test function must be a generic owner"));

        let substitution = values
            .intern_generic_substitution(
                GenericSubstitutionData::try_new(owner, [], [])
                    .unwrap_or_else(|error| panic!("test substitution must validate: {error:?}")),
            )
            .unwrap_or_else(|error| panic!("test substitution must intern: {error:?}"));

        let definition = CallableDefinitionId::try_new(function.into())
            .unwrap_or_else(|| panic!("test function must be callable"));

        let target =
            BoundCallableTarget::Declaration(CallableInstanceData::new(definition, substitution));

        let resolution = BoundResolvedCall::new(target, [], BoundCallResult::Immediate(ty));

        let template = test_bound_unit(unit_id);
        let origin = bray_bound_tree::BoundNodeOrigin::source(template.key().source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit_id));

        let callee = push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, ty)),
        );

        let argument = push_expression(
            &mut tree,
            BoundExpression::Literal(BoundLiteralExpression::new(
                origin,
                template.key().source().syntax().full_range(),
                BoundLiteralKind::Integer,
                Some(ty),
                false,
            )),
        );

        let call = push_expression(
            &mut tree,
            BoundExpression::Call(BoundCallExpression::resolved(
                origin,
                callee,
                [],
                [BoundArgument::new(argument, None, false)],
                resolution.clone(),
            )),
        );

        let return_expression = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Return,
                Some(call),
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
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body,
            },
        )
        .unwrap_or_else(|error| panic!("test bound unit must validate: {error:?}"));

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);
        let expressions = [callee, argument, call, return_expression];

        let types = CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .iter()
                .copied()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        );

        let dependency = values
            .empty_dependency_contract_template()
            .unwrap_or_else(|error| panic!("empty dependency template must exist: {error:?}"));

        let selected = SelectedCall::new(
            resolution,
            CallableAbi::C,
            CallablePhaseBehaviors::empty(CallableDependencyContracts::synchronous(dependency)),
            None,
            [
                SelectedArgument::Explicit {
                    expression: argument,
                    parameter: Some(CallableParameterSymbolId::from_symbol_id(SymbolId::new(2))),
                    ordinal: 0,
                    conversion: SelectedConversion::new(
                        ty,
                        ty,
                        bray_bound_tree::ConversionTarget::Identity,
                    ),
                },
                SelectedArgument::Default {
                    parameter: CallableParameterSymbolId::from_symbol_id(SymbolId::new(3)),
                    ordinal: 1,
                    provider: CallableParameterDefaultProviderSymbolId::from_symbol_id(
                        SymbolId::new(4),
                    ),
                },
            ],
            [],
        );

        let selections = CheckedSemanticSelections::try_new(
            &unit,
            &types,
            [SemanticSelectionEntry::new(
                call,
                SemanticSelection::Call(selected),
            )],
        )
        .unwrap_or_else(|error| panic!("test call selection must validate: {error:?}"));

        let literal = constant_value(&values, ty, 1);

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [CheckedLiteralValueEntry::new(argument, literal)],
        )
        .unwrap_or_else(|error| panic!("test literal values must validate: {error:?}"));

        (
            lowering_fixture_from_parts(
                unit,
                types,
                selections,
                literals,
                values,
                &expressions,
                [ControlCompletionKind::Return],
            ),
            ty,
        )
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
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body,
            },
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

        lowering_fixture_from_parts(
            unit,
            types,
            selections,
            literals,
            values,
            &expressions,
            [ControlCompletionKind::Return],
        )
    }

    fn nullable_propagation_fixture(unit_id: u32) -> LoweringFixture {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let value_type = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test value type must intern: {error:?}"));

        let nullable_type = values
            .intern_type(TypeData::Nullable(value_type))
            .unwrap_or_else(|error| panic!("test nullable type must intern: {error:?}"));

        let template = test_bound_unit(unit_id);
        let origin = bray_bound_tree::BoundNodeOrigin::source(template.key().source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit_id));

        let operand = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Absence,
                [],
                [],
                [],
                Some(nullable_type),
                false,
            )),
        );

        let propagation = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::NullablePropagation,
                [operand],
                [],
                [],
                Some(value_type),
                false,
            )),
        );

        let return_value = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Absence,
                [],
                [],
                [],
                Some(nullable_type),
                false,
            )),
        );

        let return_expression = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Return,
                Some(return_value),
                None,
                Some(value_type),
                false,
            )),
        );

        let block = tree
            .push_block(BoundBlock::new(
                origin,
                [
                    BoundBlockItem::Expression(propagation),
                    BoundBlockItem::Expression(return_expression),
                ],
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
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body,
            },
        )
        .unwrap_or_else(|error| panic!("test bound unit must validate: {error:?}"));

        let entries = [
            ExpressionTypeEntry::new(
                operand,
                ExpressionTypeResult::new(nullable_type, ExpressionTypeStatus::Valid),
            ),
            ExpressionTypeEntry::new(
                propagation,
                ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid),
            ),
            ExpressionTypeEntry::new(
                return_value,
                ExpressionTypeResult::new(nullable_type, ExpressionTypeStatus::Valid),
            ),
            ExpressionTypeEntry::new(
                return_expression,
                ExpressionTypeResult::new(value_type, ExpressionTypeStatus::Valid),
            ),
        ];

        let types = CheckedExpressionTypes::new(unit.unit(), unit.key().kind(), entries)
            .with_callable_result_type(nullable_type);

        let selection = SemanticSelectionEntry::new(
            propagation,
            SemanticSelection::Propagation(SelectedPropagation::Nullable {
                boundary: SelectedPropagationBoundary::Callable,
                result_type: nullable_type,
            }),
        );

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [selection])
            .unwrap_or_else(|error| panic!("propagation selection must validate: {error:?}"));

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [],
        )
        .unwrap_or_else(|error| panic!("empty literal values must validate: {error:?}"));

        let expressions = [operand, propagation, return_value, return_expression];

        lowering_fixture_from_parts(
            unit,
            types,
            selections,
            literals,
            values,
            &expressions,
            [
                ControlCompletionKind::Propagation,
                ControlCompletionKind::Return,
            ],
        )
    }

    fn lowering_fixture_from_parts(
        unit: BoundUnit,
        types: CheckedExpressionTypes,
        selections: CheckedSemanticSelections,
        literals: CheckedLiteralValues,
        values: SemanticValueStore,
        expressions: &[BoundExpressionId],
        completion: impl IntoIterator<Item = ControlCompletionKind>,
    ) -> LoweringFixture {
        let control_flow = CheckedControlFlowFacts::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::from_kinds(completion),
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
