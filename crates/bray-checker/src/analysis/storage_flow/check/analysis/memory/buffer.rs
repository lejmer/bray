use bray_symbols::DependencyMutationKind;
use bray_bound_tree::StorageProjection;
use super::super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::guarantee::ExecutionState;
use crate::analysis::storage_flow::memory::{Effect, Place, Target};
use crate::analysis::storage_flow::model::StorageFlowState;
use bray_bound_tree::{CheckedMemoryOperationKind, MemoryOperationStatus};

type BufferValue = bray_symbols::CapturedValueTemplate<bray_bound_tree::StorageValueInput, StorageProjection>;

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
        let execution = state.execution.as_ref().expect("buffer view keeps its admitted operands");

        let Some((_, fields, source)) = self.buffer_argument(execution, operation, 0) else {
            return MemoryOperationStatus::Recovered;
        };

        if let Some(result) = destination {
            let Some(owners) = source.address_regions() else {
                let kind = match operation.kind() {
                    CheckedMemoryOperationKind::RawBufferInitializedSlice | CheckedMemoryOperationKind::RawBufferInitializedSliceMut =>
                        bray_symbols::DependencyReferenceKind::Borrow,
                    _ => bray_symbols::DependencyReferenceKind::Address,
                };

                state.memory.point_at_addresses(bray_bound_tree::StorageAddressValue::unknown(), result, fields.element, kind);

                return MemoryOperationStatus::Valid;
            };

            // Every view reaches the same allocation of the owner's element type.
            // A slice or spare-capacity view does not redeclare its backing storage.
            state.memory.owned_region(
                &owners,
                Target::storage(result.clone()),
                fields.element,
                bray_bound_tree::StorageProjection::ProductField(fields.pointer),
                source.known_address().is_none(),
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
        execution: &ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        transition: Option<bray_symbols::DependencyMutationKind>,
    ) -> Option<Vec<Effect>> {

        let (_, fields, source) = self.buffer_argument(execution, operation, 0)?;

        let owners = source.mutation_targets()?;

        let mut effects = Vec::new();

        // Prefix metadata and backing effects retain the same evaluated descriptor.
        for owner in owners {
            effects.push(Effect::through(owner.clone().project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.initialized,
                )]), DependencyMutationKind::Write, None));

            if let Some(kind) = transition {
                let backing = state.memory.owned_mutation_targets(
                    &owner, StorageProjection::ProductField(fields.pointer),
                );

                if let Some(backing) = backing {
                    effects.extend(backing.into_iter()
                        .map(|target| Effect::through(target.project(&[StorageProjection::AnyElement]), kind, None)));
                } else {
                    effects.push(Effect::unbounded(kind, None, false));
                }
            }
        }

        Some(effects)
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn resolve_memory_values(
        &self, execution: &crate::analysis::guarantee::ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation, effects: &[Effect],
    ) -> std::sync::Arc<[bray_bound_tree::StorageValueTransition]> {
        use bray_symbols::{CapturedValueOperation as Op, CapturedValueTemplate as Value, SymbolOrdinal};
        use bray_bound_tree::{StorageValueDestination, StorageValueInput, StorageValueTransition};

        if matches!(operation.kind(), CheckedMemoryOperationKind::RawBufferAllocate) {
            let result = self.expression_result_storage(operation.expression()).expect("buffer allocation has a result slot");
            let ty = self.storage.identity_type(result.root).expect("buffer allocation result retains its checked type");
            let fields = self.input.raw_buffers.get(&ty).expect("checked buffer allocation prepares its descriptor fields");
            let initialized = self.buffer_count(0);

            return [StorageValueTransition::new(None, [(StorageValueDestination::Result,
                Value::Aggregate { base: std::sync::Arc::new(Value::Unknown),
                    fields: [
                        (StorageProjection::ProductField(fields.capacity), Value::Data(StorageValueInput::Argument(SymbolOrdinal::new(0)))),
                        (StorageProjection::ProductField(fields.initialized), Value::Data(StorageValueInput::Term(initialized))),
                        (StorageProjection::OwnedTarget, Value::Operation(Op::OwningStorage(StorageProjection::ProductField(fields.pointer)),
                            [Value::Operation(Op::EmptyPrefix, [].into())].into())),
                    ].into() })])].into();
        }

        if !matches!(operation.kind(), CheckedMemoryOperationKind::RawBufferPush { .. } | CheckedMemoryOperationKind::RawBufferPop { .. }) {
            return self.buffer_bookkeeping_values(execution, operation, effects).into_iter().collect();
        }

        let base = Value::Operation(Op::Read, [Value::Input(StorageValueInput::Argument(SymbolOrdinal::new(0)))].into());
        let prefix = Value::Operation(Op::Select(StorageProjection::OwnedTarget), [base.clone()].into());
        let pop = matches!(operation.kind(), CheckedMemoryOperationKind::RawBufferPop { .. });
        let mut writes = Vec::new();

        let ty = self.request.semantic_values().unborrowed_type(self.execution.semantics.types()
            .expression(operation.arguments()[0]).expect("checked buffer receiver has a type").ty());

        let fields = &self.input.raw_buffers[&ty];

        if let Some((_, _, receiver)) = self.buffer_argument(execution, operation, 0)
            && let Some(target) = receiver.known_address() {
            let value = if pop { Value::Operation(Op::Remainder, [prefix.clone()].into()) }
                else { Value::Operation(Op::Append, [prefix.clone(), Value::Input(StorageValueInput::Argument(SymbolOrdinal::new(1)))].into()) };

            let initialized = StorageProjection::ProductField(fields.initialized);

            let count = Value::Operation(if pop { Op::Subtract } else { Op::Add }, [
                Value::Operation(Op::Select(initialized), [base.clone()].into()),
                Value::Data(StorageValueInput::Term(self.buffer_count(1))),
            ].into());

            writes.push((StorageValueDestination::Current { target: target.clone(), ty, installed: false },
                Value::Aggregate { base: std::sync::Arc::new(base), fields: [
                    (initialized, count), (StorageProjection::OwnedTarget, value),
                ].into() }));
        }

        if !pop {
            for effect in effects {
                if let Effect::Mutation { kind: DependencyMutationKind::Replace(_), footprint, .. } = effect {
                    writes.push((StorageValueDestination::Region {
                        target: footprint.clone(),
                        via: bray_symbols::DependencySubject::root(bray_symbols::DependencySubjectRoot::Receiver),
                    }, Value::Input(StorageValueInput::Argument(SymbolOrdinal::new(1)))));
                }
            }
        }

        if pop {
            writes.push((StorageValueDestination::Result, Value::Operation(Op::Last, [prefix].into())));
        }

        [StorageValueTransition::new(None, writes)].into()
    }

    fn buffer_bookkeeping_values(
        &self,
        execution: &ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        effects: &[Effect],
    ) -> Option<bray_bound_tree::StorageValueTransition> {
        use bray_bound_tree::{StorageValueDestination, StorageValueInput, StorageValueTransition};
        use bray_symbols::{CapturedValueOperation as Op, SymbolOrdinal};
        use CheckedMemoryOperationKind::{RawBufferRelease, RawBufferRelocate, RawBufferReplace, RawBufferSetInitializedCount};

        if effects.is_empty() || !matches!(operation.kind(),
            RawBufferRelocate { .. } | RawBufferReplace { .. } | RawBufferRelease { .. } | RawBufferSetInitializedCount)
        {
            return None;
        }

        let operand = |index: usize| {
            let (ty, fields, captured) = self.buffer_argument(execution, operation, index)?;

            let target = captured.known_address()?.clone();

            let value = BufferValue::Operation(Op::Read, [BufferValue::Input(StorageValueInput::Argument(
                SymbolOrdinal::new(u32::try_from(index).expect("checked buffer operand position fits an ordinal")),
            ))].into());

            Some((StorageValueDestination::Current { target, ty, installed: true }, fields, value))
        };

        let (first, fields, first_value) = operand(0)?;

        let initialized = StorageProjection::ProductField(fields.initialized);
        let backing = StorageProjection::OwnedTarget;
        let zero = BufferValue::Data(StorageValueInput::Term(self.buffer_count(0)));
        let prefix = |contents| BufferValue::Operation(Op::OwningStorage(StorageProjection::ProductField(fields.pointer)), [contents].into());
        let empty = prefix(BufferValue::Operation(Op::EmptyPrefix, [].into()));

        let overlay = |base, count, prefix| BufferValue::Aggregate {
            base: std::sync::Arc::new(base), fields: [(initialized, count), (backing, prefix)].into(),
        };

        let empty_buffer = || BufferValue::Aggregate {
            base: std::sync::Arc::new(BufferValue::Unknown),
            fields: [(StorageProjection::ProductField(fields.capacity), zero.clone()), (initialized, zero.clone()), (backing, empty.clone())].into(),
        };

        let writes = match operation.kind() {
            RawBufferRelease { .. } => vec![(first, empty_buffer())],
            RawBufferSetInitializedCount => {
                let count = BufferValue::Data(StorageValueInput::Argument(SymbolOrdinal::new(1)));

                vec![(first, overlay(first_value, count, prefix(BufferValue::Unknown)))]
            }
            RawBufferRelocate { .. } => {
                let (second, _, second_value) = operand(1)?;

                if first == second { return None; }

                let count = BufferValue::Operation(Op::Select(initialized), [first_value.clone()].into());
                let moved = BufferValue::Operation(Op::Select(backing), [first_value.clone()].into());

                vec![(first, overlay(first_value, zero, empty)), (second, overlay(second_value, count, moved))]
            }
            RawBufferReplace { .. } => {
                let (second, _, second_value) = operand(1)?;

                if first == second { return None; }

                vec![(first, second_value), (second, empty_buffer())]
            }
            _ => unreachable!("buffer bookkeeping only binds checked storage operations"),
        };

        Some(StorageValueTransition::new(None, writes))
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

    fn buffer_argument<'entry>(
        &self,
        execution: &'entry ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
        index: usize,
    ) -> Option<(
        bray_symbols::TypeId,
        crate::representation::RawBufferFields,
        &'entry bray_bound_tree::StorageCapturedValue,
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

        let capture = self.captured_memory_operand(execution, operation, argument)?;

        Some((ty, *fields, capture))
    }

    pub(super) fn buffer_transfer_effects(
        &self,
        state: &mut StorageFlowState,
        execution: &ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> Option<Vec<Effect>> {

        let relocate = matches!(
            operation.kind(),
            CheckedMemoryOperationKind::RawBufferRelocate { .. }
        );

        let source_index = usize::from(!relocate);
        let destination_index = usize::from(relocate);

        let (_, fields, source) = self.buffer_argument(execution, operation, source_index)?;

        let (buffer_type, _, destination) =
            self.buffer_argument(execution, operation, destination_index)?;

        let sources = source.mutation_targets()?;
        let destinations = destination.mutation_targets()?;
        let exact = source.known_address().is_some() && destination.known_address().is_some();

        let pointer = StorageProjection::ProductField(fields.pointer);

        let backings = |owners: &[bray_bound_tree::StorageMutationTarget]| {
            owners.iter().map(|owner| state.memory.owned_mutation_targets(owner, pointer)).collect::<Vec<_>>()
        };

        let source_regions = backings(&sources);
        let destination_regions = backings(&destinations);

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
                for (owner, backing) in owners.iter().zip(regions) {
                    effects.push(Effect::through(owner.clone().project(&[StorageProjection::ProductField(fields.initialized)]), DependencyMutationKind::Write, None));

                    effects.extend(buffer_backing_effects(backing.as_deref(), kind));
                }
            }
        } else {
            for (owner, regions) in destinations.iter().zip(&destination_regions) {
                self.discard_buffer_contents(&state.memory, owner, buffer_type, regions.as_deref(), &mut effects);
                effects.push(Effect::through(owner.clone(), DependencyMutationKind::Replace(buffer_type), None));
            }

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
            for regions in &source_regions {
                effects.extend(buffer_backing_effects(regions.as_deref(), DependencyMutationKind::Escape));
            }
        }

        if exact && let ([source], [destination]) = (sources.as_slice(), destinations.as_slice())
            && source.footprint() != destination.footprint()
        {
            let call = operation.expression().into();

            if relocate {
                let address = |targets: &[Option<Vec<bray_bound_tree::StorageMutationTarget>>]| {
                    let [target] = targets[0].as_deref()? else { return None; };

                    target.address.clone()
                };

                if let (Some(source), Some(destination)) = (address(&source_regions), address(&destination_regions))
                    && crate::storage::buffer_backings_disjoint(self.request.semantic_values(), self.storage, &source, &destination)
                {
                    state.memory.stage_value(call, &source, destination);
                    state.memory.stage_null(call, source);
                }
            } else {
                state.memory.stage_value(call, source.footprint(), destination.footprint().clone());
                state.memory.stage_whole_sources(call, source.footprint().clone(), [], true, false);

                state.memory.stage_null(
                    call,
                    source.footprint().project(&[pointer]),
                );
            }
        }

        Some(effects)
    }

    pub(super) fn buffer_release_effects(
        &self,
        state: &mut StorageFlowState,
        execution: &ExecutionState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> Option<Vec<Effect>> {

        let (ty, fields, source) = self.buffer_argument(execution, operation, 0)?;

        let owners = source.mutation_targets()?;
        let mut effects = Vec::new();

        // Release snapshots the owner descriptor and its actual backing before clearing either.
        for owner in &owners {
            let regions = state.memory.owned_mutation_targets(owner, StorageProjection::ProductField(fields.pointer));

            self.discard_buffer_contents(&state.memory, owner, ty, regions.as_deref(), &mut effects);
            effects.push(Effect::through(owner.clone(), DependencyMutationKind::Clear, None));
        }

        // Element cleanup belongs to the preceding lifecycle operation. This
        // storage release leaves one empty descriptor and no initialized backing.
        if source.known_address().is_some()
            && let [owner] = owners.as_slice()
        {
            let call = operation.expression().into();

            state.memory.stage_whole_sources(call, owner.footprint().clone(), [], true, false);

            state.memory.stage_null(
                call,
                owner.footprint().project(&[bray_bound_tree::StorageProjection::ProductField(
                    fields.pointer,
                )]),
            );
        }

        Some(effects)
    }

    fn discard_buffer_contents(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        owner: &bray_bound_tree::StorageMutationTarget,
        ty: bray_symbols::TypeId,
        regions: Option<&[bray_bound_tree::StorageMutationTarget]>,
        effects: &mut Vec<Effect>,
    ) {
        let cleanup = owner.address.as_ref().and_then(|receiver| self.buffer_cleanup_effects(memory, receiver, ty))
            .unwrap_or_else(|| memory.cleanup_effects(owner.address.as_ref(), ty));

        effects.extend(cleanup);
        effects.extend(buffer_backing_effects(regions, DependencyMutationKind::Retire));
    }
}

fn buffer_backing_effects(
    backing: Option<&[bray_bound_tree::StorageMutationTarget]>,
    kind: DependencyMutationKind,
) -> impl Iterator<Item = Effect> + '_ {
    backing.into_iter().flatten().cloned().map(move |target| Effect::through(target, kind, None))
        .chain(backing.is_none().then(|| Effect::unbounded(kind, None, false)))
}
