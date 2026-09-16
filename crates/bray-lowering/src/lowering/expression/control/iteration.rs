use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundForExpression, BoundGeneratorExpression,
    BoundStructuredExpression, BoundStructuredExpressionKind, IterationSourceMode, SelectedCall,
    SemanticSelection, StorageAccessPurpose, StorageIdentity,
};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallTarget, MirCallableReference, MirEdge,
    MirGeneratorKind, MirGeneratorOperation, MirImmediateValue, MirOperand, MirOperationKind,
    MirPlace, MirStorageKind, MirStoreKind, MirTerminatorKind,
};
use bray_symbols::{BorrowKind, CallableAbi, ReceiverMode};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::{LoopTarget, Lowerer, YieldTarget};

struct Iteration {
    header: MirBlockId,
    item: MirBlockId,
    exhausted: MirBlockId,
    element: MirPlace,
    source: bray_ir::MirSourceAnchor,
}
impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_range_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        selection: &SelectedCall,
    ) -> Result<Option<LoweredExpression>, LoweringError> {
        match selection.implementation_hook() {
            hook if is_range_iterate_hook(hook) => {
                let receiver = selection
                    .receiver()
                    .unwrap_or_else(|| panic!("lowering contract violation: MissingSemanticSelection {value:?}", value = id));

                let mode = match receiver.mode() {
                    ReceiverMode::Shared => IterationSourceMode::Shared,
                    ReceiverMode::Mutable => IterationSourceMode::Mutable,
                    ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => {
                        IterationSourceMode::Move
                    }
                };

                self.lower_range_iteration_source(receiver.expression(), mode, current)
                    .map(Some)
            }
            Some(ImplementationHook::RangeNext) => {
                self.lower_range_next_call(id, current, selection).map(Some)
            }
            _ => Ok(None),
        }
    }

    pub(in crate::lowering::expression) fn lower_for(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundForExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let iteration = self.begin_iteration(id, expression.source(), current)?;

        self.push_loop_target(
            expression.origin().source_anchor().syntax(),
            iteration.header,
            join,
            result_type,
        );

        let pattern_subject = self.pattern_place_operand(
            expression.pattern(),
            Self::retained_place(&iteration.element),
            iteration.item,
        )?;

        let item =
            self.lower_pattern_bindings(expression.pattern(), pattern_subject, iteration.item)?;

        let body = self.lower_block(expression.body(), item)?;
        self.finish_edge(body, iteration.header)?;

        self.finish_iteration_else(
            expression.else_body(),
            iteration.exhausted,
            join,
            result_type,
            &iteration.source,
        )?;

        self.loop_targets.pop();

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            iteration.source,
        ))
    }

    pub(super) fn lower_boolean_fold(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [source_expression] = expression.operands() else {
            panic!("lowering contract violation: UnsupportedExpression {value:?}", value = id);
        };

        let iteration = self.begin_iteration(id, *source_expression, current)?;

        let join = self.builder.push_block(
            Self::retained_source(&iteration.source),
            MirBlockKind::Ordinary,
        )?;

        let result_type = self.expression_type(id);

        let result = self.builder.push_block_parameter(
            join,
            Self::retained_source(&iteration.source),
            result_type,
        )?;

        let (then_edge, else_edge) = match expression.kind() {
            BoundStructuredExpressionKind::BooleanAllFold => (
                MirEdge::new(iteration.header, []),
                MirEdge::new(join, [self.boolean_operand(result_type, false)]),
            ),
            BoundStructuredExpressionKind::BooleanAnyFold => (
                MirEdge::new(join, [self.boolean_operand(result_type, true)]),
                MirEdge::new(iteration.header, []),
            ),
            _ => panic!("lowering contract violation: UnsupportedExpression {value:?}", value = id),
        };

        self.set_terminator(
            iteration.item,
            Self::retained_source(&iteration.source),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(iteration.element),
                then_edge,
                else_edge,
            },
        )?;

        self.set_terminator(
            iteration.exhausted,
            Self::retained_source(&iteration.source),
            MirTerminatorKind::Goto(MirEdge::new(
                join,
                [self.boolean_operand(
                    result_type,
                    expression.kind() == BoundStructuredExpressionKind::BooleanAllFold,
                )],
            )),
        )?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            iteration.source,
        ))
    }

    pub(super) fn lower_generator(
        &mut self,
        result_id: BoundExpressionId,
        iteration_id: BoundExpressionId,
        expression: &BoundGeneratorExpression,
        current: MirBlockId,
        kind: MirGeneratorKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.source(expression.origin());
        let result_type = self.expression_type(result_id);

        let destination_storage = self.builder.push_storage(
            Self::retained_source(&source),
            MirStorageKind::Temporary,
            result_type,
        )?;

        let destination = MirPlace::new(destination_storage, [], result_type);

        // Lowering mutates the MIR builder after consulting this immutable checked selection.
        let selection = self.iteration_selection(iteration_id).clone();

        self.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Generator(MirGeneratorOperation::Begin {
                kind,
                destination: Self::retained_place(&destination),
                element: selection.element_type(),
                exact_count: selection.exact_count(),
            }),
            None,
        )?;

        let result_syntax = self
            .input
            .unit()
            .view()
            .expression(result_id)
            .map(BoundExpression::origin)
            .unwrap_or_else(|| panic!("lowering contract violation: MissingBoundNode {value:?}", value = result_id))
            .source_anchor()
            .syntax();

        self.yield_targets.push(YieldTarget::Generator {
            syntax: result_syntax,
            destination: Self::retained_place(&destination),
            element_type: selection.element_type(),
        });

        let iteration = self.lower_generator_iteration_loop(iteration_id, expression, current)?;

        self.yield_targets.pop();

        let value = self.push_value_operation(
            result_id,
            iteration.exhausted,
            Self::retained_source(&source),
            MirOperationKind::Generator(MirGeneratorOperation::Finish { destination }),
        )?;

        Ok(LoweredExpression::continuing(
            iteration.exhausted,
            Some(value),
            source,
        ))
    }

    pub(in crate::lowering::expression) fn lower_generator_iteration(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundGeneratorExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.source(expression.origin());
        let iteration = self.lower_generator_iteration_loop(id, expression, current)?;
        let result_type = self.expression_type(id);

        Ok(LoweredExpression::continuing(
            iteration.exhausted,
            Some(self.unit_operand(result_type)),
            source,
        ))
    }

    pub(super) fn lower_generator_region(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
        kind: MirGeneratorKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let [iteration_id] = expression.operands() else {
            panic!("lowering contract violation: UnsupportedExpression {value:?}", value = id);
        };

        let Some(BoundExpression::Generator(iteration)) =
            self.input.unit().view().expression(*iteration_id)
        else {
            panic!("lowering contract violation: UnsupportedExpression {value:?}", value = id);
        };

        let iteration = *iteration;

        self.lower_generator(id, *iteration_id, &iteration, current, kind)
    }

    fn lower_generator_iteration_loop(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundGeneratorExpression,
        current: MirBlockId,
    ) -> Result<Iteration, LoweringError> {
        let iteration = self.begin_iteration(id, expression.source(), current)?;
        let iteration_type = self.expression_type(id);

        let break_block = self.builder.push_block(
            Self::retained_source(&iteration.source),
            MirBlockKind::Ordinary,
        )?;

        self.builder.push_block_parameter(
            break_block,
            Self::retained_source(&iteration.source),
            iteration_type,
        )?;

        self.set_terminator(
            break_block,
            Self::retained_source(&iteration.source),
            MirTerminatorKind::Goto(MirEdge::new(iteration.exhausted, [])),
        )?;

        self.loop_targets.push(LoopTarget {
            syntax: expression.region(),
            continue_block: iteration.header,
            break_block,
            result_type: iteration_type,
            scope_depth: self.active_scopes.len(),
        });

        let pattern_subject = self.pattern_place_operand(
            expression.pattern(),
            Self::retained_place(&iteration.element),
            iteration.item,
        )?;

        let item =
            self.lower_pattern_bindings(expression.pattern(), pattern_subject, iteration.item)?;

        let body = self.lower_block(expression.body(), item)?;
        self.finish_edge(body, iteration.header)?;

        self.loop_targets.pop();

        Ok(iteration)
    }

    fn begin_iteration(
        &mut self,
        id: BoundExpressionId,
        source_expression: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<Iteration, LoweringError> {
        // Lowering mutates the MIR builder after consulting this immutable checked selection.
        let selection = self.iteration_selection(id).clone();
        let source = self.expression_source(id);
        let range_cursor = self.is_range_type(selection.cursor_type());

        let direct_range_source = is_range_iterate_hook(
            self.input
                .available_compiler_known_symbols()
                .symbol_implementation(selection.iterate().definition().symbol()),
        );

        let source_value = if direct_range_source {
            self.lower_range_iteration_source(source_expression, selection.mode(), current)?
        } else {
            self.lower_iteration_source(
                source_expression,
                selection.mode(),
                selection.source_type(),
                current,
            )?
        };

        let Some(mut current) = source_value.block else {
            panic!("lowering contract violation: UnsupportedExpression {value:?}", value = id);
        };

        let Some(source_operand) = source_value.value else {
            panic!("lowering contract violation: MissingOperationResult {value:?}", value = source_expression);
        };

        let cursor_value = if direct_range_source {
            source_operand
        } else {
            let (continuation, cursor_value) = self.push_checked_call(
                id,
                current,
                Self::retained_source(&source),
                MirCall::protocol(
                    MirCallTarget::Direct(MirCallableReference::new(
                        selection.iterate(),
                        CallableAbi::Bray,
                    )),
                    bray_bound_tree::BoundCallResult::Immediate(selection.cursor_type()),
                    [source_operand],
                    [bray_bound_tree::SelectedImplementationWitness::new(
                        selection.iterable_requirement(),
                        selection.iterable_witness(),
                    )],
                ),
                selection.cursor_type(),
            )?;

            current = continuation;

            cursor_value
        };

        let cursor = self.iteration_place(
            id,
            StorageIdentity::IterationCursor(id),
            selection.cursor_type(),
        )?;

        let element = self.iteration_place(
            id,
            StorageIdentity::IterationElement(id),
            selection.element_type(),
        )?;

        self.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&cursor),
                value: cursor_value,
            },
            None,
        )?;

        let header = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let item = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let exhausted = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let item_value = self.builder.push_block_parameter(
            item,
            Self::retained_source(&source),
            selection.element_type(),
        )?;

        self.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(header, [])),
        )?;

        let terminator = if range_cursor {
            MirTerminatorKind::RangeIterate {
                cursor: Self::retained_place(&cursor),
                element_type: selection.element_type(),
                item,
                exhausted: MirEdge::new(exhausted, []),
            }
        } else {
            MirTerminatorKind::Iterate {
                cursor: Self::retained_place(&cursor),
                next: MirCallableReference::new(selection.next(), CallableAbi::Bray),
                witness: selection.iterator_witness(),
                element_type: selection.element_type(),
                item,
                exhausted: MirEdge::new(exhausted, []),
            }
        };

        self.set_terminator(header, Self::retained_source(&source), terminator)?;

        self.push_operation(
            item,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: Self::retained_place(&element),
                value: MirOperand::Value(item_value),
            },
            None,
        )?;

        Ok(Iteration {
            header,
            item,
            exhausted,
            element,
            source,
        })
    }

    fn lower_iteration_source(
        &mut self,
        expression: BoundExpressionId,
        mode: IterationSourceMode,
        ty: bray_symbols::TypeId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let kind = match mode {
            IterationSourceMode::Shared => Some(BorrowKind::Shared),
            IterationSourceMode::Mutable => Some(BorrowKind::Mutable),
            IterationSourceMode::Move => None,
        };

        let Some(kind) = kind else {
            return self.lower_expression(expression, current);
        };

        let decision = self.storage_decision(expression, |purpose| {
            purpose == StorageAccessPurpose::Borrow(kind)
        });

        self.lower_access_place_with(
            expression,
            decision.access(),
            current,
            |lowerer, current, place| {
                let source = lowerer.expression_source(expression);

                let commit = lowerer.push_operation(
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Borrow { kind, place },
                    Some(ty),
                )?;

                let value = commit
                    .result()
                    .map(MirOperand::Value)
                    .unwrap_or_else(|| panic!("lowering contract violation: MissingOperationResult {value:?}", value = expression));

                Ok(LoweredExpression::continuing(current, Some(value), source))
            },
        )
    }

    fn lower_range_iteration_source(
        &mut self,
        expression: BoundExpressionId,
        mode: IterationSourceMode,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        if mode == IterationSourceMode::Move {
            return self.lower_expression(expression, current);
        }

        let kind = match mode {
            IterationSourceMode::Shared => BorrowKind::Shared,
            IterationSourceMode::Mutable => BorrowKind::Mutable,
            IterationSourceMode::Move => return self.lower_expression(expression, current),
        };

        let decision = self.storage_decision(expression, |purpose| {
            purpose == StorageAccessPurpose::Borrow(kind)
        });

        self.lower_materialized_access_place_with(
            expression,
            decision.access(),
            current,
            |lowerer, current, place| {
                let source = lowerer.expression_source(expression);

                Ok(LoweredExpression::continuing(
                    current,
                    Some(MirOperand::Copy(place)),
                    source,
                ))
            },
        )
    }

    fn lower_range_next_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        selection: &SelectedCall,
    ) -> Result<LoweredExpression, LoweringError> {
        let receiver = selection
            .receiver()
            .unwrap_or_else(|| panic!("lowering contract violation: MissingSemanticSelection {value:?}", value = id));

        let decision = self.storage_decision(receiver.expression(), |purpose| {
            purpose == StorageAccessPurpose::Borrow(BorrowKind::Mutable)
        });

        self.lower_access_place_with(
            receiver.expression(),
            decision.access(),
            current,
            |lowerer, current, cursor| {
                lowerer.lower_range_next_from_cursor(id, current, cursor, receiver.target_type())
            },
        )
    }

    fn lower_range_next_from_cursor(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        cursor: MirPlace,
        cursor_type: bray_symbols::TypeId,
    ) -> Result<LoweredExpression, LoweringError> {
        let element_type = self
            .input
            .available_compiler_known_symbols()
            .unary_representation_argument(
                self.input.semantic_values(),
                RepresentationRole::Range,
                cursor_type,
            )
            .unwrap_or_else(|| panic!("lowering contract violation: MissingSemanticSelection {value:?}", value = id));

        let result_type = self.expression_type(id);
        let source = self.expression_source(id);

        let item = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let exhausted = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let item_value = self.builder.push_block_parameter(
            item,
            Self::retained_source(&source),
            element_type,
        )?;

        let result =
            self.builder
                .push_block_parameter(join, Self::retained_source(&source), result_type)?;

        self.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::RangeIterate {
                cursor,
                element_type,
                item,
                exhausted: MirEdge::new(exhausted, []),
            },
        )?;

        let present = self.push_nullable_present(
            id,
            item,
            Self::retained_source(&source),
            MirOperand::Value(item_value),
            result_type,
        )?;

        self.set_terminator(
            item,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(join, [present])),
        )?;

        let absent = Self::immediate_operand(result_type, MirImmediateValue::NullableAbsent);

        self.set_terminator(
            exhausted,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(join, [absent])),
        )?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    fn is_range_type(&self, ty: bray_symbols::TypeId) -> bool {
        let data = self.input.semantic_values().type_data(ty);

        let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
            return false;
        };

        let role = match definition {
            bray_symbols::NamedTypeSymbolId::Struct(definition) => self
                .input
                .available_compiler_known_symbols()
                .symbol_representation(*definition),
            bray_symbols::NamedTypeSymbolId::Union(definition) => self
                .input
                .available_compiler_known_symbols()
                .symbol_representation(*definition),
        };

        role == Some(RepresentationRole::Range)
    }

    fn iteration_selection(
        &self,
        expression: BoundExpressionId,
    ) -> &bray_bound_tree::SelectedIterationSource {
        self.input
            .semantic_selections()
            .expression(expression)
            .and_then(|selection| match selection {
                SemanticSelection::Iteration(iteration) => Some(iteration),
                _ => None,
            })
            .unwrap_or_else(|| panic!("lowering contract violation: MissingSemanticSelection {value:?}", value = expression))
    }

    fn iteration_place(
        &mut self,
        expression: BoundExpressionId,
        identity: StorageIdentity,
        ty: bray_symbols::TypeId,
    ) -> Result<MirPlace, LoweringError> {
        let (id, _) = self
            .input
            .storage_plan()
            .identity_entries()
            .find(|(_, candidate)| *candidate == identity)
            .unwrap_or_else(|| panic!("lowering contract violation: MissingIterationStorage {value:?}", value = expression));

        let origin = self
            .input
            .unit()
            .view()
            .expression(expression)
            .map(BoundExpression::origin)
            .unwrap_or_else(|| panic!("lowering contract violation: MissingBoundNode {value:?}", value = expression));

        self.place_for_identity(id, ty, origin)
    }

    fn push_loop_target(
        &mut self,
        syntax: bray_declarations::SyntaxAnchor,
        continue_block: MirBlockId,
        break_block: MirBlockId,
        result_type: bray_symbols::TypeId,
    ) {
        self.loop_targets.push(LoopTarget {
            syntax,
            continue_block,
            break_block,
            result_type,
            scope_depth: self.active_scopes.len(),
        });
    }

    fn finish_iteration_else(
        &mut self,
        else_body: Option<bray_bound_tree::BoundBlockId>,
        exhausted: MirBlockId,
        join: MirBlockId,
        result_type: bray_symbols::TypeId,
        source: &bray_ir::MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        let completion = match else_body {
            Some(body) => self.lower_block(body, exhausted)?,
            None => LoweredExpression::continuing(
                exhausted,
                Some(self.unit_operand(result_type)),
                Self::retained_source(source),
            ),
        };

        self.finish_result_edge(completion, join, result_type)
    }

    pub(in crate::lowering) fn boolean_operand(
        &self,
        ty: bray_symbols::TypeId,
        value: bool,
    ) -> MirOperand {
        MirOperand::Immediate {
            value: MirImmediateValue::Boolean(value),
            ty,
        }
    }
}

const fn is_range_iterate_hook(hook: Option<ImplementationHook>) -> bool {
    matches!(
        hook,
        Some(ImplementationHook::RangeSharedIterate | ImplementationHook::RangeMoveIterate)
    )
}
