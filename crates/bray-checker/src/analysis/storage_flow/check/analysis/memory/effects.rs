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
        use bray_bound_tree::MemoryReadKind;

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
        &self,
        state: &mut StorageFlowState,
        expression: BoundExpressionId,
    ) -> Option<Vec<Effect>> {
        let operation = self.memory.operation(expression)?;

        if crate::memory::execution::storage_effect(operation.kind())
            == crate::memory::execution::StorageEffect::None
        {
            return Some(Vec::new());
        }

        match *operation.kind() {
            CheckedMemoryOperationKind::RawBufferPush { element } => {
                return self.buffer_prefix_effects(
                    state,
                    operation,
                    0,
                    Some(bray_symbols::DependencyMutationKind::Replace(element)),
                );
            }
            CheckedMemoryOperationKind::RawBufferPop { .. } => {
                return self.buffer_prefix_effects(
                    state,
                    operation,
                    0,
                    Some(bray_symbols::DependencyMutationKind::Consume),
                );
            }
            CheckedMemoryOperationKind::RawBufferSetInitializedCount => {
                return self.buffer_prefix_effects(state, operation, 0, None);
            }
            CheckedMemoryOperationKind::RawBufferRelocate { .. }
            | CheckedMemoryOperationKind::RawBufferReplace { .. } => {
                return self.buffer_transfer_effects(state, operation);
            }
            CheckedMemoryOperationKind::RawBufferRelease { .. } => {
                return self.buffer_release_effects(state, operation);
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                return Some(self.expression_reference(state, operation.arguments()[0])?
                    .regions(&state.memory)?.into_iter().map(|target| {
                        Effect::Mutation(target, bray_symbols::DependencyMutationKind::Retire, None)
                    }).collect());
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
            let targets = self
                .expression_reference(state, operation.arguments()[0])
                .and_then(|source| source.targets(&state.memory));

            let Some(targets) = targets else {
                return Some(vec![Effect::Atomic]);
            };

            return Some(
                targets
                    .into_iter()
                    .map(|target| {
                        Effect::Mutation(target, bray_symbols::DependencyMutationKind::Atomic, None)
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

        let targets = if protected && !self.expression_is_borrowed(argument) {
            vec![self.uninit_target(state, argument)?]
        } else {
            // An unknown element offset still bounds effects to its backing.
            // Bounds grant neither exact value identity nor initialization evidence.
            let targets = self.expression_reference(state, argument)?.regions(&state.memory)?;

            if protected {
                targets.into_iter().map(|target| target.project(&[
                    bray_bound_tree::StorageProjection::UninitPayload
                ])).collect()
            } else {
                targets
            }
        };

        let kind = if purpose == StorageAccessPurpose::Move {
            bray_symbols::DependencyMutationKind::Consume
        } else {
            bray_symbols::DependencyMutationKind::Replace(element)
        };

        Some(
            targets
                .into_iter()
                .map(|target| Effect::Mutation(target, kind, None))
                .collect(),
        )
    }
}
