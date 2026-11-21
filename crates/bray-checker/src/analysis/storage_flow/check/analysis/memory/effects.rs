use bray_bound_tree::MemoryReadKind;
use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Effect;
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{BoundExpressionId, CheckedMemoryOperationKind, StorageAccessPurpose};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn memory_mutation(
        &self,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> Option<(
        BoundExpressionId,
        bray_symbols::TypeId,
        StorageAccessPurpose,
    )> {

        let (index, element, purpose) = match *operation.kind() {
            CheckedMemoryOperationKind::Read {
                pointee: element,
                kind: MemoryReadKind::Move,
            }
            | CheckedMemoryOperationKind::VolatileRead {
                pointee: element,
                kind: MemoryReadKind::Move,
                ..
            }
            | CheckedMemoryOperationKind::UninitMove { element }
            | CheckedMemoryOperationKind::UninitAssumeInitialized { element } => {
                (0, element, StorageAccessPurpose::Move)
            }
            CheckedMemoryOperationKind::Write { pointee: element }
            | CheckedMemoryOperationKind::VolatileWrite {
                pointee: element, ..
            }
            | CheckedMemoryOperationKind::UninitWrite { element } => {
                (0, element, StorageAccessPurpose::Initialize)
            }
            CheckedMemoryOperationKind::Copy { pointee, .. } => {
                (1, pointee, StorageAccessPurpose::Initialize)
            }
            CheckedMemoryOperationKind::ByteBufferFill => {
                let element = self
                    .execution
                    .semantics
                    .types()
                    .expression(operation.arguments()[1])
                    .expect("checked byte fill retains its byte value type")
                    .ty();

                (0, element, StorageAccessPurpose::Initialize)
            }
            _ => return None,
        };

        operation
            .arguments()
            .get(index)
            .copied()
            .map(|argument| (argument, element, purpose))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn memory_effects(
        &mut self,
        state: &mut StorageFlowState,
        expression: BoundExpressionId,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> Option<Vec<Effect>> {
        let operation = self.memory.operation(expression)?;

        if crate::memory::execution::storage_effect(operation.kind())
            == crate::memory::execution::StorageEffect::None
        {
            return Some(Vec::new());
        }

        match *operation.kind() {
            CheckedMemoryOperationKind::Copy { .. }
                if execution.admitted_call(expression.into()).is_some_and(|entry| {
                    entry.memory_access == Some(crate::execution_guarantees::MemoryAccessProof::EmptyCopy)
                }) => return Some(Vec::new()),
            CheckedMemoryOperationKind::RawBufferPush { element } => {
                return self.buffer_prefix_effects(
                    state,
                    execution,
                    operation,
                    Some(bray_symbols::DependencyMutationKind::Replace(element)),
                );

            }
            CheckedMemoryOperationKind::RawBufferPop { .. } => {
                return self.buffer_prefix_effects(
                    state,
                    execution,
                    operation,
                    Some(bray_symbols::DependencyMutationKind::Consume),
                );

            }
            CheckedMemoryOperationKind::RawBufferSetInitializedCount => {
                return self.buffer_prefix_effects(state, execution, operation, None);
            }
            CheckedMemoryOperationKind::RawBufferRelocate { .. }
            | CheckedMemoryOperationKind::RawBufferReplace { .. } => {
                return self.buffer_transfer_effects(state, execution, operation);
            }
            CheckedMemoryOperationKind::RawBufferRelease { .. } => {
                return self.buffer_release_effects(state, execution, operation);
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                let ty = if matches!(operation.kind(), CheckedMemoryOperationKind::Deallocate) {
                    self.input.allocation_pointer.expect("allocation field is prepared").1
                } else {
                    self.execution.semantics.types().expression(operation.arguments()[0])
                        .expect("checked deallocation retains its pointer type").ty()
                };

                let kind = bray_symbols::DependencyMutationKind::Retire;

                let targets = self.captured_deallocation_pointer(execution, operation)
                    .and_then(|pointer| pointer.mutation_targets());

                return Some(match targets {
                    Some(targets) => targets.into_iter().map(|target| {
                        let address = target.address.clone();

                        Effect::through(target, kind, Some(ty)).with_retirement_discharges(
                            address.as_ref().into_iter().flat_map(|address| state.memory.retirement_discharges(address)),
                        )
                    }).collect(),
                    None => vec![Effect::unbounded(kind, Some(ty), false)],
                });
            }
            CheckedMemoryOperationKind::InlineAssembly { .. }
            | CheckedMemoryOperationKind::DebuggerTrap
            | CheckedMemoryOperationKind::UnreachableTermination => return None,
            _ => {}
        }

        if matches!(
            operation.kind(),
            CheckedMemoryOperationKind::AtomicStore { .. }
                | CheckedMemoryOperationKind::AtomicExchange { .. }
                | CheckedMemoryOperationKind::AtomicCompareExchange { .. }
                | CheckedMemoryOperationKind::AtomicFetch { .. }
        ) {
            // Atomics update their protected word, never retire its owner or unrelated storage.
            let targets = self.captured_memory_operand(execution, operation, operation.arguments()[0])
                .and_then(bray_bound_tree::StorageCapturedValue::mutation_targets);

            let Some(targets) = targets else {
                return Some(vec![Effect::Atomic]);
            };

            return Some(
                targets
                    .into_iter()
                    .map(|target| {
                        Effect::through(target, bray_symbols::DependencyMutationKind::Atomic, None)
                    })
                    .collect(),
            );
        }

        let (argument, element, purpose) = self.memory_mutation(operation)
            .unwrap_or_else(|| panic!("checked mutating memory operation has no effect model: {:?}", operation.kind()));

        let protected = matches!(
            operation.kind(),
            CheckedMemoryOperationKind::UninitMove { .. }
                | CheckedMemoryOperationKind::UninitAssumeInitialized { .. }
                | CheckedMemoryOperationKind::UninitWrite { .. }
        );

        let kind = if purpose == StorageAccessPurpose::Move {
            bray_symbols::DependencyMutationKind::Consume
        } else {
            bray_symbols::DependencyMutationKind::Replace(element)
        };

        let access_type = (!protected).then(|| self.execution.semantics.types()
            .expression(argument).expect("checked memory access retains its argument type").ty());

        let targets = if protected {
            self.memory_uninit_address(state, execution, operation, argument).and_then(|address| {
                Some(crate::analysis::storage_flow::memory::MutationTarget::reference(Some(&address), address.regions()?))
            })
        } else {
            self.captured_memory_operand(execution, operation, argument)
                .and_then(bray_bound_tree::StorageCapturedValue::mutation_targets)
        };

        Some(match targets {
            Some(targets) => targets.into_iter().map(|target| Effect::through(target, kind, access_type)).collect(),
            None => vec![Effect::unbounded(kind, access_type, false)],
        })
    }
}
