use bray_bound_tree::{CheckedMemoryOperationKind, MemoryOffsetUnit, VolatileAddressSpace};
use bray_codegen::{CodegenFailure, CodegenTypeKind};
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
            CheckedMemoryOperationKind::RawBufferRelocate { element } => {
                self.translate_raw_buffer_relocate(memory, element)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ByteBufferFill => {
                self.translate_byte_buffer_fill(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ByteSliceCopy => {
                self.translate_byte_slice_copy(memory)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ByteBufferRead => {
                self.translate_byte_buffer_read(operation, memory).map(Some)
            }
            CheckedMemoryOperationKind::SliceLength => {
                self.translate_slice_length(memory).map(Some)
            }
            CheckedMemoryOperationKind::VolatileRead {
                pointee,
                address_space,
                ..
            } => {
                self.translate_volatile_read(memory, pointee, address_space)
                    .map(Some)
            }
            CheckedMemoryOperationKind::VolatileWrite { address_space, .. } => {
                self.translate_volatile_write(memory, address_space)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::ExposeAddress { .. } => {
                self.translate_expose_address(memory).map(Some)
            }
            CheckedMemoryOperationKind::FromExposedAddress { .. } => {
                self.translate_from_exposed_address(operation, memory).map(Some)
            }
            CheckedMemoryOperationKind::CompareAddress { comparison, .. } => {
                self.translate_address_comparison(memory, comparison).map(Some)
            }
            CheckedMemoryOperationKind::Fence {
                compiler_only,
                order,
            } => {
                self.translate_fence(order, compiler_only)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::CatastrophicAbort => {
                self.translate_catastrophic_abort()?;

                Ok(None)
            }
            CheckedMemoryOperationKind::DebuggerTrap => {
                self.translate_debugger_trap()?;

                Ok(None)
            }
            CheckedMemoryOperationKind::UnreachableTermination => Ok(None),
            CheckedMemoryOperationKind::SpinLoopHint => {
                self.translate_spin_loop_hint()?;

                Ok(None)
            }
            CheckedMemoryOperationKind::TargetFeatureEnabled { feature } => {
                self.translate_target_feature(feature).map(Some)
            }
            CheckedMemoryOperationKind::InlineAssembly {
                output, contract, ..
            } => {
                self.translate_inline_assembly(id, operation, memory, contract, output.is_none())
            }
            CheckedMemoryOperationKind::AtomicInitialize { .. }
            | CheckedMemoryOperationKind::AtomicLoad { .. }
            | CheckedMemoryOperationKind::AtomicStore { .. }
            | CheckedMemoryOperationKind::AtomicExchange { .. }
            | CheckedMemoryOperationKind::AtomicCompareExchange { .. }
            | CheckedMemoryOperationKind::AtomicFetch { .. }
            | CheckedMemoryOperationKind::AtomicWait { .. }
            | CheckedMemoryOperationKind::AtomicNotify { .. } => {
                self.translate_atomic_memory(memory)
            }
        }
    }

    fn ensure_memory_available(
        &self,
        kind: CheckedMemoryOperationKind,
    ) -> Result<(), CodegenFailure> {
        let facts = self.request.target().profile().facts();
        let operations = facts.operations();

        let control = bray_target::TargetControlFacts::for_architecture(
            self.request.target().profile().machine().architecture(),
        );

        let available = match kind {
            CheckedMemoryOperationKind::LayoutQuery { .. }
            | CheckedMemoryOperationKind::SliceLength
            | CheckedMemoryOperationKind::CallbackState { .. }
            | CheckedMemoryOperationKind::Fence { .. }
            | CheckedMemoryOperationKind::CatastrophicAbort
            | CheckedMemoryOperationKind::DebuggerTrap
            | CheckedMemoryOperationKind::UnreachableTermination
            | CheckedMemoryOperationKind::SpinLoopHint
            | CheckedMemoryOperationKind::TargetFeatureEnabled { .. } => true,
            CheckedMemoryOperationKind::AtomicInitialize { .. }
            | CheckedMemoryOperationKind::AtomicLoad { .. }
            | CheckedMemoryOperationKind::AtomicStore { .. }
            | CheckedMemoryOperationKind::AtomicExchange { .. }
            | CheckedMemoryOperationKind::AtomicCompareExchange { .. }
            | CheckedMemoryOperationKind::AtomicFetch { .. }
            | CheckedMemoryOperationKind::AtomicWait { .. }
            | CheckedMemoryOperationKind::AtomicNotify { .. } => self.atomic_memory_available(kind)?,
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
            CheckedMemoryOperationKind::RawBufferRelocate { .. } => {
                operations.allocation() && operations.raw_memory()
            }
            CheckedMemoryOperationKind::VolatileRead {
                address_space: VolatileAddressSpace::Device,
                ..
            }
            | CheckedMemoryOperationKind::VolatileWrite {
                address_space: VolatileAddressSpace::Device,
                ..
            } => operations.raw_memory() && facts.address_spaces().device(),
            CheckedMemoryOperationKind::InlineAssembly { .. } => control.inline_assembly(),
            _ => operations.raw_memory(),
        };

        if available {
            Ok(())
        } else {
            Err(CodegenFailure::UnsupportedTarget)
        }
    }

    fn atomic_memory_available(
        &self,
        kind: CheckedMemoryOperationKind,
    ) -> Result<bool, CodegenFailure> {
        use bray_target::TargetAtomicRepresentation as Representation;

        let value = match kind {
            CheckedMemoryOperationKind::AtomicInitialize { value }
            | CheckedMemoryOperationKind::AtomicLoad { value, .. }
            | CheckedMemoryOperationKind::AtomicStore { value, .. }
            | CheckedMemoryOperationKind::AtomicExchange { value, .. }
            | CheckedMemoryOperationKind::AtomicCompareExchange { value, .. }
            | CheckedMemoryOperationKind::AtomicFetch { value, .. }
            | CheckedMemoryOperationKind::AtomicWait { value, .. }
            | CheckedMemoryOperationKind::AtomicNotify { value, .. } => value,
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let mapping = self
            .type_mapping(value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let representation = match mapping.kind() {
            CodegenTypeKind::Boolean
                if matches!(kind, CheckedMemoryOperationKind::AtomicFetch { .. }) =>
            {
                return Ok(false);
            }
            CodegenTypeKind::Boolean => Representation::U8,
            CodegenTypeKind::SignedInteger(width) | CodegenTypeKind::UnsignedInteger(width) => {
                match width.get() {
                    8 => Representation::U8,
                    16 => Representation::U16,
                    32 => Representation::U32,
                    64 => Representation::U64,
                    128 => Representation::U128,
                    _ => return Ok(false),
                }
            }
            CodegenTypeKind::Pointer { .. } => Representation::Pointer,
            CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. } => {
                let Some(representation) = mapping
                    .layout()
                    .and_then(|layout| Representation::for_storage_size(layout.size()))
                else {
                    return Ok(false);
                };

                representation
            }
            _ => return Ok(false),
        };

        let facts = self
            .request
            .target()
            .profile()
            .facts()
            .atomics()
            .representation(representation);

        let operations = facts.operations();

        Ok(match kind {
            CheckedMemoryOperationKind::AtomicInitialize { .. }
            | CheckedMemoryOperationKind::AtomicLoad { .. }
            | CheckedMemoryOperationKind::AtomicStore { .. } => operations.load_store(),
            CheckedMemoryOperationKind::AtomicExchange { .. } => operations.exchange(),
            CheckedMemoryOperationKind::AtomicCompareExchange { .. } => {
                operations.compare_exchange()
            }
            CheckedMemoryOperationKind::AtomicFetch {
                kind: bray_bound_tree::AtomicFetchKind::Add
                    | bray_bound_tree::AtomicFetchKind::Subtract,
                ..
            } => operations.fetch_arithmetic(),
            CheckedMemoryOperationKind::AtomicFetch { .. } => operations.fetch_bitwise(),
            CheckedMemoryOperationKind::AtomicWait { .. }
            | CheckedMemoryOperationKind::AtomicNotify { .. } => facts.wait_notify(),
            _ => false,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

    use bray_bound_tree::{
        AtomicFetchKind, CheckedMemoryOperationKind, MemoryAddressKind, MemoryCopyKind,
        MemoryLayoutQueryKind,
        MemoryOffsetUnit, MemoryOrder, MemoryReadKind, PointerAddressComparison,
        VolatileAddressSpace,
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
        MirPlace, MirSourceAnchor, MirStorageKind, MirTargetFacts, MirTerminatorKind,
        MirUnitBuilder, MirUnitKind, MirValueId,
    };
    use bray_runtime_interface::{BinarySymbolName, RuntimeAbiVersion};
    use bray_symbols::{
        BorrowKind, CallableAbi, IntegerConstant, SemanticValueStore, SymbolId, TypeData, TypeId,
        UnionVariantSymbolId,
    };
    use bray_target::test_support::test_target_profile;
    use bray_target::{
        TargetAtomicFacts, TargetAtomicOperationFacts, TargetAtomicRepresentationFacts,
        TargetAddressSpaceFacts, TargetFacts, TargetLayoutContract, TargetOperationFacts,
        TargetProfile, TargetValueLayout,
    };
    use inkwell::context::Context;

    use crate::backend::LlvmCodeGenerator;

    #[derive(Clone, Copy)]
    struct MemoryTypes {
        value: TypeId,
        aligned_value: TypeId,
        borrow: TypeId,
        pointer: TypeId,
        device_pointer: TypeId,
        usize: TypeId,
        boolean: TypeId,
        compare_exchange_result: TypeId,
        atomic_value: TypeId,
        atomic_storage: TypeId,
        atomic_pointer: TypeId,
        atomic_compare_exchange_result: TypeId,
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

        assert!(module.verify().is_ok(), "{ir}");

        for spelling in [
            "llvm.memcpy",
            "llvm.memmove",
            "load volatile",
            "store volatile",
            "ptrtoint",
            "inttoptr",
            "llvm.debugtrap",
            "pause",
            bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL,
            bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL,
        ] {
            assert!(
                ir.contains(spelling),
                "missing generated LLVM for {spelling}"
            );
        }

        assert!(
            ir.lines()
                .any(|line| line.contains("load volatile i32, ptr addrspace(5)")),
            "device volatile load did not retain target address space 5: {ir}"
        );

        assert!(
            ir.lines().any(|line| {
                line.contains("store volatile i32") && line.contains("ptr addrspace(5)")
            }),
            "device volatile store did not retain target address space 5: {ir}"
        );

        for spelling in [
            "load atomic i32",
            "store atomic i32",
            "atomicrmw add",
            "atomicrmw xchg",
            "cmpxchg",
            "atomic.wait",
        ] {
            assert!(ir.contains(spelling), "missing atomic LLVM for {spelling}");
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

        let relocation = ir
            .lines()
            .find(|line| {
                line.contains("@llvm.memcpy")
                    && line.contains("%memory.buffer.relocate.bytes")
            })
            .unwrap_or_else(|| panic!("missing raw-buffer relocation memcpy"));

        assert!(
            relocation.matches("align 64").count() >= 2,
            "raw-buffer relocation did not preserve over-aligned source and destination storage: {relocation}"
        );

        assert!(
            relocation.contains("%memory.buffer.relocate.bytes"),
            "raw-buffer relocation must use the initialized element byte count in a non-overlapping memcpy: {relocation}"
        );

        assert!(
            ir.lines().any(|line| {
                line.contains("memory.buffer.relocate.bytes")
                    && line.contains("mul i64")
                    && line.ends_with(", 64")
            }),
            "raw-buffer relocation did not multiply the initialized count by the over-aligned element size"
        );

        let deallocations = ir
            .lines()
            .filter(|line| {
                line.contains("call void")
                    && line.contains(bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL)
            })
            .count();

        assert_eq!(deallocations, 4);
    }

    #[test]
    fn observed_memory_operations_record_only_after_the_effect() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = memory_operation_fixture(&backend)
            .with_runtime_observations(bray_codegen::RuntimeObservationMode::Memory);

        let context = Context::create();

        let (_machine, module) = backend
            .prepare_module(fixture.request(), &context)
            .unwrap_or_else(|error| panic!("observed memory LLVM must generate: {error:?}"))
            .unwrap_or_else(|| panic!("observed memory LLVM must not be cancelled"));

        let ir = module.print_to_string().to_string();
        let allocation = position(&ir, bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL);

        let allocation_observation = position(
            &ir,
            bray_runtime_abi::MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
        );

        let relocation = ir
            .find("memory.buffer.relocate.bytes")
            .unwrap_or_else(|| panic!("observed memory LLVM must relocate storage"));

        let copy_observation = ir[relocation..]
            .find(bray_runtime_abi::MEMORY_COPY_OBSERVATION_SYMBOL)
            .map(|position| relocation + position)
            .unwrap_or_else(|| panic!("observed memory LLVM must record relocation"));

        assert!(allocation < allocation_observation);
        assert!(relocation < copy_observation);
        assert!(!ir.contains(bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL));
        assert!(!ir.contains(bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL));
    }

    fn position(ir: &str, symbol: &str) -> usize {
        ir.match_indices(symbol)
            .find(|(position, _)| ir[..*position].rsplit_once('\n').is_some_and(|(_, line)| line.contains("call")))
            .map(|(position, _)| position)
            .unwrap_or_else(|| panic!("observed memory LLVM is missing {symbol}"))
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

    #[test]
    fn aggregate_atomic_values_use_integer_storage_and_restore_semantic_values() {
        let Ok(backend) = LlvmCodeGenerator::try_new() else {
            panic!("LLVM backend constants must be valid");
        };

        let fixture = memory_operation_fixture(&backend);
        let context = Context::create();

        let (_machine, module) = backend
            .prepare_module(fixture.request(), &context)
            .unwrap_or_else(|error| panic!("aggregate atomic LLVM generation must succeed: {error:?}"))
            .unwrap_or_else(|| panic!("aggregate atomic LLVM generation must not be cancelled"));

        let ir = module.print_to_string().to_string();

        for spelling in [
            "atomic.value.storage",
            "atomic.value.bits",
            "atomic.bits.storage",
            "atomic.bits.value",
        ] {
            assert!(ir.contains(spelling), "missing aggregate atomic LLVM for {spelling}");
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

        push_atomic_operations(&mut builder, entry, &source, types, address, read);
        push_aggregate_atomic_operations(&mut builder, entry, &source, types, read);

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

        let volatile_read = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::VolatileRead {
                pointee: types.value,
                address_space: VolatileAddressSpace::Host,
                kind: MemoryReadKind::Copy,
            },
            [MirOperand::Value(address)],
            [types.pointer],
            Some(types.value),
        )
        .result()
        .unwrap_or_else(|| panic!("volatile read must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::VolatileWrite {
                pointee: types.value,
                address_space: VolatileAddressSpace::Host,
            },
            [
                MirOperand::Value(address),
                MirOperand::Value(volatile_read),
            ],
            [types.pointer, types.value],
            None,
        );

        let device_pointer = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Null {
                pointee: types.value,
            },
            [],
            [],
            Some(types.device_pointer),
        )
        .result()
        .unwrap_or_else(|| panic!("device null operation must produce a value"));

        let device_value = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::VolatileRead {
                pointee: types.value,
                address_space: VolatileAddressSpace::Device,
                kind: MemoryReadKind::Copy,
            },
            [MirOperand::Value(device_pointer)],
            [types.device_pointer],
            Some(types.value),
        )
        .result()
        .unwrap_or_else(|| panic!("device volatile read must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::VolatileWrite {
                pointee: types.value,
                address_space: VolatileAddressSpace::Device,
            },
            [
                MirOperand::Value(device_pointer),
                MirOperand::Value(device_value),
            ],
            [types.device_pointer, types.value],
            None,
        );

        let exposed = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::ExposeAddress {
                pointee: types.value,
            },
            [MirOperand::Value(address)],
            [types.pointer],
            Some(types.usize),
        )
        .result()
        .unwrap_or_else(|| panic!("address exposure must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::FromExposedAddress {
                pointee: types.value,
            },
            [MirOperand::Value(exposed)],
            [types.usize],
            Some(types.pointer),
        );

        for comparison in [PointerAddressComparison::Equal, PointerAddressComparison::Less] {
            push_memory(
                &mut builder,
                entry,
                &source,
                CheckedMemoryOperationKind::CompareAddress {
                    pointee: types.value,
                    comparison,
                },
                [MirOperand::Value(address), MirOperand::Value(null)],
                [types.pointer, types.pointer],
                Some(types.boolean),
            );
        }

        for kind in [
            CheckedMemoryOperationKind::Fence {
                compiler_only: true,
                order: MemoryOrder::SequentiallyConsistent,
            },
            CheckedMemoryOperationKind::DebuggerTrap,
            CheckedMemoryOperationKind::SpinLoopHint,
        ] {
            push_memory(&mut builder, entry, &source, kind, [], [], None);
        }

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

        let byte_slice = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferInitializedSlice,
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
            CheckedMemoryOperationKind::ByteSliceCopy,
            [MirOperand::Value(byte_slice), MirOperand::Value(null)],
            [types.slice, types.pointer],
            None,
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::RawBufferRelocate {
                element: types.aligned_value,
            },
            [
                MirOperand::Value(buffer),
                MirOperand::Value(source_buffer),
            ],
            [types.raw_buffer_borrow, types.raw_buffer_borrow],
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
            ],
            [
                types.raw_buffer_borrow,
                types.raw_buffer_borrow,
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
        let aligned_value = intern_type(&store, TypeData::tuple([value, value]));
        let borrow = intern_type(&store, TypeData::tuple([value]));
        let pointer = intern_type(&store, TypeData::tuple([borrow]));
        let device_pointer = intern_type(&store, TypeData::tuple([pointer]));
        let usize = intern_type(&store, TypeData::tuple([device_pointer]));
        let boolean = intern_type(&store, TypeData::tuple([usize]));
        let compare_exchange_result = intern_type(&store, TypeData::tuple([value, boolean]));
        let atomic_value = intern_type(&store, TypeData::tuple([value, value, value]));
        let atomic_storage = intern_type(&store, TypeData::tuple([atomic_value, value]));
        let atomic_pointer = intern_type(&store, TypeData::tuple([atomic_storage, value]));

        let atomic_compare_exchange_result =
            intern_type(&store, TypeData::tuple([atomic_value, boolean, value]));

        let tag = intern_type(
            &store,
            TypeData::tuple([compare_exchange_result, atomic_compare_exchange_result]),
        );

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
            aligned_value,
            borrow,
            pointer,
            device_pointer,
            usize,
            boolean,
            compare_exchange_result,
            atomic_value,
            atomic_storage,
            atomic_pointer,
            atomic_compare_exchange_result,
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

        let baseline = profile.facts();

        let address_spaces = TargetAddressSpaceFacts::try_new(true, true)
            .unwrap_or_else(|| panic!("memory test address spaces must be valid"));

        let facts = TargetFacts::new(
            baseline.identity().clone(),
            baseline.scalars(),
            test_atomic_facts(),
            baseline.abis(),
            baseline.c_abi(),
            address_spaces,
            baseline.alignments(),
            TargetOperationFacts::new(raw_memory, allocation),
        );

        let profile =
            TargetProfile::try_new(profile.identity().clone(), profile.machine().clone(), facts)
                .unwrap_or_else(|error| {
                    panic!("memory test target profile must be valid: {error:?}")
                });

        codegen_target_with_profile(profile, "x86_64-unknown-linux-gnu")
    }

    fn push_atomic_operations(
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        types: MemoryTypes,
        address: MirValueId,
        value: MirValueId,
    ) {
        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicLoad {
                value: types.value,
                order: MemoryOrder::Acquire,
            },
            [MirOperand::Value(address)],
            [types.pointer],
            Some(types.value),
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicExchange {
                value: types.value,
                order: MemoryOrder::SequentiallyConsistent,
            },
            [MirOperand::Value(address), MirOperand::Value(value)],
            [types.pointer, types.value],
            Some(types.value),
        );

        for weak in [false, true] {
            push_memory(
                builder,
                block,
                source,
                CheckedMemoryOperationKind::AtomicCompareExchange {
                    value: types.value,
                    weak,
                    success: MemoryOrder::AcquireRelease,
                    failure: MemoryOrder::Acquire,
                },
                [
                    MirOperand::Value(address),
                    MirOperand::Value(value),
                    MirOperand::Value(value),
                ],
                [types.pointer, types.value, types.value],
                Some(types.compare_exchange_result),
            );
        }

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicStore {
                value: types.value,
                order: MemoryOrder::Release,
            },
            [MirOperand::Value(address), MirOperand::Value(value)],
            [types.pointer, types.value],
            None,
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicWait {
                value: types.value,
                order: MemoryOrder::Acquire,
            },
            [MirOperand::Value(address), MirOperand::Value(value)],
            [types.pointer, types.value],
            None,
        );

        for all in [false, true] {
            push_memory(
                builder,
                block,
                source,
                CheckedMemoryOperationKind::AtomicNotify {
                    value: types.value,
                    all,
                },
                [MirOperand::Value(address)],
                [types.pointer],
                None,
            );
        }

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicFetch {
                value: types.value,
                kind: AtomicFetchKind::Add,
                order: MemoryOrder::AcquireRelease,
            },
            [MirOperand::Value(address), MirOperand::Value(value)],
            [types.pointer, types.value],
            Some(types.value),
        );

    }

    fn push_aggregate_atomic_operations(
        builder: &mut MirUnitBuilder,
        block: bray_ir::MirBlockId,
        source: &MirSourceAnchor,
        types: MemoryTypes,
        value: MirValueId,
    ) {
        let aggregate = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Aggregate(MirAggregate::new(
                    MirAggregateKind::Tuple,
                    [MirOperand::Value(value)],
                )),
                Some(types.atomic_value),
            )
            .unwrap_or_else(|error| panic!("atomic aggregate fixture must be valid: {error:?}"))
            .result()
            .unwrap_or_else(|| panic!("atomic aggregate fixture must produce a value"));

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicInitialize {
                value: types.atomic_value,
            },
            [MirOperand::Value(aggregate)],
            [types.atomic_value],
            Some(types.atomic_storage),
        );

        let address = push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::Null {
                pointee: types.atomic_value,
            },
            [],
            [],
            Some(types.atomic_pointer),
        )
        .result()
        .unwrap_or_else(|| panic!("atomic aggregate address must produce a value"));

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicLoad {
                value: types.atomic_value,
                order: MemoryOrder::Acquire,
            },
            [MirOperand::Value(address)],
            [types.atomic_pointer],
            Some(types.atomic_value),
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicExchange {
                value: types.atomic_value,
                order: MemoryOrder::AcquireRelease,
            },
            [MirOperand::Value(address), MirOperand::Value(aggregate)],
            [types.atomic_pointer, types.atomic_value],
            Some(types.atomic_value),
        );

        push_memory(
            builder,
            block,
            source,
            CheckedMemoryOperationKind::AtomicCompareExchange {
                value: types.atomic_value,
                weak: false,
                success: MemoryOrder::AcquireRelease,
                failure: MemoryOrder::Acquire,
            },
            [
                MirOperand::Value(address),
                MirOperand::Value(aggregate),
                MirOperand::Value(aggregate),
            ],
            [
                types.atomic_pointer,
                types.atomic_value,
                types.atomic_value,
            ],
            Some(types.atomic_compare_exchange_result),
        );
    }

    fn test_atomic_facts() -> TargetAtomicFacts {
        let unavailable = |alignment| TargetAtomicRepresentationFacts::unavailable(alignment);

        let available = TargetAtomicRepresentationFacts::try_new(
            TargetAtomicOperationFacts::integer(),
            NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN),
            true,
            true,
            true,
        )
        .unwrap_or_else(|| panic!("atomic test facts must be valid"));

        TargetAtomicFacts::new(
            unavailable(NonZeroU64::MIN),
            unavailable(NonZeroU64::new(2).unwrap_or(NonZeroU64::MIN)),
            available,
            unavailable(NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN)),
            unavailable(NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN)),
            unavailable(NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN)),
        )
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
        let align8 = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let align4 = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);
        let align64 = NonZeroU64::new(64).unwrap_or(NonZeroU64::MIN);
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
                types.aligned_value,
                layout(64, align64),
                CodegenTypeKind::aggregate([CodegenFieldLayout::new(None, types.value, 0)]),
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
                types.device_pointer,
                layout(8, align8),
                CodegenTypeKind::Pointer {
                    target: types.value,
                    address_space: TargetAddressSpaceKind::Device,
                },
            ),
            CodegenTypeMapping::new(
                types.usize,
                layout(8, align8),
                CodegenTypeKind::UnsignedInteger(width64),
            ),
            CodegenTypeMapping::new(types.boolean, layout(1, align1), CodegenTypeKind::Boolean),
            CodegenTypeMapping::new(
                types.compare_exchange_result,
                layout(8, align4),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.value, 0),
                    CodegenFieldLayout::new(None, types.boolean, 4),
                ]),
            ),
            CodegenTypeMapping::new(
                types.atomic_value,
                layout(4, align4),
                CodegenTypeKind::aggregate([CodegenFieldLayout::new(None, types.value, 0)]),
            ),
            CodegenTypeMapping::new(
                types.atomic_storage,
                layout(4, align4),
                CodegenTypeKind::UnsignedInteger(width32),
            ),
            CodegenTypeMapping::new(
                types.atomic_pointer,
                layout(8, align8),
                CodegenTypeKind::Pointer {
                    target: types.atomic_storage,
                    address_space: TargetAddressSpaceKind::Default,
                },
            ),
            CodegenTypeMapping::new(
                types.atomic_compare_exchange_result,
                layout(8, align4),
                CodegenTypeKind::aggregate([
                    CodegenFieldLayout::new(None, types.atomic_value, 0),
                    CodegenFieldLayout::new(None, types.boolean, 4),
                ]),
            ),
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
