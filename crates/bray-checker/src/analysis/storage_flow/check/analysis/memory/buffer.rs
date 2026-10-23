use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Effect, Place, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{CheckedMemoryOperationKind, MemoryOperationStatus};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn retain_allocated_buffer_pointer(
        &self,
        state: &mut StorageFlowState,
        result: &Place,
        allocation: Target,
    ) {
        let ty = self
            .storage
            .storage_type(result.root)
            .expect("checked buffer allocation retains its result type");

        let (fields, _) = self
            .input
            .raw_buffers
            .get(&ty)
            .expect("checked buffer allocation prepares its field identities");

        let pointer = Place {
            root: result.root,
            projections: result
                .projections
                .iter()
                .copied()
                .chain([bray_bound_tree::StorageProjection::ProductField(
                    fields.pointer,
                )])
                .collect(),
        };

        state.memory.point_at(
            allocation,
            pointer,
            fields.element,
            bray_symbols::DependencyReferenceKind::Address,
        );
    }

    pub(super) fn apply_buffer_view(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        destination: Option<Place>,
    ) -> MemoryOperationStatus {
        let Some(source) = self
            .expression_reference(state, operation.arguments()[0])
            .and_then(|source| source.targets(&state.memory))
        else {
            return MemoryOperationStatus::Recovered;
        };

        if let ([owner], Some(result)) = (source.as_slice(), destination) {
            // This closed intrinsic exposes the owned allocation, not the owner's bytes.
            let ty = match operation.kind() {
                CheckedMemoryOperationKind::RawBufferSparePointer { element } => self
                    .request
                    .semantic_values()
                    .intern_type(bray_symbols::TypeData::Slice(element))
                    .expect("checked buffer element has a slice type"),
                CheckedMemoryOperationKind::RawBufferPointer => {
                    crate::representation::reference_target_type(
                        self.request,
                        self.execution
                            .semantics
                            .types()
                            .expression(operation.expression())
                            .expect("checked pointer retains its result type")
                            .ty(),
                    )
                    .expect("checked buffer pointer has its element type")
                }
                _ => self.request.semantic_values().unborrowed_type(
                    self.execution
                        .semantics
                        .types()
                        .expression(operation.expression())
                        .expect("checked buffer view has its result type")
                        .ty(),
                ),
            };

            state.memory.owned_region(owner.clone(), result.clone(), ty);

            if matches!(
                operation.kind(),
                CheckedMemoryOperationKind::RawBufferSparePointer { .. }
            ) {
                // Its initialized prefix has an unknown offset into this allocation.
                state
                    .memory
                    .offset(&Target::storage(result.clone()), result);
            }
        }

        MemoryOperationStatus::Valid
    }

    pub(super) fn buffer_prefix_effects(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        transition: Option<bray_symbols::DependencyMutationKind>,
    ) -> Option<Vec<Effect>> {
        use bray_symbols::DependencyMutationKind;

        let argument = operation.arguments()[0];

        let ty = self.request.semantic_values().unborrowed_type(
            self.execution
                .semantics
                .types()
                .expression(argument)
                .expect("checked buffer prefix operation retains its receiver type")
                .ty(),
        );

        let (fields, _) = self
            .input
            .raw_buffers
            .get(&ty)
            .expect("checked buffer prefix operation prepares its field identities");

        let owners = self
            .expression_reference(state, argument)?
            .targets(&state.memory)?;

        let mut effects = Vec::new();

        for owner in owners {
            effects.push(Effect::Mutation(
                owner.project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.initialized,
                )]),
                DependencyMutationKind::Write,
            ));

            if let Some(kind) = transition {
                effects.extend(
                    state
                        .memory
                        .owned_regions(&owner)?
                        .into_iter()
                        .map(|target| Effect::Mutation(target, kind)),
                );
            }
        }

        Some(effects)
    }
}
