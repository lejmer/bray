use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundCallableBodyKind, BoundNodeOrigin, BoundUnitRoot, StorageAccessId, StorageIdentity,
    StorageIdentityId,
};
use bray_declarations::SyntaxAnchor;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirFrameDescriptor, MirFrameState,
    MirFrameStateId, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor, MirStorageId,
    MirStorageKind, MirTaskTerminalState, MirTerminatorKind, MirUnit, MirUnitBuilder,
};
use bray_runtime_interface::{ProtectedFrameAbiVersions, RuntimeAbiRole};
use bray_symbols::StaticReferenceSelection;

use super::LoweringError;
use super::initialization::InitializationState;
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
    // Cleanup guards can reserve temporary storage before its producer has been evaluated.
    pub(super) initialized_temporaries: BTreeSet<StorageIdentityId>,
    pub(super) guard_bindings: Vec<BTreeMap<StorageIdentityId, MirPlace>>,
    pub(super) owned_targets: BTreeMap<MirStorageId, MirPlace>,
    pub(super) initialization_guards: BTreeMap<MirStorageId, InitializationState<'unit>>,
    pub(super) static_storages: BTreeMap<StaticReferenceSelection, MirStorageId>,
    pub(super) static_accesses: BTreeMap<StorageAccessId, StaticReferenceSelection>,
    pub(super) parameter_positions: BTreeMap<StorageIdentityId, u32>,
    pub(super) active_scopes: Vec<bray_bound_tree::BoundBlockId>,
    pub(super) yield_targets: Vec<YieldTarget>,
    pub(super) loop_targets: Vec<LoopTarget>,
    pub(super) catch_targets: Vec<CatchTarget>,
    pub(super) input_temporaries: Vec<super::inputs::InputTemporary>,
    pub(super) frame_states: Vec<MirFrameState>,
    pub(super) cleanup_outcome: Option<crate::cleanup_outcome::CleanupOutcome>,
    pub(super) cleanup_failure_targets: Option<(MirBlockId, MirBlockId, bray_symbols::TypeId)>,
}

/// Lowers one complete checked semantic unit into validated backend-independent MIR.
pub fn lower_unit(input: LoweringInput<'_>) -> Result<MirUnit, LoweringError> {
    Lowerer::new(input).lower()
}

impl<'unit> Lowerer<'unit> {
    fn new(input: LoweringInput<'unit>) -> Self {
        let builder = input.mir_builder();
        let parameter_positions = parameter_positions(input.storage_plan());

        Self {
            input,
            builder,
            storages: BTreeMap::new(),
            initialized_temporaries: BTreeSet::new(),
            guard_bindings: Vec::new(),
            owned_targets: BTreeMap::new(),
            initialization_guards: BTreeMap::new(),
            static_storages: BTreeMap::new(),
            static_accesses: BTreeMap::new(),
            parameter_positions,
            active_scopes: Vec::new(),
            yield_targets: Vec::new(),
            loop_targets: Vec::new(),
            catch_targets: Vec::new(),
            input_temporaries: Vec::new(),
            frame_states: Vec::new(),
            cleanup_outcome: None,
            cleanup_failure_targets: None,
        }
    }

    fn lower(mut self) -> Result<MirUnit, LoweringError> {
        let root = self.input.unit().root();
        let source = self.source(BoundNodeOrigin::source(self.input.unit().key().source()));

        let entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.initialize_cleanup_guards(entry, &source)?;

        if let Some((reference, ty)) = self.input.static_owner().cloned() {
            self.builder.push_storage(
                Self::retained_source(&source),
                MirStorageKind::Static(reference),
                ty,
            )?;
        }

        if self.input.unit_kind().protected_frame().is_some() {
            self.frame_states.push(
                MirFrameState::new(
                    MirFrameStateId::new(0),
                    entry,
                    self.execution_lane_requirements(),
                    [],
                )
                .with_affinity(self.frame_affinity()),
            );
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
                    .unwrap_or_else(|| panic!("lowering contract violation: MissingBoundNode {value:?}", value = body_id));

                let block = match body.kind() {
                    BoundCallableBodyKind::Block(block) => block,
                    BoundCallableBodyKind::Error(_) => {
                        panic!("lowering contract violation: RecoveredBoundNode {value:?}", value = body_id);
                    }
                };

                self.lower_block(block, entry)?
            }
            BoundUnitRoot::Expression(expression) => self.lower_expression(expression, entry)?,
            BoundUnitRoot::ExpressionSequence(block) => self.lower_block(block, entry)?,
        };

        if let Some(block) = completion.block {
            // Ordinary checking proves required callable results cannot fall through.
            // Conservative cleanup branches can still leave this end block in MIR.
            let required_result = matches!(
                root,
                BoundUnitRoot::CallableBody { .. } | BoundUnitRoot::AnonymousCallable { .. }
            ) && match self.input.expression_types().callable_result_type() {
                Some(result) => {
                    self.type_representation(result)
                        != Some(bray_compiler_known::RepresentationRole::Unit)
                }
                None => false,
            };

            if required_result || !self.builder.is_reachable(entry, block) {
                self.set_terminator(block, completion.source, MirTerminatorKind::Unreachable)?;
            } else if self.input.unit_kind().protected_frame().is_some() {
                let result_type = self
                    .input
                    .expression_types()
                    .callable_result_type()
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: protected-frame unit {:?} has no callable result type",
                            self.input.unit().key()
                        )
                    });

                let value = completion
                    .value
                    .unwrap_or_else(|| self.unit_operand(result_type));

                self.push_operation(
                    block,
                    Self::retained_source(&completion.source),
                    MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                        state: MirTaskTerminalState::Completed(value),
                        runtime: self.runtime_reference(RuntimeAbiRole::TerminalPublication),
                    }),
                    None,
                )?;

                self.set_terminator(block, completion.source, MirTerminatorKind::Return(None))?;
            } else {
                self.set_terminator(
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
                .unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: protected frame {frame:?} in unit {:?} has no callable result type",
                        self.input.unit().key()
                    )
                });

            let runtime_abi = self.input.target().runtime_abi();

            let descriptor = MirFrameDescriptor::new(
                frame,
                runtime_abi,
                ProtectedFrameAbiVersions::uniform(runtime_abi),
                result_type,
                self.frame_states,
            )?;

            self.builder.set_frame_descriptor(descriptor);
        }

        Ok(self.builder.finish(entry))
    }

    pub(super) fn source(&self, origin: BoundNodeOrigin) -> MirSourceAnchor {
        MirSourceAnchor::source(origin)
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

    pub(super) fn guard_binding(&self, identity: StorageIdentityId) -> Option<&MirPlace> {
        self.guard_bindings
            .iter()
            .rev()
            .find_map(|bindings| bindings.get(&identity))
    }
}

fn parameter_positions(plan: &bray_bound_tree::StoragePlan) -> BTreeMap<StorageIdentityId, u32> {
    let entries = plan
        .identity_entries()
        .filter(|(_, identity)| is_parameter_identity(*identity))
        .collect::<Vec<_>>();

    entries
        .iter()
        .copied()
        .filter(|(_, identity)| matches!(identity, StorageIdentity::Receiver(_)))
        .chain(
            entries
                .iter()
                .copied()
                .filter(|(_, identity)| !matches!(identity, StorageIdentity::Receiver(_))),
        )
        .enumerate()
        .map(|(position, (identity, _))| (identity, u32::try_from(position).unwrap_or(u32::MAX)))
        .collect()
}

const fn is_parameter_identity(identity: StorageIdentity) -> bool {
    matches!(
        identity,
        StorageIdentity::Parameter(_)
            | StorageIdentity::Receiver(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::PredicateParameter(_)
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        AnyBoundNodeId, AsyncScopeExitPlan, BoundArgument, BoundBinaryExpression, BoundBlock,
        BoundBlockItem, BoundCallExpression, BoundCallResult, BoundCallableBody,
        BoundCallableTarget, BoundControlTransferExpression, BoundControlTransferKind,
        BoundDependencyContract, BoundErrorExpression, BoundExpression, BoundExpressionId,
        BoundLiteralExpression, BoundLiteralKind, BoundNameExpression, BoundOperator,
        BoundReferenceTarget, BoundResolvedCall, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitRoot,
        CheckedAsync, CheckedBodyBehavior, CheckedControlFlow, CheckedDependencyContracts,
        CheckedExpressionTypes, CheckedLiteralValueEntry, CheckedLiteralValues,
        CheckedMemoryOperation, CheckedMemoryOperationKind, CheckedMemoryOperations,
        CheckedPatterns, CheckedRefinements, CheckedSemanticSelections, ControlCompletion,
        ControlCompletionKind, ExpressionTypeEntry, ExpressionTypeResult, ExpressionTypeStatus,
        InlineAssemblyContract, Liveness, MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind,
        MemoryOffsetUnit, MemoryOperationDecision, MemoryOperationStatus, MemoryOrder,
        MemoryReadKind, OperatorTarget, SelectedArgument, SelectedCall, SelectedConversion,
        SelectedOperation, SelectedPropagation, SelectedPropagationBoundary, SemanticSelection,
        SemanticSelectionEntry, StorageExitDecision, StorageExitPoint, StorageFlow,
        StoragePlanBuilder, VolatileAddressSpace,
    };
    use bray_ir::{
        MirBinaryOperator, MirCallArgument, MirOperationKind, MirTerminatorKind, MirUnitKind,
    };
    use bray_symbols::testing::available_compiler_known_symbols;
    use bray_symbols::{
        CallableAbi, CallableConstness, CallableDefinitionId, CallableDependencyContracts,
        CallableInstanceData, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
        CallablePhaseBehaviors, CallableTrust, CallableTypeData, ConstantValueData,
        ConstantValueKind, CurrentRunCancellation, FunctionSymbolId, GenericOwnerId,
        GenericSubstitutionData, IntegerConstant, SemanticValueStore, SymbolId, TypeData,
    };
    use bray_testing::{test_bound_unit, test_mir_target};

    use super::lower_unit;
    use crate::LoweringInput;

    fn synchronous_callable_unit(
        template: &BoundUnit,
        mut tree: BoundTreeBuilder,
        origin: bray_bound_tree::BoundNodeOrigin,
        block: bray_bound_tree::BoundBlockId,
    ) -> BoundUnit {
        let body = tree
            .push_callable_body(BoundCallableBody::block(origin, block))
            .unwrap_or_else(|error| panic!("test callable body must fit: {error:?}"));

        BoundUnit::new(
            template.key().clone(),
            tree.finish(),
            template.local_symbols().clone(),
            [],
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body,
            },
        )
    }

    fn uniform_expression_types(
        unit: &BoundUnit,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
        result: ExpressionTypeResult,
    ) -> CheckedExpressionTypes {
        CheckedExpressionTypes::new(
            unit.unit(),
            unit.key().kind(),
            expressions
                .into_iter()
                .map(|expression| ExpressionTypeEntry::new(expression, result)),
        )
    }

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
    fn lowering_panics_with_the_missing_checked_expression_identity() {
        let mut fixture = lowering_fixture(101, BoundOperator::Add);
        let missing = fixture.types.entries()[0].expression();

        fixture.types = CheckedExpressionTypes::new(
            fixture.unit.unit(),
            fixture.unit.key().kind(),
            [],
        );

        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = lower_unit(fixture.input());
        }))
        .unwrap_err();

        let message = bray_testing::panic_payload_text(panic.as_ref());

        assert!(message.contains("MissingExpressionType"));
        assert!(message.contains(&format!("{missing:?}")));
    }

    #[test]
    fn only_the_consuming_successor_clears_a_cleanup_guard() {
        use bray_ir::{
            MirBlockKind, MirEdge, MirImmediateValue, MirOperand, MirPlace, MirSourceAnchor,
            MirStorageKind,
        };

        let fixture = lowering_fixture(95, BoundOperator::Add);
        let mut lowerer = super::Lowerer::new(fixture.input());
        let source = MirSourceAnchor::from(fixture.unit.key().source());
        let ty = fixture.values.intern_type(TypeData::tuple([])).unwrap();

        let entry = lowerer
            .builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let consumed = lowerer
            .builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let retained = lowerer
            .builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap();

        let storage = lowerer
            .builder
            .push_storage(source.clone(), MirStorageKind::Local, ty)
            .unwrap();

        let flag = lowerer
            .builder
            .push_storage(source.clone(), MirStorageKind::Local, ty)
            .unwrap();

        lowerer.initialization_guards.insert(
            storage,
            super::super::initialization::InitializationState {
                guard: MirPlace::new(flag, [], ty),
                parts: Vec::new(),
            },
        );

        let parameter = lowerer
            .builder
            .push_block_parameter(consumed, source.clone(), ty)
            .unwrap();

        lowerer
            .set_terminator(
                entry,
                source.clone(),
                MirTerminatorKind::Branch {
                    condition: MirOperand::Immediate {
                        value: MirImmediateValue::Boolean(true),
                        ty,
                    },
                    then_edge: MirEdge::new(
                        consumed,
                        [MirOperand::Move(MirPlace::new(storage, [], ty))],
                    ),
                    else_edge: MirEdge::new(retained, []),
                },
            )
            .unwrap();

        lowerer
            .set_terminator(
                consumed,
                source.clone(),
                MirTerminatorKind::Return(Some(MirOperand::Value(parameter))),
            )
            .unwrap();

        lowerer
            .set_terminator(retained, source, MirTerminatorKind::Return(None))
            .unwrap();

        let mir = lowerer.builder.finish(entry);

        let MirTerminatorKind::Branch {
            then_edge,
            else_edge,
            ..
        } = mir.block(entry).unwrap().terminator().kind()
        else {
            panic!("expected branch")
        };

        assert_ne!(then_edge.target(), consumed);
        assert_eq!(else_edge.target(), retained);
        assert!(mir.block(entry).unwrap().operations().is_empty());
        assert!(mir.block(retained).unwrap().operations().is_empty());
        let bridge = mir.block(then_edge.target()).unwrap();
        assert_eq!(bridge.operations().len(), 1);

        assert!(
            matches!(mir.operation(bridge.operations()[0]).unwrap().kind(), MirOperationKind::Store {
            destination, value: MirOperand::Immediate { value: MirImmediateValue::Boolean(false), .. }, ..
        } if destination.storage() == flag)
        );

        assert!(
            matches!(bridge.terminator().kind(), MirTerminatorKind::Goto(edge) if edge.target() == consumed && matches!(edge.arguments(), [MirOperand::Value(_)]))
        );
    }

    #[test]
    fn lowering_omits_the_false_edge_of_a_constant_true_while_loop() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let ty = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test expression type must intern: {error:?}"));

        let template = test_bound_unit(87);
        let origin = bray_bound_tree::BoundNodeOrigin::source(template.key().source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(87));

        let condition = push_expression(
            &mut tree,
            BoundExpression::Literal(BoundLiteralExpression::new(
                origin,
                template.key().source().syntax().full_range(),
                BoundLiteralKind::Boolean,
                Some(ty),
                false,
            )),
        );

        let body = tree
            .push_block(BoundBlock::new(origin, [], false))
            .unwrap_or_else(|error| panic!("loop body must fit: {error:?}"));

        let while_expression = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::While,
                [condition],
                [body],
                [],
                Some(ty),
                false,
            )),
        );

        let block = tree
            .push_block(BoundBlock::new(
                origin,
                [BoundBlockItem::Expression(while_expression)],
                false,
            ))
            .unwrap_or_else(|error| panic!("test block must fit: {error:?}"));

        let unit = synchronous_callable_unit(&template, tree, origin, block);

        let types = uniform_expression_types(
            &unit,
            [condition, while_expression],
            ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid),
        )
        .with_callable_result_type(ty);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let condition_value = values
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
            .unwrap_or_else(|error| panic!("true value must intern: {error:?}"));

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [CheckedLiteralValueEntry::new(condition, condition_value)],
        )
        .unwrap_or_else(|error| panic!("true literal must validate: {error:?}"));

        let fixture = lowering_fixture_from_parts(
            unit,
            types,
            selections,
            literals,
            values,
            &[condition, while_expression],
            [ControlCompletionKind::Divergence],
        );

        let mir = lower_unit(fixture.input())
            .unwrap_or_else(|error| panic!("constant loop must lower: {error:?}"));

        assert!(
            !mir.blocks()
                .iter()
                .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Branch { .. }))
        );

        assert!(
            mir.blocks()
                .iter()
                .any(|block| matches!(block.terminator().kind(), MirTerminatorKind::Unreachable))
        );
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
                predicate: bray_ir::MirPatternPredicate::NullablePresent,
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
        let (fixture, call_type) = selected_call_fixture(83, true);

        let input = fixture.input();

        let mir = lower_unit(input)
            .unwrap_or_else(|error| panic!("checked selected call must lower: {error:?}"));

        let Some(call) = mir
            .operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                MirOperationKind::Call(call)
                    if matches!(call.target(), bray_ir::MirCallTarget::Direct(_)) =>
                {
                    Some(call)
                }
                _ => None,
            })
        else {
            panic!("lowered unit must contain its selected call");
        };

        let default = mir
            .operations()
            .iter()
            .find_map(|operation| match operation.kind() {
                MirOperationKind::Call(call)
                    if matches!(call.target(), bray_ir::MirCallTarget::DefaultValue { .. }) =>
                {
                    Some(call)
                }
                _ => None,
            })
            .expect("default must be evaluated by its own checked call");

        assert_eq!(default.result(), BoundCallResult::Immediate(call_type));
        assert_eq!(default.arguments().len(), 1);

        assert!(
            mir.operations()
                .iter()
                .any(|operation| matches!(operation.kind(), MirOperationKind::Borrow { .. }))
        );

        assert_eq!(call.target().abi(), CallableAbi::C);
        assert_eq!(call.result(), BoundCallResult::Immediate(call_type));
        assert!(call.phase_behaviors().is_some());

        assert!(matches!(
            call.arguments(),
            [
                MirCallArgument::Explicit { ordinal: 0, .. },
                MirCallArgument::Explicit { ordinal: 1, .. }
            ]
        ));
    }

    #[test]
    fn lowering_materializes_declared_bray_callable_values() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must initialize: {error:?}"));

        let unit_type = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("test unit type must intern: {error:?}"));

        let dependency = values
            .empty_dependency_contract_template()
            .unwrap_or_else(|error| panic!("empty dependency template must exist: {error:?}"));

        let callable_type = values
            .intern_type(TypeData::Callable(CallableTypeData::new(
                [],
                unit_type,
                CallableConstness::Runtime,
                CallableTrust::Safe,
                CallableAbi::Bray,
                CallableDependencyContracts::synchronous(dependency),
            )))
            .unwrap_or_else(|error| panic!("test callable type must intern: {error:?}"));

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(1));
        let template = test_bound_unit(88);
        let origin = bray_bound_tree::BoundNodeOrigin::source(template.key().source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(88));

        let reference = push_expression(
            &mut tree,
            BoundExpression::Name(BoundNameExpression::new(
                origin,
                BoundReferenceTarget::Surface(function.into()),
                Some(callable_type),
                false,
            )),
        );

        let return_expression = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Return,
                Some(reference),
                None,
                Some(callable_type),
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

        let unit = synchronous_callable_unit(&template, tree, origin, block);
        let result = ExpressionTypeResult::new(callable_type, ExpressionTypeStatus::Valid);

        let types = uniform_expression_types(&unit, [reference, return_expression], result)
            .with_callable_result_type(callable_type);

        let selections = CheckedSemanticSelections::try_new(&unit, &types, [])
            .unwrap_or_else(|error| panic!("empty selections must validate: {error:?}"));

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            [],
        )
        .unwrap_or_else(|error| panic!("empty literal values must validate: {error:?}"));

        let fixture = lowering_fixture_from_parts(
            unit,
            types,
            selections,
            literals,
            values,
            &[reference, return_expression],
            [ControlCompletionKind::Return],
        );

        let mir = lower_unit(fixture.input())
            .unwrap_or_else(|error| panic!("declared callable value must lower: {error:?}"));

        assert!(matches!(
            mir.operations()[0].kind(),
            MirOperationKind::DeclaredCallable(reference)
                if reference.instance().definition().symbol() == function.into()
                    && reference.abi() == CallableAbi::Bray
        ));
    }

    #[test]
    fn lowering_replaces_every_checked_memory_family_with_explicit_mir() {
        type Kind = CheckedMemoryOperationKind;
        type MemoryCase = (usize, fn(bray_symbols::TypeId) -> Kind);

        let cases: [MemoryCase; 19] = [
            (1, |ty| Kind::Address {
                kind: MemoryAddressKind::Shared,
                pointee: ty,
            }),
            (1, |ty| Kind::Address {
                kind: MemoryAddressKind::Mutable,
                pointee: ty,
            }),
            (0, |ty| Kind::Null { pointee: ty }),
            (1, |ty| Kind::IsNull { pointee: ty }),
            (2, |ty| Kind::Offset {
                unit: MemoryOffsetUnit::Element,
                pointee: ty,
            }),
            (2, |ty| Kind::Offset {
                unit: MemoryOffsetUnit::Byte,
                pointee: ty,
            }),
            (1, |ty| Kind::Reinterpret {
                source: ty,
                target: ty,
            }),
            (1, |ty| Kind::Read {
                pointee: ty,
                kind: MemoryReadKind::Copy,
            }),
            (2, |ty| Kind::Write { pointee: ty }),
            (3, |ty| Kind::Copy {
                pointee: ty,
                kind: MemoryCopyKind::NonOverlapping,
            }),
            (3, |ty| Kind::Copy {
                pointee: ty,
                kind: MemoryCopyKind::Overlapping,
            }),
            (0, |ty| Kind::LayoutQuery {
                ty,
                kind: MemoryLayoutQueryKind::Size,
            }),
            (0, |ty| Kind::LayoutQuery {
                ty,
                kind: MemoryLayoutQueryKind::Alignment,
            }),
            (0, |ty| Kind::LayoutQuery {
                ty,
                kind: MemoryLayoutQueryKind::Stride,
            }),
            (1, |ty| Kind::LayoutQuery {
                ty,
                kind: MemoryLayoutQueryKind::Layout,
            }),
            (2, |_| Kind::RawAllocate),
            (3, |_| Kind::RawDeallocate),
            (1, |_| Kind::Allocate),
            (1, |_| Kind::Deallocate),
        ];

        for (index, (argument_count, kind)) in cases.into_iter().enumerate() {
            let unit_id = 84_u32
                .checked_add(
                    u32::try_from(index).unwrap_or_else(|_| panic!("test case index must fit")),
                )
                .unwrap_or_else(|| panic!("test unit ID must fit"));

            let (mut fixture, pointee) = call_fixture(unit_id, argument_count, false);

            let [entry] = fixture.selections.entries() else {
                panic!("selected call fixture must contain one selection");
            };

            let expression = entry.expression();

            let SemanticSelection::Call(call) = entry.selection() else {
                panic!("selected call fixture must contain a call");
            };

            let arguments = call
                .arguments()
                .iter()
                .map(|argument| match argument {
                    SelectedArgument::Explicit { expression, .. } => *expression,
                    SelectedArgument::Default { .. } => {
                        panic!("memory fixture must contain only explicit arguments")
                    }
                })
                .collect::<Vec<_>>();

            let kind = kind(pointee);

            let operations = CheckedMemoryOperations::try_new(
                fixture.unit.unit(),
                fixture.unit.key().kind(),
                [CheckedMemoryOperation::new(expression, kind, arguments)],
                false,
            )
            .unwrap_or_else(|error| panic!("checked memory operation must validate: {error:?}"));

            fixture.storage_flow = fixture
                .storage_flow
                .with_memory_operations(
                    &operations,
                    [MemoryOperationDecision::new(
                        expression,
                        MemoryOperationStatus::Valid,
                    )],
                )
                .unwrap_or_else(|error| panic!("memory flow must validate: {error:?}"));

            let mir = lower_unit(fixture.input())
                .unwrap_or_else(|error| panic!("checked memory call must lower: {error:?}"));

            assert!(matches!(
                mir.operations()[0].kind(),
                MirOperationKind::Memory(operation)
                    if operation.kind() == kind
                        && operation.operands().len() == argument_count
                        && operation.result_type().is_some() == kind.produces_value()
            ));
        }
    }

    #[test]
    fn target_control_runtime_arguments_are_selected_before_lowering() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test target-control type must intern: {error:?}"));

        let constant = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("test target-control constant must intern: {error:?}"));

        let contract = InlineAssemblyContract::try_new(
            constant,
            constant,
            constant,
            constant,
            constant,
            [None; bray_bound_tree::MAX_INLINE_ASSEMBLY_OPERANDS],
            0,
            "",
            "",
        )
        .unwrap_or_else(|| panic!("test target-control contract must validate"));

        let fence = CheckedMemoryOperationKind::Fence {
            compiler_only: false,
            order: MemoryOrder::AcquireRelease,
        };

        let feature = CheckedMemoryOperationKind::TargetFeatureEnabled { feature: constant };

        let assembly = CheckedMemoryOperationKind::InlineAssembly {
            inputs: ty,
            output: Some(ty),
            labels: None,
            contract,
        };

        let branching = CheckedMemoryOperationKind::InlineAssembly {
            inputs: ty,
            output: Some(ty),
            labels: Some(ty),
            contract,
        };

        let volatile = CheckedMemoryOperationKind::VolatileRead {
            pointee: ty,
            address_space: VolatileAddressSpace::Host,
            kind: MemoryReadKind::Copy,
        };

        let address = CheckedMemoryOperationKind::ExposeAddress { pointee: ty };

        assert_eq!(fence.operand_count(), 0);
        assert_eq!(fence.runtime_argument_index(0), None);
        assert_eq!(feature.operand_count(), 0);
        assert_eq!(feature.runtime_argument_index(0), None);
        assert_eq!(assembly.operand_count(), 1);

        assert_eq!(
            (0..6)
                .map(|ordinal| assembly.runtime_argument_index(ordinal))
                .collect::<Vec<_>>(),
            [None, None, None, None, None, Some(0)]
        );

        assert_eq!(branching.operand_count(), 1);

        assert_eq!(
            (0..7)
                .map(|ordinal| branching.runtime_argument_index(ordinal))
                .collect::<Vec<_>>(),
            [None, None, None, None, None, Some(0), None]
        );

        assert_eq!(volatile.runtime_argument_index(0), Some(0));
        assert_eq!(address.runtime_argument_index(0), Some(0));
    }

    struct LoweringFixture {
        unit: BoundUnit,
        control_flow: CheckedControlFlow,
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

    impl LoweringFixture {
        fn input(&self) -> LoweringInput<'_> {
            let target = test_mir_target();

            LoweringInput::new(
                &self.unit,
                &self.control_flow,
                &self.types,
                &self.patterns,
                &self.literals,
                &self.refinements,
                &self.storage,
                &self.liveness,
                &self.storage_flow,
                &self.dependencies,
                &self.selections,
                available_compiler_known_symbols(),
                &self.async_analysis,
                std::collections::BTreeSet::new(),
                &self.behavior,
                &self.values,
                &[],
                MirUnitKind::Synchronous,
                target,
            )
        }
    }

    fn selected_call_fixture(
        unit_id: u32,
        include_default: bool,
    ) -> (LoweringFixture, bray_symbols::TypeId) {
        call_fixture(unit_id, 1, include_default)
    }

    fn call_fixture(
        unit_id: u32,
        argument_count: usize,
        include_default: bool,
    ) -> (LoweringFixture, bray_symbols::TypeId) {
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

        let arguments = (0..argument_count)
            .map(|_| {
                push_expression(
                    &mut tree,
                    BoundExpression::Literal(BoundLiteralExpression::new(
                        origin,
                        template.key().source().syntax().full_range(),
                        BoundLiteralKind::Integer,
                        Some(ty),
                        false,
                    )),
                )
            })
            .collect::<Vec<_>>();

        let call = push_expression(
            &mut tree,
            BoundExpression::Call(BoundCallExpression::resolved(
                origin,
                callee,
                [],
                arguments
                    .iter()
                    .copied()
                    .map(|argument| BoundArgument::new(argument, None, false)),
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

        let unit = synchronous_callable_unit(&template, tree, origin, block);

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);

        let expressions = [callee]
            .into_iter()
            .chain(arguments.iter().copied())
            .chain([call, return_expression])
            .collect::<Vec<_>>();

        let types = uniform_expression_types(&unit, expressions.iter().copied(), result);

        let dependency = values
            .empty_dependency_contract_template()
            .unwrap_or_else(|error| panic!("empty dependency template must exist: {error:?}"));

        let mut selected_arguments = arguments
            .iter()
            .copied()
            .enumerate()
            .map(|(ordinal, expression)| {
                let ordinal = u32::try_from(ordinal)
                    .unwrap_or_else(|_| panic!("test parameter ordinal must fit"));

                SelectedArgument::Explicit {
                    expression,
                    parameter: Some(CallableParameterSymbolId::from_symbol_id(SymbolId::new(
                        ordinal + 2,
                    ))),
                    ordinal,
                    conversion: SelectedConversion::new(
                        ty,
                        ty,
                        bray_bound_tree::ConversionTarget::Identity,
                    ),
                }
            })
            .collect::<Vec<_>>();

        if include_default {
            let ordinal = u32::try_from(argument_count)
                .unwrap_or_else(|_| panic!("test parameter ordinal must fit"));

            selected_arguments.push(SelectedArgument::Default {
                parameter: CallableParameterSymbolId::from_symbol_id(SymbolId::new(3)),
                ordinal,
                provider: CallableParameterDefaultProviderSymbolId::from_symbol_id(SymbolId::new(
                    4,
                )),
                ty,
            });
        }

        let selected = SelectedCall::new(
            resolution,
            CallableAbi::C,
            CallablePhaseBehaviors::empty(CallableDependencyContracts::synchronous(dependency)),
            None,
            selected_arguments,
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

        let literals = CheckedLiteralValues::try_new(
            &unit,
            &types,
            &values,
            test_mir_target().machine().pointer_width_bits(),
            arguments
                .iter()
                .copied()
                .enumerate()
                .map(|(index, argument)| {
                    CheckedLiteralValueEntry::new(
                        argument,
                        constant_value(
                            &values,
                            ty,
                            u64::try_from(index + 1)
                                .unwrap_or_else(|_| panic!("test literal value must fit")),
                        ),
                    )
                }),
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

        let unit = synchronous_callable_unit(&template, tree, origin, block);

        let result = ExpressionTypeResult::new(ty, ExpressionTypeStatus::Valid);
        let expressions = [left, right, binary, return_expression];

        let types = uniform_expression_types(&unit, expressions, result);

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

        let unit = synchronous_callable_unit(&template, tree, origin, block);

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
        let control_flow = CheckedControlFlow::new(
            unit.unit(),
            unit.key().kind(),
            ControlCompletion::from_kinds(completion),
        );

        let patterns = CheckedPatterns::new(unit.unit(), unit.key().kind(), [], [], []);
        let storage = StoragePlanBuilder::new(unit.unit(), unit.key().kind()).finish();

        let exit_nodes = unit
            .tree()
            .expressions()
            .map(|(expression, _)| AnyBoundNodeId::Expression(expression))
            .chain(
                unit.tree()
                    .blocks()
                    .map(|(block, _)| AnyBoundNodeId::Block(block)),
            )
            .filter(|node| unit.view().node_is_recovered(*node) == Some(false))
            .collect::<Vec<_>>();

        let scope_exits = unit
            .tree()
            .blocks()
            .flat_map(|(scope, _)| exit_nodes.iter().copied().map(move |exit| (scope, exit)))
            .collect::<Vec<_>>();

        let storage_flow = StorageFlow::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [],
            scope_exits
                .iter()
                .map(|(scope, exit)| StorageExitPoint::new(*scope, *exit)),
            scope_exits.iter().map(|(scope, exit)| {
                StorageExitDecision::new(*scope, *exit, [], [], [], [], [], [], false)
            }),
            false,
        )
        .unwrap_or_else(|error| panic!("empty storage flow must validate: {error:?}"));

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], false)
            .unwrap_or_else(|error| panic!("empty liveness must validate: {error:?}"));

        let refinements = CheckedRefinements::try_new(unit.unit(), unit.key().kind(), [], false)
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
            [],
            false,
        )
        .unwrap_or_else(|error| panic!("empty dependencies must validate: {error:?}"));

        let async_analysis = CheckedAsync::try_new(
            unit.unit(),
            unit.key().kind(),
            [],
            [],
            [],
            [],
            types
                .entries()
                .iter()
                .map(|entry| entry.result().ty())
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .map(|ty| {
                    bray_bound_tree::StorageCleanupType::new(
                        ty,
                        bray_bound_tree::AsyncStorageCleanupRequirement::None,
                    )
                }),
            scope_exits
                .iter()
                .map(|(scope, exit)| AsyncScopeExitPlan::new(*scope, *exit, [], [], [], [], false)),
            false,
        )
        .unwrap_or_else(|error| panic!("empty async analysis must validate: {error:?}"));

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
            async_analysis,
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
