use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Effect, Place, Source, Target};
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

        // Formation binds the descriptor to the allocator's actual target.
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
        let Some((_, fields, source)) = self.buffer_argument(state, operation, 0) else {
            return MemoryOperationStatus::Recovered;
        };

        let Some(owners) = source.regions(&state.memory) else {
            return MemoryOperationStatus::Recovered;
        };

        if let Some(result) = destination {
            let inexact = !source.is_exact(&state.memory);

            // Every view reaches the same allocation of the owner's element type.
            // A slice or spare-capacity view does not redeclare its backing storage.
            state.memory.owned_region(
                &owners,
                Target::storage(result.clone()),
                fields.element,
                bray_bound_tree::StorageProjection::ProductField(fields.pointer),
                inexact,
            );

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
        index: usize,
        transition: Option<bray_symbols::DependencyMutationKind>,
    ) -> Option<Vec<Effect>> {
        use bray_symbols::DependencyMutationKind;

        let (_, fields, source) = self.buffer_argument(state, operation, index)?;

        let owners = source.regions(&state.memory)?;

        let mut effects = Vec::new();

        for owner in owners {
            effects.push(Effect::Mutation(owner.project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.initialized,
                )]), DependencyMutationKind::Write, None));

            if let Some(kind) = transition {
                effects.extend(
                    state
                        .memory
                        .owned_regions(
                            &owner,
                            bray_bound_tree::StorageProjection::ProductField(fields.pointer),
                        )?
                        .into_iter()
                        .map(|target| Effect::Mutation(target, kind, None)),
                );
            }
        }

        Some(effects)
    }

    fn buffer_argument(
        &self,
        state: &StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        index: usize,
    ) -> Option<(
        bray_symbols::TypeId,
        crate::representation::RawBufferFields,
        Source,
    )> {
        let argument = operation.arguments()[index];

        let ty = self.request.semantic_values().unborrowed_type(
            self.execution
                .semantics
                .types()
                .expression(argument)
                .expect("checked buffer operation retains its receiver type")
                .ty(),
        );

        let (fields, _) = self
            .input
            .raw_buffers
            .get(&ty)
            .expect("checked buffer operation prepares its field identities");

        Some((ty, *fields, self.memory_argument_reference(state, operation, argument)?))
    }

    pub(super) fn buffer_transfer_effects(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> Option<Vec<Effect>> {
        use bray_bound_tree::StorageProjection;
        use bray_symbols::DependencyMutationKind;

        let relocate = matches!(
            operation.kind(),
            CheckedMemoryOperationKind::RawBufferRelocate { .. }
        );

        let source_index = usize::from(!relocate);
        let destination_index = usize::from(relocate);

        let (_, fields, source) = self.buffer_argument(state, operation, source_index)?;

        let (buffer_type, _, destination) =
            self.buffer_argument(state, operation, destination_index)?;

        let sources = source.regions(&state.memory)?;
        let destinations = destination.regions(&state.memory)?;
        let exact = source.is_exact(&state.memory) && destination.is_exact(&state.memory);

        let source_regions = sources
            .iter()
            .map(|owner| {
                state
                    .memory
                    .owned_regions(owner, StorageProjection::ProductField(fields.pointer))
            })
            .collect::<Option<Vec<_>>>()?;

        let destination_regions = destinations
            .iter()
            .map(|owner| {
                state
                    .memory
                    .owned_regions(owner, StorageProjection::ProductField(fields.pointer))
            })
            .collect::<Option<Vec<_>>>()?;

        let mut effects = Vec::new();

        if relocate {
            for (owners, regions, kind) in [
                (&sources, &source_regions, DependencyMutationKind::Consume),
                (
                    &destinations,
                    &destination_regions,
                    DependencyMutationKind::Replace(fields.element),
                ),
            ] {
                for owner in owners {
                    effects.push(Effect::Mutation(owner.project(&[StorageProjection::ProductField(fields.initialized)]), DependencyMutationKind::Write, None));
                }

                effects.extend(
                    regions
                        .iter()
                        .flatten()
                        .map(|target| Effect::Mutation(target.clone(), kind, None)),
                );
            }
        } else {
            effects.extend(destinations.iter().map(|target| {
                Effect::Mutation(target.clone(), DependencyMutationKind::Replace(buffer_type), None)
            }));

            effects.extend(
                sources
                    .iter()
                    .map(|target| Effect::Mutation(target.clone(), DependencyMutationKind::Clear, None)),
            );
        }

        // A transfer can publish its payload through the destination, never the
        // address of either buffer descriptor or its enclosing allocation.
        if destinations.iter().any(|target| {
            Effect::Mutation(target.clone(), DependencyMutationKind::Write, None)
                .may_retain_address(&state.memory)
        }) {
            effects.extend(
                source_regions
                    .iter()
                    .flatten()
                    .map(|target| Effect::Mutation(target.clone(), DependencyMutationKind::Escape, None)),
            );
        }

        if exact && let ([source], [destination]) = (sources.as_slice(), destinations.as_slice()) {
            let call = operation.expression().into();

            if relocate {
                if let ([source], [destination]) = (
                    source_regions[0].as_slice(),
                    destination_regions[0].as_slice(),
                ) {
                    state.memory.stage_value(call, source, destination.clone());
                    state.memory.stage_null(call, source.clone());
                }
            } else {
                state.memory.stage_value(call, source, destination.clone());
                state.memory.stage_null(call, source.clone());

                state.memory.stage_null(
                    call,
                    source.project(&[StorageProjection::ProductField(fields.pointer)]),
                );
            }
        }

        Some(effects)
    }

    pub(super) fn buffer_release_effects(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> Option<Vec<Effect>> {
        use bray_symbols::DependencyMutationKind;

        let (_, fields, source) = self.buffer_argument(state, operation, 0)?;

        let owners = source.regions(&state.memory)?;
        let mut effects = Vec::new();

        for owner in &owners {
            effects.push(Effect::Mutation(owner.clone(), DependencyMutationKind::Clear, None));

            effects.extend(
                state
                    .memory
                    .owned_regions(
                        owner,
                        bray_bound_tree::StorageProjection::ProductField(fields.pointer),
                    )?
                    .into_iter()
                    .map(|target| Effect::Mutation(target, DependencyMutationKind::Retire, None)),
            );
        }

        // Element cleanup belongs to the preceding lifecycle operation. This
        // storage release leaves one empty descriptor and no initialized backing.
        if source.targets(&state.memory).is_some()
            && let [owner] = owners.as_slice()
        {
            let call = operation.expression().into();

            state.memory.stage_null(call, owner.clone());

            state.memory.stage_null(
                call,
                owner.project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.pointer,
                )]),
            );
        }

        Some(effects)
    }
}
