use bray_symbols::DependencyMutationKind;
use bray_bound_tree::StorageProjection;
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

        let fields = self
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
        if self.input.footprint.complete_reference_types.get(&fields.element) != Some(&true) {
            state.memory.seed_empty_whole(allocation.clone());
            state.memory.seed_empty_whole(allocation.project(&[bray_bound_tree::StorageProjection::AnyElement]));
        }

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

        let (_, fields, source) = self.buffer_argument(state, operation, index)?;

        let owners = source.mutation_targets(&state.memory)?;

        let mut effects = Vec::new();

        // Prefix metadata and backing effects retain the same evaluated descriptor.
        for owner in owners {
            effects.push(Effect::through(owner.clone().project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.initialized,
                )]), DependencyMutationKind::Write, None));

            if let Some(kind) = transition {
                effects.extend(
                    state
                        .memory
                        .owned_mutation_targets(
                            &owner,
                            bray_bound_tree::StorageProjection::ProductField(fields.pointer),
                        )?
                        .into_iter()
                        .map(|target| Effect::through(target.project(&[StorageProjection::AnyElement]), kind, None)),
                );
            }
        }

        Some(effects)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_memory_values(&mut self, execution: &crate::analysis::guarantee::ExecutionState, operation: &bray_bound_tree::CheckedMemoryOperation, effects: &[Effect]) {
        use bray_symbols::{CapturedValueOperation as Op, CapturedValueTemplate as Value, SymbolOrdinal};
        use bray_bound_tree::{StorageValueDestination, StorageValueInput, StorageValueTransition};

        if matches!(operation.kind(), CheckedMemoryOperationKind::RawBufferAllocate) {
            let result = self.expression_result_storage(operation.expression()).expect("buffer allocation has a result slot");
            let ty = self.storage.identity_type(result.root).expect("buffer allocation result retains its checked type");
            let fields = self.input.raw_buffers.get(&ty).expect("checked buffer allocation prepares its descriptor fields");
            let initialized = self.buffer_count(0);

            self.value_transitions.insert(operation.expression().into(), [StorageValueTransition::new(None, [(StorageValueDestination::Result,
                Value::Aggregate { base: std::sync::Arc::new(Value::Unknown),
                    fields: [
                        (StorageProjection::ProductField(fields.capacity), Value::Data(StorageValueInput::Argument(SymbolOrdinal::new(0)))),
                        (StorageProjection::ProductField(fields.initialized), Value::Data(StorageValueInput::Term(initialized))),
                        (StorageProjection::OwnedTarget, Value::Operation(Op::OwningStorage(StorageProjection::ProductField(fields.pointer)),
                            [Value::Operation(Op::EmptyPrefix, [].into())].into())),
                    ].into() })])].into());

            return;
        }

        if !matches!(operation.kind(), CheckedMemoryOperationKind::RawBufferPush { .. } | CheckedMemoryOperationKind::RawBufferPop { .. }) { return; }

        let base = Value::Operation(Op::Read, [Value::Input(StorageValueInput::Argument(SymbolOrdinal::new(0)))].into());
        let prefix = Value::Operation(Op::Select(StorageProjection::OwnedTarget), [base.clone()].into());
        let pop = matches!(operation.kind(), CheckedMemoryOperationKind::RawBufferPop { .. });
        let mut writes = Vec::new();

        let ty = self.request.semantic_values().unborrowed_type(self.execution.semantics.types()
            .expression(operation.arguments()[0]).expect("checked buffer receiver has a type").ty());

        let fields = &self.input.raw_buffers[&ty];

        let receiver = execution.admitted_call(operation.expression().into())
            .and_then(|entry| crate::ExecutionPlace::argument(SymbolOrdinal::new(0)).value_in(&entry.arguments));

        if let Some(target) = receiver.as_ref().and_then(crate::ExecutionCondition::inline_captured_storage)
            .and_then(bray_bound_tree::StorageCapturedValue::known_address) {
            let value = if pop { Value::Operation(Op::Remainder, [prefix.clone()].into()) }
                else { Value::Operation(Op::Append, [prefix.clone(), Value::Input(StorageValueInput::Argument(SymbolOrdinal::new(1)))].into()) };

            let initialized = StorageProjection::ProductField(fields.initialized);

            let count = Value::Operation(if pop { Op::Subtract } else { Op::Add }, [
                Value::Operation(Op::Select(initialized), [base.clone()].into()),
                Value::Data(StorageValueInput::Term(self.buffer_count(1))),
            ].into());

            writes.push((StorageValueDestination::Current { target: target.clone(), ty },
                Value::Aggregate { base: std::sync::Arc::new(base), fields: [
                    (initialized, count), (StorageProjection::OwnedTarget, value),
                ].into() }));
        }

        if !pop {
            for effect in effects {
                if let Effect::Mutation { kind: DependencyMutationKind::Replace(_), footprint, .. } = effect {
                    writes.push((StorageValueDestination::Region(footprint.clone()), Value::Input(StorageValueInput::Argument(SymbolOrdinal::new(1)))));
                }
            }
        }

        if pop {
            writes.push((StorageValueDestination::Result, Value::Operation(Op::Last, [prefix].into())));
        }

        let values: std::sync::Arc<[StorageValueTransition]> = [StorageValueTransition::new(None, writes)].into();

        if self.publish {
            self.value_transitions.entry(operation.expression().into()).and_modify(|previous| {
                if previous != &values { *previous = StorageValueTransition::join_reachable(previous, &values); }
            }).or_insert(values);
        } else {
            self.value_transitions.insert(operation.expression().into(), values);
        }
    }

    fn buffer_count(&self, count: u64) -> bray_symbols::ConstantTermId {
        let values = self.request.semantic_values();

        let value = values.intern_constant_value(bray_symbols::ConstantValueData::new(
            crate::representation::representation_type(self.request, bray_compiler_known::RepresentationRole::ScalarUsize),
            bray_symbols::ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(count)),
        )).expect("checked buffer count belongs to its semantic store");

        values.intern_constant_term(bray_symbols::ConstantTermData::Value(value))
            .expect("checked buffer count term belongs to its semantic store")
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

        let fields = self
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

        let relocate = matches!(
            operation.kind(),
            CheckedMemoryOperationKind::RawBufferRelocate { .. }
        );

        let source_index = usize::from(!relocate);
        let destination_index = usize::from(relocate);

        let (_, fields, source) = self.buffer_argument(state, operation, source_index)?;

        let (buffer_type, _, destination) =
            self.buffer_argument(state, operation, destination_index)?;

        let sources = source.mutation_targets(&state.memory)?;
        let destinations = destination.mutation_targets(&state.memory)?;
        let exact = source.is_exact(&state.memory) && destination.is_exact(&state.memory);

        let source_regions = sources
            .iter()
            .map(|owner| {
                state
                    .memory
                    .owned_mutation_targets(owner, StorageProjection::ProductField(fields.pointer))
            })
            .collect::<Option<Vec<_>>>()?;

        let destination_regions = destinations
            .iter()
            .map(|owner| {
                state
                    .memory
                    .owned_mutation_targets(owner, StorageProjection::ProductField(fields.pointer))
            })
            .collect::<Option<Vec<_>>>()?;

        // Effect alternatives own descriptors while sharing immutable projection paths.
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
                    effects.push(Effect::through(owner.clone().project(&[StorageProjection::ProductField(fields.initialized)]), DependencyMutationKind::Write, None));
                }

                effects.extend(
                    regions
                        .iter()
                        .flatten()
                        .map(|target| Effect::through(target.clone(), kind, None)),
                );
            }
        } else {
            effects.extend(destinations.iter().map(|target| {
                Effect::through(target.clone(), DependencyMutationKind::Replace(buffer_type), None)
            }));

            effects.extend(
                sources
                    .iter()
                    .map(|target| Effect::through(target.clone(), DependencyMutationKind::Clear, None)),
            );
        }

        // A transfer can publish its payload through the destination, never the
        // address of either buffer descriptor or its enclosing allocation.
        if destinations.iter().any(|target| {
            state.memory.may_retain_address(&Effect::through(target.clone(), DependencyMutationKind::Write, None))
        }) {
            effects.extend(
                source_regions
                    .iter()
                    .flatten()
                    .map(|target| Effect::through(target.clone(), DependencyMutationKind::Escape, None)),
            );
        }

        if exact && let ([source], [destination]) = (sources.as_slice(), destinations.as_slice()) {
            let call = operation.expression().into();

            if relocate {
                if let ([source], [destination]) = (
                    source_regions[0].as_slice(),
                    destination_regions[0].as_slice(),
                ) {
                    state.memory.stage_value(call, source.footprint(), destination.footprint().clone());
                    state.memory.stage_null(call, source.footprint().clone());
                }
            } else {
                state.memory.stage_value(call, source.footprint(), destination.footprint().clone());
                state.memory.stage_null(call, source.footprint().clone());

                state.memory.stage_null(
                    call,
                    source.footprint().project(&[StorageProjection::ProductField(fields.pointer)]),
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

        let (_, fields, source) = self.buffer_argument(state, operation, 0)?;

        let owners = source.mutation_targets(&state.memory)?;
        let mut effects = Vec::new();

        // Release snapshots the owner descriptor and its actual backing before clearing either.
        for owner in &owners {
            effects.push(Effect::through(owner.clone(), DependencyMutationKind::Clear, None));

            effects.extend(
                state
                    .memory
                    .owned_mutation_targets(
                        owner,
                        bray_bound_tree::StorageProjection::ProductField(fields.pointer),
                    )?
                    .into_iter()
                    .map(|target| Effect::through(target, DependencyMutationKind::Retire, None)),
            );
        }

        // Element cleanup belongs to the preceding lifecycle operation. This
        // storage release leaves one empty descriptor and no initialized backing.
        if source.targets(&state.memory).is_some()
            && let [owner] = owners.as_slice()
        {
            let call = operation.expression().into();

            state.memory.stage_null(call, owner.footprint().clone());

            state.memory.stage_null(
                call,
                owner.footprint().project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.pointer,
                )]),
            );
        }

        Some(effects)
    }
}
