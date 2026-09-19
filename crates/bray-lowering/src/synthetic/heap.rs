use bray_bound_tree::{BoundCallResult, CheckedMemoryOperationKind, MemoryLayoutQueryKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallPanicEdge, MirCallTarget, MirCallableReference,
    MirCapacityError, MirCleanupEdge, MirCleanupPhase, MirEdge, MirMemoryOperation, MirOperand,
    MirOperationKind, MirPlace, MirProjection, MirProjectionKind, MirSourceAnchor,
    MirStandardLibraryHelper, MirStorageKind, MirStoreKind, MirTerminatorKind, MirUnit,
    MirUnitBuilder, MirUnitId,
};
use bray_symbols::{BorrowKind, CallableAbi, CallableDefinitionId, TypeData, TypeId};

use super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

/// Closed compiler-provided storage operation selected by exact declaration identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapStorageMethod {
    /// Allocates and initializes an owned target.
    Create,
    /// Borrows an initialized target immutably.
    Borrow,
    /// Borrows an initialized target mutably.
    BorrowMut,
    /// Destroys the target without releasing storage.
    Destroy,
    /// Releases storage after the target has been consumed or destroyed.
    Release,
}
/// Checked signature and specialization of one compiler-provided Heap method.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HeapStorageLoweringInput {
    unit: MirUnitId,
    definition: CallableDefinitionId,
    method: HeapStorageMethod,
    element: TypeId,
    parameter_type: TypeId,
    result: Option<TypeId>,
    target: bray_ir::MirTargetContract,
}

impl HeapStorageLoweringInput {
    /// Creates lowering input after exact declaration and closed signature selection.
    pub const fn new(
        unit: MirUnitId,
        definition: CallableDefinitionId,
        method: HeapStorageMethod,
        element: TypeId,
        parameter_type: TypeId,
        result: Option<TypeId>,
        target: bray_ir::MirTargetContract,
    ) -> Self {
        Self {
            unit,
            definition,
            method,
            element,
            parameter_type,
            result,
            target,
        }
    }
}

/// Lowers a selected compiler-provided Heap operation, preserving checked allocation failure cleanup.
pub fn lower_heap_storage<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    input: HeapStorageLoweringInput,
) -> Result<MirUnit, C::Error> {
    SyntheticLowerer { context }.lower_heap_storage(input)
}

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    fn lower_heap_storage(&self, input: HeapStorageLoweringInput) -> Result<MirUnit, C::Error> {
        let HeapStorageLoweringInput {
            unit,
            definition,
            method,
            element,
            parameter_type,
            result,
            target,
        } = input;

        let missing =
            || panic!("synthetic lowering callable {definition:?} must have a result type");

        let source = MirSourceAnchor::CompilerProvidedCallable(definition);

        let mut builder =
            MirUnitBuilder::for_compiler_provided_callable(unit, definition, target.clone());

        let invalid = |cause| self.capacity_error(cause);

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(invalid)?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Parameter(0), parameter_type)
            .map_err(invalid)?;

        let parameter = MirPlace::new(storage, [], parameter_type);

        let (end, returned) = match method {
            HeapStorageMethod::Create => self.push_heap_construction(
                &mut builder,
                entry,
                &source,
                parameter,
                result.unwrap_or_else(missing),
            )?,
            HeapStorageMethod::Borrow | HeapStorageMethod::BorrowMut => {
                let pointer = self.heap_stored_pointer(parameter)?;
                let result = result.unwrap_or_else(missing);

                let value = push_heap_memory(
                    &mut builder,
                    entry,
                    &source,
                    CheckedMemoryOperationKind::Reinterpret {
                        source: element,
                        target: element,
                    },
                    [pointer],
                    Some(result),
                )
                .map_err(invalid)?;

                (entry, value)
            }
            HeapStorageMethod::Destroy => {
                let end =
                    self.destroy_heap_target(&mut builder, entry, &source, parameter, element)?;

                (end, None)
            }
            HeapStorageMethod::Release => {
                let layout = self.push_heap_layout(&mut builder, entry, &source, element)?;

                push_heap_memory(
                    &mut builder,
                    entry,
                    &source,
                    CheckedMemoryOperationKind::RawDeallocate,
                    [
                        MirOperand::Move(parameter),
                        layout[0].clone(),
                        layout[1].clone(),
                    ],
                    None,
                )
                .map_err(invalid)?;

                (entry, None)
            }
        };

        builder.set_terminator(end, source.clone(), MirTerminatorKind::Return(returned));

        Ok(builder.finish(entry))
    }

    fn destroy_heap_target(
        &self,
        builder: &mut MirUnitBuilder,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        parameter: MirPlace,
        element: TypeId,
    ) -> Result<MirBlockId, C::Error> {
        let values = self.context.semantic_values();
        let invalid = |cause| self.capacity_error(cause);
        let missing = || panic!("synthetic heap operation for {element:?} must produce a value");
        let pointer = self.heap_stored_pointer(parameter)?;

        let borrow_type = values
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: element,
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let pointer = push_heap_memory(
            builder,
            entry,
            source,
            CheckedMemoryOperationKind::Reinterpret {
                source: element,
                target: element,
            },
            [pointer],
            Some(borrow_type),
        )
        .map_err(invalid)?
        .unwrap_or_else(missing);

        let slot = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, borrow_type)
            .map_err(invalid)?;

        builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: MirPlace::new(slot, [], borrow_type),
                    value: pointer,
                },
                None,
            )
            .map_err(invalid)?;

        let pointee = MirPlace::new(
            slot,
            [MirProjection::new(
                MirProjectionKind::Dereference,
                borrow_type,
                element,
            )],
            element,
        );

        self.resolve_lifecycle_sequence(
            builder,
            entry,
            source,
            [MirOperationKind::Destroy(pointee)],
        )
    }

    fn heap_stored_pointer(&self, parameter: MirPlace) -> Result<MirOperand, C::Error> {
        let data = self.context.semantic_values().type_data(parameter.ty());

        let TypeData::Borrow { target, .. } = data.as_ref() else {
            panic!(
                "synthetic heap storage parameter must be a borrow, got {:?}",
                parameter.ty()
            );
        };

        Ok(MirOperand::Copy(MirPlace::new(
            parameter.storage(),
            [MirProjection::new(
                MirProjectionKind::Dereference,
                parameter.ty(),
                *target,
            )],
            *target,
        )))
    }

    fn push_heap_layout(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        element: TypeId,
    ) -> Result<[MirOperand; 2], C::Error> {
        let usize_type = self
            .context
            .representation_type(RepresentationRole::ScalarUsize)?;

        let query = |builder: &mut MirUnitBuilder, kind| -> Result<MirOperand, C::Error> {
            push_heap_memory(
                builder,
                block,
                source,
                CheckedMemoryOperationKind::LayoutQuery { ty: element, kind },
                [],
                Some(usize_type),
            )
            .map_err(|cause| self.capacity_error(cause))?
            .map_or_else(
                || panic!("synthetic heap layout query for {element:?} must produce a value"),
                Ok,
            )
        };

        Ok([
            query(builder, MemoryLayoutQueryKind::Size)?,
            query(builder, MemoryLayoutQueryKind::Alignment)?,
        ])
    }

    fn push_heap_construction(
        &self,
        builder: &mut MirUnitBuilder,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        parameter: MirPlace,
        result: TypeId,
    ) -> Result<(MirBlockId, Option<MirOperand>), C::Error> {
        let element = parameter.ty();
        let layout = self.push_heap_layout(builder, entry, source, element)?;

        let allocator = self
            .context
            .standard_library_callable(MirStandardLibraryHelper::MemoryAllocate)?;

        let byte_type = self
            .context
            .representation_type(RepresentationRole::ScalarU8)?;

        let pointer_type = self
            .context
            .compiler_known_symbols()
            .unary_representation_type(
                self.context.semantic_values(),
                RepresentationRole::RawPointer,
                byte_type,
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .unwrap_or_else(|| {
                panic!(
                    "synthetic lowering contract violation: UnresolvedType {value:?}",
                    value = byte_type
                )
            });

        let invalid = |cause| self.capacity_error(cause);

        let allocation = builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(MirCallableReference::new(allocator, CallableAbi::Bray)),
                    BoundCallResult::Immediate(pointer_type),
                    layout,
                    [],
                )),
                Some(pointer_type),
            )
            .map_err(invalid)?
            .result()
            .unwrap_or_else(|| {
                panic!(
                    "synthetic lowering contract violation: UnresolvedType {value:?}",
                    value = pointer_type
                )
            });

        let completed = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(invalid)?;

        let completed_allocation = builder
            .push_block_parameter(completed, source.clone(), pointer_type)
            .map_err(invalid)?;

        let report_type = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let report_storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, report_type)
            .map_err(invalid)?;

        let report = MirPlace::new(report_storage, [], report_type);

        let panicked = self.push_heap_construction_failure(
            builder,
            source,
            parameter.clone(),
            Some(report.clone()),
        )?;

        let cancelled =
            self.push_heap_construction_failure(builder, source, parameter.clone(), None)?;

        builder.set_terminator(
            entry,
            source.clone(),
            MirTerminatorKind::CheckCallOutcome {
                completed: MirEdge::new(completed, [MirOperand::Value(allocation)]),
                panicked: MirCallPanicEdge::new(panicked, report),
                cancelled: MirEdge::new(cancelled, []),
            },
        );

        push_heap_memory(
            builder,
            completed,
            source,
            CheckedMemoryOperationKind::Write { pointee: element },
            [
                MirOperand::Value(completed_allocation),
                MirOperand::Move(parameter),
            ],
            None,
        )
        .map_err(invalid)?;

        let heap = push_heap_memory(
            builder,
            completed,
            source,
            CheckedMemoryOperationKind::Reinterpret {
                source: byte_type,
                target: element,
            },
            [MirOperand::Value(completed_allocation)],
            Some(result),
        )
        .map_err(invalid)?;

        Ok((completed, heap))
    }

    fn push_heap_construction_failure(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        parameter: MirPlace,
        report: Option<MirPlace>,
    ) -> Result<MirBlockId, C::Error> {
        let invalid = |cause| self.capacity_error(cause);

        let failed = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .map_err(invalid)?;

        let outcome = self.cleanup_outcome(builder, failed, source)?;

        if let Some(report) = report {
            outcome
                .initialize_panic(builder, failed, source, MirOperand::Move(report))
                .map_err(invalid)?;
        } else {
            outcome
                .initialize_cancellation(builder, failed, source)
                .map_err(invalid)?;
        }

        let broadcast = builder
            .push_block(source.clone(), MirBlockKind::CleanupBroadcast)
            .map_err(invalid)?;

        let resolution = builder
            .push_block(source.clone(), MirBlockKind::LifecycleResolution)
            .map_err(invalid)?;

        builder.set_terminator(
            failed,
            source.clone(),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::TaskCancellation,
                MirEdge::new(broadcast, []),
            )),
        );

        builder
            .push_operation(
                broadcast,
                source.clone(),
                MirOperationKind::Cleanup {
                    phase: MirCleanupPhase::TaskCancellation,
                    place: parameter.clone(),
                },
                None,
            )
            .map_err(invalid)?;

        let broadcast = outcome.check(builder, broadcast, source).map_err(invalid)?;

        builder.set_terminator(
            broadcast,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(resolution, []),
            )),
        );

        builder
            .push_operation(
                resolution,
                source.clone(),
                MirOperationKind::Cleanup {
                    phase: MirCleanupPhase::LifecycleResolution,
                    place: parameter,
                },
                None,
            )
            .map_err(invalid)?;

        let resolution = outcome
            .check(builder, resolution, source)
            .map_err(invalid)?;

        let completed = self.finish_cleanup_outcome(builder, resolution, source, &outcome)?;

        builder.set_terminator(completed, source.clone(), MirTerminatorKind::Unreachable);

        Ok(failed)
    }
}

fn push_heap_memory(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    kind: CheckedMemoryOperationKind,
    operands: impl IntoIterator<Item = MirOperand>,
    result: Option<TypeId>,
) -> Result<Option<MirOperand>, MirCapacityError> {
    let operands: Vec<_> = operands.into_iter().collect();

    let types = operands
        .iter()
        .map(|operand| builder.operand_type(operand))
        .collect::<Vec<_>>();

    let operation = MirMemoryOperation::new(kind, operands, types, result);

    Ok(builder
        .push_operation(
            block,
            source.clone(),
            MirOperationKind::Memory(operation),
            result,
        )?
        .result()
        .map(MirOperand::Value))
}
