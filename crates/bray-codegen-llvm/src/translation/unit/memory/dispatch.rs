use bray_bound_tree::{CheckedMemoryOperationKind, MemoryOffsetUnit};
use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation, MirOperationId};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm, pointer_value};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in super::super) fn translate_memory(
        &mut self,
        id: MirOperationId,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        self.ensure_memory_available(memory.kind())?;

        match memory.kind() {
            CheckedMemoryOperationKind::Address { .. } => {
                let [value] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                self.operand(value)
                    .and_then(|value| {
                        pointer_value(value)
                            .map(BasicValueEnum::from)
                            .ok_or(CodegenFailure::GeneratedModuleInvariant)
                    })
                    .map(Some)
            }
            CheckedMemoryOperationKind::Null { .. } => {
                let result = self.operation_result_type(operation)?;

                let BasicTypeEnum::PointerType(pointer) = self.types.map(result)? else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                Ok(Some(pointer.const_null().into()))
            }
            CheckedMemoryOperationKind::IsNull { .. } => {
                let [pointer] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let pointer = self.memory_pointer(pointer)?;

                llvm(self.builder.build_int_compare(
                    IntPredicate::EQ,
                    self.pointer_address(pointer)?,
                    self.pointer_integer_type().const_zero(),
                    "memory.is_null",
                ))
                .map(|value| Some(value.into()))
            }
            CheckedMemoryOperationKind::Offset { unit, pointee } => {
                let [pointer, offset] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let pointer = self.memory_pointer(pointer)?;

                let offset = self.operand(offset).and_then(|value| {
                    int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
                })?;

                let stride = match unit {
                    MemoryOffsetUnit::Element => self.memory_layout(pointee)?.size(),
                    MemoryOffsetUnit::Byte => 1,
                };

                self.dynamic_offset_pointer(pointer, offset, stride)
                    .map(|value| Some(value.into()))
            }
            CheckedMemoryOperationKind::Reinterpret { .. } => {
                let [pointer] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                self.memory_pointer(pointer).map(|value| Some(value.into()))
            }
            CheckedMemoryOperationKind::CallbackState { .. } => {
                let [context] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                self.memory_pointer(context).map(|value| Some(value.into()))
            }
            CheckedMemoryOperationKind::Read { pointee, .. } => {
                let [pointer] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let pointer = self.memory_pointer(pointer)?;

                llvm(
                    self.builder
                        .build_load(self.types.map(pointee)?, pointer, "memory.read"),
                )
                .map(Some)
            }
            CheckedMemoryOperationKind::Write { .. } => {
                let [pointer, value] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let pointer = self.memory_pointer(pointer)?;
                let value = self.operand(value)?;

                llvm(self.builder.build_store(pointer, value))?;

                Ok(None)
            }
            CheckedMemoryOperationKind::Copy { pointee, kind } => {
                let [source, destination, count] = memory.operands() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                self.translate_memory_copy(destination, source, count, pointee, kind)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::LayoutQuery { ty, kind } => self
                .translate_layout_query(operation, memory, ty, kind)
                .map(Some),
            CheckedMemoryOperationKind::RawAllocate => self
                .translate_raw_memory_allocation(operation, memory)
                .map(Some),
            CheckedMemoryOperationKind::RawDeallocate => {
                self.translate_raw_memory_deallocation(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::Allocate => self
                .translate_owned_memory_allocation(operation, memory)
                .map(Some),
            CheckedMemoryOperationKind::Deallocate => {
                self.translate_owned_memory_deallocation(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::RawBufferCapacity => {
                self.translate_raw_buffer_field(memory, 1).map(Some)
            }
            CheckedMemoryOperationKind::RawBufferInitializedCount => {
                self.translate_raw_buffer_field(memory, 2).map(Some)
            }
            CheckedMemoryOperationKind::RawBufferPointer => {
                self.translate_raw_buffer_field(memory, 0).map(Some)
            }
            CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut => {
                self.translate_raw_buffer_slice(operation, memory).map(Some)
            }
            CheckedMemoryOperationKind::RawBufferSparePointer { element } => self
                .translate_raw_buffer_spare_pointer(memory, element)
                .map(|pointer| Some(pointer.into())),
            CheckedMemoryOperationKind::RawBufferSetInitializedCount => {
                self.translate_raw_buffer_set_initialized_count(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::RawBufferRelease { element } => {
                self.translate_raw_buffer_release(id, memory, element)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::RawBufferReplace { element } => {
                self.translate_raw_buffer_replace(id, memory, element)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ByteBufferFill => {
                self.translate_byte_buffer_fill(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ByteBufferCopy => {
                self.translate_byte_buffer_copy(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ByteBufferRead => {
                self.translate_byte_buffer_read(operation, memory).map(Some)
            }
            CheckedMemoryOperationKind::SliceLength => {
                self.translate_slice_length(memory).map(Some)
            }
        }
    }

    fn ensure_memory_available(
        &self,
        kind: CheckedMemoryOperationKind,
    ) -> Result<(), CodegenFailure> {
        let operations = self.request.target().profile().facts().operations();

        let available = match kind {
            CheckedMemoryOperationKind::LayoutQuery { .. }
            | CheckedMemoryOperationKind::SliceLength
            | CheckedMemoryOperationKind::CallbackState { .. } => true,
            CheckedMemoryOperationKind::RawAllocate
            | CheckedMemoryOperationKind::RawDeallocate
            | CheckedMemoryOperationKind::Allocate
            | CheckedMemoryOperationKind::Deallocate
            | CheckedMemoryOperationKind::RawBufferCapacity
            | CheckedMemoryOperationKind::RawBufferInitializedCount
            | CheckedMemoryOperationKind::RawBufferPointer
            | CheckedMemoryOperationKind::RawBufferInitializedSlice
            | CheckedMemoryOperationKind::RawBufferInitializedSliceMut
            | CheckedMemoryOperationKind::RawBufferSparePointer { .. }
            | CheckedMemoryOperationKind::RawBufferSetInitializedCount
            | CheckedMemoryOperationKind::RawBufferRelease { .. }
            | CheckedMemoryOperationKind::RawBufferReplace { .. } => operations.allocation(),
            _ => operations.raw_memory(),
        };

        if available {
            Ok(())
        } else {
            Err(CodegenFailure::UnsupportedTarget)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

    use bray_bound_tree::{
        CheckedMemoryOperationKind, MemoryAddressKind, MemoryCopyKind, MemoryLayoutQueryKind,
        MemoryOffsetUnit, MemoryReadKind,
    };
    use bray_codegen::test_support::{codegen_request_for_unit, codegen_target_with_profile};
    use bray_codegen::{
        CodeGenerator, CodegenCallableSignature, CodegenDebugLocation, CodegenFieldLayout,
        CodegenHelperMapping, CodegenLinkage, CodegenMappings, CodegenOperationMapping,
        CodegenResultMapping, CodegenSourceFile, CodegenSymbolKey, CodegenSymbolMapping,
        CodegenTypeKind, CodegenTypeMapping, CodegenUnionVariantLayout, CodegenUnit,
        TargetAddressSpaceKind,
    };
    use bray_ir::{
        MirAggregate, MirAggregateKind, MirBlockKind, MirCleanupPhase, MirHelperReference,
        MirMemoryOperation, MirOperand, MirOperationCommit, MirOperationId, MirOperationKind,
        MirPlace, MirSourceAnchor, MirStorageKind, MirTargetFacts, MirTerminatorKind, MirUnitBuilder,
        MirUnitKind, MirValueId,
    };
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiVersion};
    use bray_symbols::{
        BorrowKind, CallableAbi, IntegerConstant, SemanticValueStore, SymbolId, TypeData, TypeId,
        UnionVariantSymbolId,
    };
    use bray_target::test_support::test_target_profile;
    use bray_target::{
        TargetLayoutContract, TargetOperationFacts, TargetProfile, TargetValueLayout,
    };
    use inkwell::context::Context;

    use crate::backend::LlvmCodeGenerator;

    #[derive(Clone, Copy)]
    struct MemoryTypes {
        value: TypeId,
        borrow: TypeId,
        pointer: TypeId,
        usize: TypeId,
        boolean: TypeId,
        tag: TypeId,
        layout: TypeId,
        layout_error: TypeId,
        layout_result: TypeId,
        allocation: TypeId,
        byte: TypeId,
        raw_buffer: TypeId,
        raw_buffer_borrow: TypeId,
        slice: TypeId,
    }

    #[test]
    fn every_memory_operation_family_generates_verified_llvm() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = memory_operation_fixture(&backend);
        let context = Context::create();

        let (_machine, module) = backend
            .prepare_module(fixture.request(), &context)
            .unwrap_or_else(|error| panic!("memory LLVM generation must succeed: {error:?}"))
            .unwrap_or_else(|| panic!("memory LLVM generation must not be cancelled"));

        let ir = module.print_to_string().to_string();

        for spelling in [
            "llvm.memcpy",
            "llvm.memmove",
            bray_runtime_interface::MEMORY_ALLOCATION_SYMBOL,
            bray_runtime_interface::MEMORY_DEALLOCATION_SYMBOL,
        ] {
            assert!(
                ir.contains(spelling),
                "missing generated LLVM for {spelling}"
            );
        }

        for intrinsic in ["@llvm.memcpy", "@llvm.memmove"] {
            let call = ir
                .lines()
                .find(|line| line.contains("call void") && line.contains(intrinsic))
                .unwrap_or_else(|| panic!("missing generated call to {intrinsic}"));

            let destination = call
                .find("null")
                .unwrap_or_else(|| panic!("{intrinsic} destination must use the null fixture"));

            let source = call
                .find("%storage.0")
                .unwrap_or_else(|| panic!("{intrinsic} source must use fixture storage"));

            assert!(destination < source, "{intrinsic} operands are reversed");
        }

        let deallocations = ir
            .lines()
            .filter(|line| {
                line.contains("call void")
                    && line.contains(bray_runtime_interface::MEMORY_DEALLOCATION_SYMBOL)
            })
            .count();

        assert_eq!(deallocations, 4);
    }

    #[test]
    fn unavailable_memory_capabilities_fail_before_artifact_serialization() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        for (raw_memory, allocation) in [(false, true), (true, false)] {
            let fixture = memory_operation_fixture_for_target(
                &backend,
                memory_target(raw_memory, allocation),
            );

            let context = Context::create();

            assert!(matches!(
                backend.prepare_module(fixture.request(), &context),
                Err(bray_codegen::CodegenFailure::UnsupportedTarget)
            ));
        }
    }

    fn memory_operation_fixture(
        backend: &LlvmCodeGenerator,
    ) -> bray_codegen::test_support::CodegenRequestFixture {
        memory_operation_fixture_for_target(backend, memory_target(true, true))
    }

    fn memory_operation_fixture_for_target(
        backend: &LlvmCodeGenerator,
        target: bray_codegen::CodegenTarget,
    ) -> bray_codegen::test_support::CodegenRequestFixture {
        let types = memory_types();

        let mir_target =
            MirTargetFacts::new(target.profile().clone(), RuntimeAbiVersion::new(1, 0));

        let bound = bray_testing::test_bound_unit(171);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder =
            MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, mir_target);

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("memory test block must be valid: {error:?}"));

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Local, types.value)
            .unwrap_or_else(|error| panic!("memory test storage must be valid: {error:?}"));

        let place = MirPlace::new(storage, [], types.value);

        let borrowed = push_borrow(
            &mut builder,
            entry,
            &source,
            BorrowKind::Shared,
            place,
            types.borrow,
        );

        let address = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Address {
                pointee: types.value,
                kind: MemoryAddressKind::Shared,
            },
            [MirOperand::Value(borrowed)],
            [types.borrow],
            Some(types.pointer),
        )
        .result()
        .unwrap_or_else(|| panic!("memory address operation must produce a value"));

        let null = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Null {
                pointee: types.value,
            },
            [],
            [],
            Some(types.pointer),
        )
        .result()
        .unwrap_or_else(|| panic!("memory null operation must produce a value"));

        let size = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::LayoutQuery {
                ty: types.value,
                kind: MemoryLayoutQueryKind::Size,
            },
            [],
            [],
            Some(types.usize),
        )
        .result()
        .unwrap_or_else(|| panic!("memory size operation must produce a value"));

        let raw_allocation = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::RawAllocate,
            [MirOperand::Value(size), MirOperand::Value(size)],
            [types.usize, types.usize],
            Some(types.pointer),
        )
        .result()
        .unwrap_or_else(|| panic!("raw memory allocation must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::RawDeallocate,
            [
                MirOperand::Value(raw_allocation),
                MirOperand::Value(size),
                MirOperand::Value(size),
            ],
            [types.pointer, types.usize, types.usize],
            None,
        );

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::IsNull {
                pointee: types.value,
            },
            [MirOperand::Value(null)],
            [types.pointer],
            Some(types.boolean),
        );

        for unit in [MemoryOffsetUnit::Element, MemoryOffsetUnit::Byte] {
            push_memory(
                &mut builder,
                entry,
                &source,
                CheckedMemoryOperationKind::Offset {
                    pointee: types.value,
                    unit,
                },
                [MirOperand::Value(address), MirOperand::Value(size)],
                [types.pointer, types.usize],
                Some(types.pointer),
            );
        }

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Reinterpret {
                source: types.value,
                target: types.value,
            },
            [MirOperand::Value(address)],
            [types.pointer],
            Some(types.pointer),
        );

        let read = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Read {
                pointee: types.value,
                kind: MemoryReadKind::Copy,
            },
            [MirOperand::Value(address)],
            [types.pointer],
            Some(types.value),
        )
        .result()
        .unwrap_or_else(|| panic!("memory read operation must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Write {
                pointee: types.value,
            },
            [MirOperand::Value(address), MirOperand::Value(read)],
            [types.pointer, types.value],
            None,
        );

        for kind in [MemoryCopyKind::NonOverlapping, MemoryCopyKind::Overlapping] {
            push_memory(
                &mut builder,
                entry,
                &source,
                CheckedMemoryOperationKind::Copy {
                    pointee: types.value,
                    kind,
                },
                [
                    MirOperand::Value(address),
                    MirOperand::Value(null),
                    MirOperand::Value(size),
                ],
                [types.pointer, types.pointer, types.usize],
                None,
            );
        }

        for kind in [
            MemoryLayoutQueryKind::Alignment,
            MemoryLayoutQueryKind::Stride,
        ] {
            push_memory(
                &mut builder,
                entry,
                &source,
                CheckedMemoryOperationKind::LayoutQuery {
                    ty: types.value,
                    kind,
                },
                [],
                [],
                Some(types.usize),
            );
        }

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::LayoutQuery {
                ty: types.value,
                kind: MemoryLayoutQueryKind::Layout,
            },
            [MirOperand::Value(size)],
            [types.usize],
            Some(types.layout_result),
        );

        let layout = builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Aggregate(MirAggregate::new(
                    MirAggregateKind::Tuple,
                    [MirOperand::Value(size), MirOperand::Value(size)],
                )),
                Some(types.layout),
            )
            .unwrap_or_else(|error| panic!("memory layout fixture must be valid: {error:?}"))
            .result()
            .unwrap_or_else(|| panic!("memory layout fixture must produce a value"));

        let allocation = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Allocate,
            [MirOperand::Value(layout)],
            [types.layout],
            Some(types.allocation),
        )
        .result()
        .unwrap_or_else(|| panic!("memory allocation operation must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Deallocate,
            [MirOperand::Value(allocation)],
            [types.allocation],
            None,
        );

        let cleanup_operations = push_buffer_and_byte_operations(
            &mut builder,
            entry,
            &source,
            types,
            address,
            null,
            size,
        );

        builder
            .set_terminator(entry, source.clone(), MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("memory test return must be valid: {error:?}"));

        let mir = builder
            .finish(entry)
            .unwrap_or_else(|error| panic!("memory test MIR must be valid: {error:?}"));

        let unit = CodegenUnit::try_new(
            bray_codegen::CodegenPartitionPolicy::NATIVE_BALANCED,
            bray_codegen::test_support::codegen_partition_compatibility(),
            [mir],
        )
        .unwrap_or_else(|error| panic!("memory test codegen unit must be valid: {error:?}"));

        let mappings = memory_mappings(&unit, &target, types, source, cleanup_operations);

        codegen_request_for_unit(unit, target, mappings, backend.identity().clone())
    }

    fn push_memory<const OPERANDS: usize, const TYPES: usize>(
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        kind: CheckedMemoryOperationKind,
        operands: [MirOperand; OPERANDS],
        operand_types: [TypeId; TYPES],
        result_type: Option<TypeId>,
    ) -> MirOperationCommit {
        let operation = MirMemoryOperation::new(kind, operands, operand_types, result_type);

        let committed = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Memory(operation),
                result_type,
            )
            .unwrap_or_else(|error| panic!("memory test operation must be valid: {error:?}"));

        committed
    }

    fn push_borrow(
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        kind: BorrowKind,
        place: MirPlace,
        result_type: TypeId,
    ) -> MirValueId {
        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Borrow { kind, place },
                Some(result_type),
            )
            .unwrap_or_else(|error| panic!("memory test borrow must be valid: {error:?}"))
            .result()
            .unwrap_or_else(|| panic!("memory test borrow must produce a value"))
    }

    fn push_buffer_and_byte_operations(
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        types: MemoryTypes,
        address: MirValueId,
        null: MirValueId,
        size: MirValueId,
    ) -> Vec<MirOperationId> {
        let buffer = push_borrowed_storage(builder, block, source, types);
        let source_buffer = push_borrowed_storage(builder, block, source, types);

        for kind in [
            CheckedMemoryOperationKind::RawBufferCapacity,
            CheckedMemoryOperationKind::RawBufferInitializedCount,
        ] {
            push_memory(
                builder,
                block,
                source,
                kind,
                [MirOperand::Value(buffer)],
                [types.raw_buffer_borrow],
                Some(types.usize),
            );
        }

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferPointer,
            [MirOperand::Value(buffer)],
            [types.raw_buffer_borrow],
            Some(types.pointer),
        );

        for kind in [
            CheckedMemoryOperationKind::RawBufferInitializedSlice,
            CheckedMemoryOperationKind::RawBufferInitializedSliceMut,
        ] {
            let slice = push_memory(
                builder,
                block,
                source,
                kind,
                [MirOperand::Value(buffer)],
                [types.raw_buffer_borrow],
                Some(types.slice),
            )
            .result()
            .unwrap_or_else(|| panic!("raw buffer slice must produce a value"));

            push_memory(
                builder,
                block,
                source,
                CheckedMemoryOperationKind::SliceLength,
                [MirOperand::Value(slice)],
                [types.slice],
                Some(types.usize),
            );
        }

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferSparePointer {
                element: types.value,
            },
            [MirOperand::Value(buffer)],
            [types.raw_buffer_borrow],
            Some(types.pointer),
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferSetInitializedCount,
            [MirOperand::Value(buffer), MirOperand::Value(size)],
            [types.raw_buffer_borrow, types.usize],
            None,
        );

        let byte = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::ByteBufferRead,
            [MirOperand::Value(address), MirOperand::Value(size)],
            [types.pointer, types.usize],
            Some(types.byte),
        )
        .result()
        .unwrap_or_else(|| panic!("byte buffer read must produce a value"));

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::ByteBufferFill,
            [
                MirOperand::Value(null),
                MirOperand::Value(byte),
                MirOperand::Value(size),
            ],
            [types.pointer, types.byte, types.usize],
            None,
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::ByteBufferCopy,
            [
                MirOperand::Value(address),
                MirOperand::Value(null),
                MirOperand::Value(size),
            ],
            [types.pointer, types.pointer, types.usize],
            None,
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::CallbackState { state: types.value },
            [MirOperand::Value(null)],
            [types.pointer],
            Some(types.borrow),
        );

        let release = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferRelease {
                element: types.value,
            },
            [MirOperand::Value(buffer)],
            [types.raw_buffer_borrow],
            None,
        );

        let replace = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferReplace {
                element: types.value,
            },
            [
                MirOperand::Value(buffer),
                MirOperand::Value(source_buffer),
                MirOperand::Value(size),
            ],
            [
                types.raw_buffer_borrow,
                types.raw_buffer_borrow,
                types.usize,
            ],
            None,
        );

        vec![release.operation(), replace.operation()]
    }

    fn push_borrowed_storage(
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        types: MemoryTypes,
    ) -> MirValueId {
        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Local, types.raw_buffer)
            .unwrap_or_else(|error| panic!("raw buffer storage must be valid: {error:?}"));

        push_borrow(
            builder,
            block,
            source,
            BorrowKind::Mutable,
            MirPlace::new(storage, [], types.raw_buffer),
            types.raw_buffer_borrow,
        )
    }

    fn memory_types() -> MemoryTypes {
        let store = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("semantic value store must be valid: {error:?}"));

        let value = intern_type(&store, TypeData::Error);
        let borrow = intern_type(&store, TypeData::tuple([value]));
        let pointer = intern_type(&store, TypeData::tuple([borrow]));
        let usize = intern_type(&store, TypeData::tuple([pointer]));
        let boolean = intern_type(&store, TypeData::tuple([usize]));
        let tag = intern_type(&store, TypeData::tuple([boolean]));
        let layout = intern_type(&store, TypeData::tuple([tag]));
        let layout_error = intern_type(&store, TypeData::tuple([layout]));
        let layout_result = intern_type(&store, TypeData::tuple([layout_error]));
        let allocation = intern_type(&store, TypeData::tuple([layout_result]));
        let byte = intern_type(&store, TypeData::tuple([allocation]));
        let raw_buffer = intern_type(&store, TypeData::tuple([byte]));
        let raw_buffer_borrow = intern_type(&store, TypeData::tuple([raw_buffer]));
        let slice = intern_type(&store, TypeData::tuple([raw_buffer_borrow]));

        MemoryTypes {
            value,
            borrow,
            pointer,
            usize,
            boolean,
            tag,
            layout,
            layout_error,
            layout_result,
            allocation,
            byte,
            raw_buffer,
            raw_buffer_borrow,
            slice,
        }
    }

    fn memory_target(raw_memory: bool, allocation: bool) -> bray_codegen::CodegenTarget {
        let profile = test_target_profile();

        let facts = profile
            .facts()
            .clone()
            .with_operations(TargetOperationFacts::new(raw_memory, allocation));

        let profile =
            TargetProfile::try_new(profile.identity().clone(), profile.machine().clone(), facts)
                .unwrap_or_else(|error| {
                    panic!("memory test target profile must be valid: {error:?}")
                });

        codegen_target_with_profile(profile, "x86_64-unknown-linux-gnu")
    }

    fn intern_type(store: &SemanticValueStore, data: TypeData) -> TypeId {
        store
            .intern_type(data)
            .unwrap_or_else(|error| panic!("memory test type must be valid: {error:?}"))
    }

    fn memory_mappings(
        unit: &CodegenUnit,
        target: &bray_codegen::CodegenTarget,
        types: MemoryTypes,
        source: MirSourceAnchor,
        cleanup_operations: Vec<MirOperationId>,
    ) -> CodegenMappings {
        let align1 = NonZeroU64::MIN;
        let align4 = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);
        let align8 = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let width8 = NonZeroU16::new(8).unwrap_or(NonZeroU16::MIN);
        let width32 = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);
        let width64 = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);

        let layout = |size, alignment| {
            TargetValueLayout::new(size, alignment, TargetLayoutContract::Default)
        };

        let success = UnionVariantSymbolId::from_symbol_id(SymbolId::new(1));
        let failure = UnionVariantSymbolId::from_symbol_id(SymbolId::new(2));
        let overflow = UnionVariantSymbolId::from_symbol_id(SymbolId::new(3));
        let unsupported = UnionVariantSymbolId::from_symbol_id(SymbolId::new(4));

        let type_mappings = [
            CodegenTypeMapping::new(
                types.value,
                layout(4, align4),
                CodegenTypeKind::SignedInteger(width32),
            ),
            CodegenTypeMapping::new(
                types.borrow,
                layout(8, align8),
                CodegenTypeKind::Pointer {
                    target: types.value,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                types.pointer,
                layout(8, align8),
                CodegenTypeKind::Pointer {
                    target: types.value,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                types.usize,
                layout(8, align8),
                CodegenTypeKind::UnsignedInteger(width64),
            ),
            CodegenTypeMapping::new(types.boolean, layout(1, align1), CodegenTypeKind::Boolean),
            CodegenTypeMapping::new(
                types.tag,
                layout(1, align1),
                CodegenTypeKind::UnsignedInteger(width8),
            ),
            CodegenTypeMapping::new(
                types.layout,
                layout(16, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.usize, 0),
                    CodegenFieldLayout::new(None, types.usize, 8),
                ]),
            ),
            CodegenTypeMapping::new(
                types.layout_error,
                layout(1, align1),
                CodegenTypeKind::union(
                    types.tag,
                    [
                        CodegenUnionVariantLayout::new(overflow, IntegerConstant::from_u64(0), []),
                        CodegenUnionVariantLayout::new(
                            unsupported,
                            IntegerConstant::from_u64(1),
                            [],
                        ),
                    ],
                ),
            ),
            CodegenTypeMapping::new(
                types.layout_result,
                layout(24, align8),
                CodegenTypeKind::union(
                    types.tag,
                    [
                        CodegenUnionVariantLayout::new(
                            success,
                            IntegerConstant::from_u64(0),
                            [CodegenFieldLayout::new(None, types.layout, 8)],
                        ),
                        CodegenUnionVariantLayout::new(
                            failure,
                            IntegerConstant::from_u64(1),
                            [CodegenFieldLayout::new(None, types.layout_error, 8)],
                        ),
                    ],
                ),
            ),
            CodegenTypeMapping::new(
                types.allocation,
                layout(24, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.pointer, 0),
                    CodegenFieldLayout::new(None, types.usize, 8),
                    CodegenFieldLayout::new(None, types.usize, 16),
                ]),
            ),
            CodegenTypeMapping::new(
                types.byte,
                layout(1, align1),
                CodegenTypeKind::UnsignedInteger(width8),
            ),
            CodegenTypeMapping::new(
                types.raw_buffer,
                layout(24, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.pointer, 0),
                    CodegenFieldLayout::new(None, types.usize, 8),
                    CodegenFieldLayout::new(None, types.usize, 16),
                ]),
            ),
            CodegenTypeMapping::new(
                types.raw_buffer_borrow,
                layout(8, align8),
                CodegenTypeKind::Pointer {
                    target: types.raw_buffer,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                types.slice,
                layout(16, align8),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.pointer, 0),
                    CodegenFieldLayout::new(None, types.usize, 8),
                ]),
            ),
        ];

        let instance = unit
            .instances()
            .first()
            .unwrap_or_else(|| panic!("memory test unit must contain one instance"));

        let Some(name) = BinarySymbolName::try_new("bray_memory_operation_test") else {
            panic!("memory test symbol must be valid");
        };

        let symbol = CodegenSymbolMapping::new(
            CodegenSymbolKey::Instance(instance.key().clone()),
            name,
            CodegenLinkage::Internal,
            CodegenCallableSignature::new([], CodegenResultMapping::Void, CallableAbi::Bray, false),
        );

        let cleanup_reference = MirHelperReference::Cleanup {
            phase: MirCleanupPhase::LifecycleResolution,
            ty: types.value,
        };

        let operation_mappings = cleanup_operations.into_iter().map(|operation| {
            CodegenOperationMapping::new(
                instance.key().clone(),
                operation,
                [CodegenHelperMapping::lowered(cleanup_reference.clone())],
            )
        });

        let Some(file) = CodegenSourceFile::try_new("memory-operations.bray") else {
            panic!("memory test source file must be valid");
        };

        let debug = CodegenDebugLocation::new(source, file, NonZeroU32::MIN, NonZeroU32::MIN);

        CodegenMappings::try_new(
            unit,
            target,
            type_mappings,
            [],
            [symbol],
            [],
            [],
            [],
            operation_mappings,
            [],
            [debug],
        )
        .unwrap_or_else(|error| panic!("memory test mappings must be valid: {error:?}"))
    }
}
