use bray_bound_tree::{
    BoundExpressionId, CheckedMemoryOperation, CheckedMemoryOperationKind, SelectedArgument,
};
use bray_ir::{
    MirBlockId, MirMemoryOperation, MirOperand, MirOperationKind, MirSourceAnchor,
    MirTerminatorKind,
};
use bray_symbols::{TypeData, TypeId};

use super::assembly::lower_inline_assembly_contract;
use crate::lowering::{LoweringError, block::LoweredExpression, lowerer::Lowerer};

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_memory_call(
        &mut self,
        id: BoundExpressionId,
        mut current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        operation: &CheckedMemoryOperation,
    ) -> Result<LoweredExpression, LoweringError> {
        let kind = operation.kind();

        let (kind, inline_assembly_symbols) = lower_inline_assembly_contract(kind);

        let selected_arguments = ordered_memory_arguments(id, selection, operation);

        let mut arguments = Vec::with_capacity(operation.arguments().len());
        let mut inline_assembly_labels = None;

        for (index, (ordinal, expression, conversion, reborrow)) in
            selected_arguments.iter().enumerate()
        {
            let ordinal = memory_argument_index(id, *ordinal);

            if matches!(
                kind,
                CheckedMemoryOperationKind::InlineAssembly {
                    labels: Some(_),
                    ..
                }
            ) && ordinal == 6
            {
                let (continuation, labels) =
                    self.lower_inline_assembly_labels(*expression, current)?;

                current = continuation;
                inline_assembly_labels = Some(labels);

                continue;
            }

            let Some(runtime_index) = kind.runtime_argument_index(ordinal) else {
                continue;
            };

            let assembly_inputs =
                matches!(kind, CheckedMemoryOperationKind::InlineAssembly { .. }) && ordinal == 5;

            let lowered = if assembly_inputs {
                let CheckedMemoryOperationKind::InlineAssembly { contract, .. } = &kind else {
                    panic!(
                        "lowering contract violation: MissingSemanticSelection {value:?}",
                        value = id
                    );
                };

                self.lower_inline_assembly_inputs(*expression, current, **contract)?
            } else {
                self.lower_call_argument(*expression, *reborrow, current)?
            };

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = *expression
                );
            };

            let (continuation, operand) = self.convert_memory_operand(
                id,
                current,
                &source,
                operand,
                conversion,
                assembly_inputs,
            )?;

            current = continuation;

            let operand_type = if assembly_inputs {
                let CheckedMemoryOperationKind::InlineAssembly { contract, .. } = &kind else {
                    panic!(
                        "lowering contract violation: MissingSemanticSelection {value:?}",
                        value = id
                    );
                };

                self.inline_assembly_runtime_input_type(**contract)?
            } else {
                conversion.target_type()
            };

            let later_expressions =
                selected_arguments[index + 1..]
                    .iter()
                    .filter_map(|(ordinal, expression, _, _)| {
                        usize::try_from(*ordinal)
                            .is_ok_and(|ordinal| is_runtime_expression(&kind, ordinal))
                            .then_some(*expression)
                    });

            let operand = self.materialize_memory_argument(
                current,
                *expression,
                operand,
                operand_type,
                self.later_evaluation_may_change_block(later_expressions)
                    || matches!(kind, CheckedMemoryOperationKind::RawBufferAllocate),
            )?;

            arguments.push((runtime_index, operand, operand_type));
        }

        arguments.sort_unstable_by_key(|(runtime_index, _, _)| *runtime_index);

        if arguments.len() != kind.operand_count()
            || arguments
                .iter()
                .enumerate()
                .any(|(expected, (actual, _, _))| expected != *actual)
        {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = id
            );
        }

        let (operands, operand_types): (Vec<_>, Vec<_>) = arguments
            .into_iter()
            .map(|(_, operand, ty)| (operand, ty))
            .unzip();

        let kind = match (kind, operand_types.first().copied()) {
            (
                CheckedMemoryOperationKind::InlineAssembly {
                    output,
                    labels,
                    contract,
                    ..
                },
                Some(inputs),
            ) => CheckedMemoryOperationKind::InlineAssembly {
                inputs,
                output,
                labels,
                contract,
            },
            (kind, _) => kind,
        };

        let result_type = self.expression_type(id);
        let result = kind.produces_value().then_some(result_type);

        if let Some(lowered) = self.lower_inline_assembly_terminator(
            id,
            current,
            &source,
            &kind,
            &operands,
            inline_assembly_labels,
            &inline_assembly_symbols,
        )? {
            return Ok(lowered);
        }

        let diverges = matches!(
            kind,
            CheckedMemoryOperationKind::CatastrophicAbort
                | CheckedMemoryOperationKind::UnreachableTermination
                | CheckedMemoryOperationKind::InlineAssembly { output: None, .. }
        );

        let admitted_owner =
            matches!(kind, CheckedMemoryOperationKind::RawBufferAllocate).then_some(result_type);

        let check_outcome = kind.requires_checked_outcome()
            && !matches!(
                kind,
                CheckedMemoryOperationKind::RawBufferRelease { .. }
                    | CheckedMemoryOperationKind::RawBufferReplace { .. }
            );

        let memory = MirMemoryOperation::new(kind, operands, operand_types, result)
            .with_inline_assembly_symbols(inline_assembly_symbols);

        if let Some(owner) = admitted_owner {
            current = self.admit_outgoing_owner(id, current, &source, owner)?;
        }

        let (completed, result) = self.push_memory_call(id, current, &source, memory)?;

        current = completed;

        if diverges {
            self.set_terminator(
                current,
                Self::retained_source(&source),
                MirTerminatorKind::Unreachable,
            )?;

            return Ok(LoweredExpression::terminated(source));
        }

        let value = match result {
            Some(value) => MirOperand::Value(value),
            None => self.unit_operand(result_type),
        };

        if check_outcome
        {
            let (completed, value) = self.finish_typed_call_panic_check(
                id,
                current,
                &source,
                &value,
                result_type,
                admitted_owner,
                crate::lowering::expression::CallOutcome::Runtime,
            )?;

            return Ok(LoweredExpression::continuing(
                completed,
                Some(value),
                source,
            ));
        }

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn push_memory_call(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        memory: MirMemoryOperation,
    ) -> Result<(MirBlockId, Option<bray_ir::MirValueId>), LoweringError> {
        if !matches!(
            memory.kind(),
            CheckedMemoryOperationKind::RawBufferRelease { .. }
                | CheckedMemoryOperationKind::RawBufferReplace { .. }
        ) {
            let result = memory.result_type();

            let commit = self.push_operation(
                block,
                Self::retained_source(source),
                MirOperationKind::Memory(Box::new(memory)),
                result,
            )?;

            return Ok((block, commit.result()));
        }

        let mut arguments = Vec::new();

        for (operand, ty) in memory.operands().iter().zip(memory.operand_types()) {
            let storage = self.builder.push_storage(
                Self::retained_source(source),
                bray_ir::MirStorageKind::Temporary,
                *ty,
            )?;

            let place = bray_ir::MirPlace::new(storage, [], *ty);

            self.push_operation(
                block,
                Self::retained_source(source),
                MirOperationKind::Store {
                    kind: bray_ir::MirStoreKind::Initialize,
                    destination: place.clone(),
                    // Preserve each evaluated compact borrow through cleanup suspension.
                    value: operand.clone(),
                },
                None,
            )?;

            arguments.push(place);
        }

        let pointer = arguments[0].ty();

        let TypeData::Borrow { target, .. } = *self.input.semantic_values().type_data(pointer)
        else {
            panic!("checked buffer release has a mutable owner borrow");
        };

        let place = arguments[0].project(bray_ir::MirProjectionKind::Dereference, target);

        let previous_occurrence =
            self.cleanup_occurrence
                .replace(bray_bound_tree::BoundExecutionSite::Cleanup(
                    expression.into(),
                ));

        let outcome = self.create_cleanup_outcome(block, source)?;
        let retention = self.cleanup_retention(source)?;
        let outcome = outcome.with_retention(retention);

        let broadcast = self.builder.push_block(
            Self::retained_source(source),
            bray_ir::MirBlockKind::CleanupBroadcast,
        )?;

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            bray_ir::MirBlockKind::LifecycleResolution,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::BeginCleanup(bray_ir::MirCleanupEdge::new(
                bray_ir::MirCleanupPhase::TaskCancellation,
                bray_ir::MirEdge::new(broadcast, []),
            )),
        )?;

        let mut current = broadcast;

        for (phase, next) in [
            (bray_ir::MirCleanupPhase::TaskCancellation, Some(lifecycle)),
            (bray_ir::MirCleanupPhase::LifecycleResolution, None),
        ] {
            self.push_operation(
                current,
                Self::retained_source(source),
                match phase {
                    bray_ir::MirCleanupPhase::TaskCancellation => MirOperationKind::Cleanup {
                        collection: None,
                        phase,
                        place: place.clone(),
                    },
                    bray_ir::MirCleanupPhase::LifecycleResolution => {
                        MirOperationKind::ReleaseBuffer {
                            collection: None,
                            place: place.clone(),
                        }
                    }
                },
                None,
            )?;

            current = outcome.check(&mut self.builder, current, source, false)?;

            if let Some(next) = next {
                self.set_terminator(
                    current,
                    Self::retained_source(source),
                    MirTerminatorKind::ContinueCleanup(bray_ir::MirCleanupEdge::new(
                        bray_ir::MirCleanupPhase::LifecycleResolution,
                        bray_ir::MirEdge::new(next, []),
                    )),
                )?;

                current = next;
            }
        }

        if matches!(
            memory.kind(),
            CheckedMemoryOperationKind::RawBufferReplace { .. }
        ) {
            self.push_operation(
                current,
                Self::retained_source(source),
                MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                    memory.kind().clone(),
                    arguments.into_iter().map(MirOperand::Copy),
                    memory.operand_types().iter().copied(),
                    None,
                ))),
                None,
            )?;
        }

        let completed = self.finish_replacement_cleanup(expression, current, source, &outcome)?;

        self.cleanup_occurrence = previous_occurrence;

        Ok((completed, None))
    }

    fn convert_memory_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        operand: MirOperand,
        conversion: &bray_bound_tree::SelectedConversion,
        assembly_inputs: bool,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        if assembly_inputs {
            return Ok((current, operand));
        }

        self.convert_operand(
            expression,
            current,
            Self::retained_source(source),
            operand,
            conversion,
        )
    }

    fn materialize_memory_argument(
        &mut self,
        current: MirBlockId,
        expression: BoundExpressionId,
        operand: MirOperand,
        operand_type: TypeId,
        crosses_block_boundary: bool,
    ) -> Result<MirOperand, LoweringError> {
        if !crosses_block_boundary {
            return Ok(operand);
        }

        let lowered = LoweredExpression::continuing(
            current,
            Some(operand),
            self.expression_source(expression),
        );

        Ok(self
            .materialize_typed_for_later_evaluation(expression, lowered, operand_type)?
            .value
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = expression
                )
            }))
    }
}

fn ordered_memory_arguments<'a>(
    id: BoundExpressionId,
    selection: &'a bray_bound_tree::SelectedCall,
    operation: &CheckedMemoryOperation,
) -> Vec<(
    u32,
    BoundExpressionId,
    &'a bray_bound_tree::SelectedConversion,
    Option<bray_symbols::BorrowKind>,
)> {
    let mut selected_arguments = Vec::with_capacity(operation.arguments().len());

    for argument in selection.arguments() {
        let SelectedArgument::Explicit {
            expression,
            ordinal,
            conversion,
            reborrow,
            ..
        } = argument
        else {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = id
            );
        };

        selected_arguments.push((*ordinal, *expression, conversion, *reborrow));
    }

    selected_arguments.sort_unstable_by_key(|(ordinal, _, _, _)| *ordinal);

    if selected_arguments.len() != operation.arguments().len()
        || selected_arguments
            .iter()
            .map(|(_, expression, _, _)| expression)
            .ne(operation.arguments())
    {
        panic!(
            "lowering contract violation: MissingSemanticSelection {value:?}",
            value = id
        );
    }

    selected_arguments
}

fn memory_argument_index(expression: BoundExpressionId, ordinal: u32) -> usize {
    usize::try_from(ordinal).unwrap_or_else(|_| {
        panic!(
            "lowering contract violation: memory argument {ordinal} for {expression:?} must index the host collection"
        )
    })
}

const fn is_runtime_expression(kind: &CheckedMemoryOperationKind, ordinal: usize) -> bool {
    kind.runtime_argument_index(ordinal).is_some()
        || matches!(
            kind,
            CheckedMemoryOperationKind::InlineAssembly {
                labels: Some(_),
                ..
            }
        ) && ordinal == 6
}
