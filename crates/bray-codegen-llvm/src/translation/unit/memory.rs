use bray_bound_tree::{
    CheckedMemoryOperationKind, MemoryCopyKind, MemoryLayoutQueryKind, MemoryOffsetUnit,
};
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use super::core::UnitTranslator;
use super::support::{
    aggregate_value_element, insert_value, int_value, integer_constant, llvm, pointer_value,
};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_memory(
        &mut self,
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
            CheckedMemoryOperationKind::Allocate => self
                .translate_memory_allocation(operation, memory)
                .map(Some),
            CheckedMemoryOperationKind::Deallocate => {
                self.translate_memory_deallocation(memory)?;

                Ok(None)
            }
        }
    }

    fn ensure_memory_available(
        &self,
        kind: CheckedMemoryOperationKind,
    ) -> Result<(), CodegenFailure> {
        let operations = self.request.target().profile().facts().operations();

        let available = match kind {
            CheckedMemoryOperationKind::LayoutQuery { .. } => true,
            CheckedMemoryOperationKind::Allocate | CheckedMemoryOperationKind::Deallocate => {
                operations.allocation()
            }
            _ => operations.raw_memory(),
        };

        if available {
            Ok(())
        } else {
            Err(CodegenFailure::UnsupportedTarget)
        }
    }

    fn translate_memory_copy(
        &mut self,
        destination: &bray_ir::MirOperand,
        source: &bray_ir::MirOperand,
        count: &bray_ir::MirOperand,
        pointee: bray_symbols::TypeId,
        kind: MemoryCopyKind,
    ) -> Result<(), CodegenFailure> {
        let destination = self.memory_pointer(destination)?;
        let source = self.memory_pointer(source)?;

        let count = self
            .operand(count)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let layout = self.memory_layout(pointee)?;

        let bytes = llvm(self.builder.build_int_mul(
            self.pointer_sized_integer(count.into())?,
            self.pointer_integer_type().const_int(layout.size(), false),
            "memory.copy.bytes",
        ))?;

        let alignment = u32::try_from(layout.alignment().get())
            .map_err(|_| CodegenFailure::UnsupportedTarget)?;

        match kind {
            MemoryCopyKind::NonOverlapping => {
                llvm(
                    self.builder
                        .build_memcpy(destination, alignment, source, alignment, bytes),
                )?;
            }
            MemoryCopyKind::Overlapping => {
                llvm(
                    self.builder
                        .build_memmove(destination, alignment, source, alignment, bytes),
                )?;
            }
        }

        Ok(())
    }

    fn translate_layout_query(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        ty: bray_symbols::TypeId,
        kind: MemoryLayoutQueryKind,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let layout = self.memory_layout(ty)?;

        match kind {
            MemoryLayoutQueryKind::Size | MemoryLayoutQueryKind::Stride => {
                self.layout_integer_result(operation, layout.size())
            }
            MemoryLayoutQueryKind::Alignment => {
                self.layout_integer_result(operation, layout.alignment().get())
            }
            MemoryLayoutQueryKind::Layout => {
                self.translate_allocation_layout(operation, memory, layout)
            }
        }
    }

    fn layout_integer_result(
        &mut self,
        operation: &MirOperation,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation)?;

        let BasicTypeEnum::IntType(result) = self.types.map(result)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(result.const_int(value, false).into())
    }

    fn translate_allocation_layout(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
        layout: bray_target::TargetValueLayout,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [count] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let count = self
            .operand(count)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let count = self.pointer_sized_integer(count.into())?;
        let integer = self.pointer_integer_type();
        let result = self.operation_result_type(operation)?;

        let maximum_alignment = self
            .request
            .target()
            .profile()
            .facts()
            .alignments()
            .max_allocation()
            .get();

        if layout.alignment().get() > maximum_alignment {
            return self.allocation_layout_error(result, 1);
        }

        let stride = layout.size();

        let bytes = llvm(self.builder.build_int_mul(
            count,
            integer.const_int(stride, false),
            "memory.layout.bytes",
        ))?;

        let overflow = if stride == 0 {
            self.types.context().bool_type().const_zero()
        } else {
            let width = integer.get_bit_width();

            if width > 64 {
                return Err(CodegenFailure::UnsupportedTarget);
            }

            let maximum = u64::MAX >> (64 - width);
            let limit = integer.const_int(maximum / stride, false);

            llvm(self.builder.build_int_compare(
                IntPredicate::UGT,
                count,
                limit,
                "memory.layout.overflow",
            ))?
        };

        let layout_type = self.union_payload_type(result, 0)?;

        let layout_value = self.construct_positional_product(
            layout_type,
            &[
                bytes.into(),
                integer.const_int(layout.alignment().get(), false).into(),
            ],
        )?;

        let success = self.construct_positional_union(result, 0, &[layout_value])?;
        let error = self.allocation_layout_error(result, 0)?;

        llvm(
            self.builder
                .build_select(overflow, error, success, "memory.layout.result"),
        )
    }

    fn allocation_layout_error(
        &mut self,
        result: bray_symbols::TypeId,
        error_ordinal: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let error_type = self.union_payload_type(result, 1)?;
        let error = self.construct_positional_union(error_type, error_ordinal, &[])?;

        self.construct_positional_union(result, 1, &[error])
    }

    fn union_payload_type(
        &self,
        ty: bray_symbols::TypeId,
        variant: usize,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        let mapping = self
            .request
            .mappings()
            .ty(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [field] = variants
            .get(variant)
            .map(bray_codegen::CodegenUnionVariantLayout::fields)
            .unwrap_or_default()
        else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(field.ty())
    }

    fn construct_positional_product(
        &mut self,
        ty: bray_symbols::TypeId,
        values: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = self
            .request
            .mappings()
            .ty(ty)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) => Some(fields.clone()),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if fields.len() != values.len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let mut result = self.types.map(ty)?.const_zero();

        for (index, value) in values.iter().copied().enumerate() {
            result = insert_value(
                &self.builder,
                result,
                value,
                aggregate_value_element(self.request.mappings(), &fields, index)?,
            )?;
        }

        Ok(result)
    }

    fn construct_positional_union(
        &mut self,
        ty: bray_symbols::TypeId,
        ordinal: usize,
        values: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (tag, variant) = self
            .request
            .mappings()
            .ty(ty)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Union { tag, variants } => variants
                    .get(ordinal)
                    .cloned()
                    .map(|variant| (*tag, variant)),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if variant.fields().len() != values.len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let llvm_type = self.types.map(ty)?;
        let storage = llvm(self.builder.build_alloca(llvm_type, "memory.union"))?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        llvm(
            self.builder
                .build_store(storage, integer_constant(tag_type, variant.tag())),
        )?;

        for (field, value) in variant.fields().iter().zip(values.iter().copied()) {
            let destination = self.constant_offset_pointer(storage, field.offset_bytes())?;

            llvm(self.builder.build_store(destination, value))?;
        }

        llvm(
            self.builder
                .build_load(llvm_type, storage, "memory.union.value"),
        )
    }

    fn translate_memory_allocation(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [bytes, alignment] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let bytes = self.pointer_sized_memory_operand(bytes)?;
        let alignment = self.pointer_sized_memory_operand(alignment)?;
        let result = self.operation_result_type(operation)?;

        let BasicTypeEnum::PointerType(pointer) = self.types.map(result)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let function = self.memory_allocation_function(pointer);

        llvm(self.builder.build_call(
            function,
            &[bytes.into(), alignment.into()],
            "memory.allocate",
        ))?
        .try_as_basic_value()
        .basic()
        .and_then(pointer_value)
        .map(Into::into)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }

    fn translate_memory_deallocation(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [pointer, bytes, alignment] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let pointer = self.memory_pointer(pointer)?;
        let bytes = self.pointer_sized_memory_operand(bytes)?;
        let alignment = self.pointer_sized_memory_operand(alignment)?;
        let function = self.memory_deallocation_function(pointer.get_type());

        llvm(self.builder.build_call(
            function,
            &[pointer.into(), bytes.into(), alignment.into()],
            "memory.deallocate",
        ))?;

        Ok(())
    }

    fn memory_allocation_function(
        &self,
        pointer: inkwell::types::PointerType<'context>,
    ) -> inkwell::values::FunctionValue<'context> {
        let integer = self.pointer_integer_type();
        let ty = pointer.fn_type(&[integer.into(), integer.into()], false);

        self.module
            .get_function(bray_runtime_interface::MEMORY_ALLOCATION_SYMBOL)
            .unwrap_or_else(|| {
                self.module.add_function(
                    bray_runtime_interface::MEMORY_ALLOCATION_SYMBOL,
                    ty,
                    None,
                )
            })
    }

    fn memory_deallocation_function(
        &self,
        pointer: inkwell::types::PointerType<'context>,
    ) -> inkwell::values::FunctionValue<'context> {
        let integer = self.pointer_integer_type();

        let ty = self.types.context().void_type().fn_type(
            &[pointer.into(), integer.into(), integer.into()],
            false,
        );

        self.module
            .get_function(bray_runtime_interface::MEMORY_DEALLOCATION_SYMBOL)
            .unwrap_or_else(|| {
                self.module.add_function(
                    bray_runtime_interface::MEMORY_DEALLOCATION_SYMBOL,
                    ty,
                    None,
                )
            })
    }

    fn memory_pointer(
        &mut self,
        operand: &bray_ir::MirOperand,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        self.operand(operand)
            .and_then(|value| pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))
    }

    fn pointer_sized_memory_operand(
        &mut self,
        operand: &bray_ir::MirOperand,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let value = self.operand(operand)?;

        self.pointer_sized_integer(value)
    }

    fn pointer_address(
        &self,
        pointer: PointerValue<'context>,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        llvm(
            self.builder
                .build_ptr_to_int(pointer, self.pointer_integer_type(), "memory.address"),
        )
    }

    fn pointer_integer_type(&self) -> inkwell::types::IntType<'context> {
        self.types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None)
    }

    fn memory_layout(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<bray_target::TargetValueLayout, CodegenFailure> {
        self.request
            .mappings()
            .ty(ty)
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
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
        CodegenLinkage, CodegenMappings, CodegenResultMapping, CodegenSourceFile,
        CodegenSymbolKey, CodegenSymbolMapping, CodegenTypeKind, CodegenTypeMapping,
        CodegenUnionVariantLayout, CodegenUnit, TargetAddressSpaceKind,
    };
    use bray_ir::{
        MirBlockKind, MirMemoryOperation, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor,
        MirStorageKind, MirTargetFacts, MirTerminatorKind, MirUnitBuilder, MirUnitKind, MirValueId,
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

    use super::super::super::super::backend::LlvmCodeGenerator;

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
            assert!(ir.contains(spelling), "missing generated LLVM for {spelling}");
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

        let mir_target = MirTargetFacts::new(
            target.profile().clone(),
            RuntimeAbiVersion::new(1, 0),
        );

        let bound = bray_testing::test_bound_unit(171);
        let source = MirSourceAnchor::from(bound.key().source());

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            mir_target,
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .unwrap_or_else(|error| panic!("memory test block must be valid: {error:?}"));

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Local, types.value)
            .unwrap_or_else(|error| panic!("memory test storage must be valid: {error:?}"));

        let place = MirPlace::new(storage, [], types.value);

        let borrowed = builder
            .push_operation(
                entry,
                source.clone(),
                MirOperationKind::Borrow {
                    kind: BorrowKind::Shared,
                    place,
                },
                Some(types.borrow),
            )
            .unwrap_or_else(|error| panic!("memory test borrow must be valid: {error:?}"))
            .result()
            .unwrap_or_else(|| panic!("memory test borrow must produce a value"));

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
        .unwrap_or_else(|| panic!("memory size operation must produce a value"));

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

        let allocation = push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Allocate,
            [MirOperand::Value(size), MirOperand::Value(size)],
            [types.usize, types.usize],
            Some(types.pointer),
        )
        .unwrap_or_else(|| panic!("memory allocation operation must produce a value"));

        push_memory(
            &mut builder,
            entry,
            &source,
            CheckedMemoryOperationKind::Deallocate,
            [
                MirOperand::Value(allocation),
                MirOperand::Value(size),
                MirOperand::Value(size),
            ],
            [types.pointer, types.usize, types.usize],
            None,
        );

        builder
            .set_terminator(entry, source.clone(), MirTerminatorKind::Return(None))
            .unwrap_or_else(|error| panic!("memory test return must be valid: {error:?}"));

        let mir = builder
            .finish(entry)
            .unwrap_or_else(|error| panic!("memory test MIR must be valid: {error:?}"));

        let unit = CodegenUnit::try_new(1, [mir])
            .unwrap_or_else(|error| panic!("memory test codegen unit must be valid: {error:?}"));

        let mappings = memory_mappings(&unit, &target, types, source);

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
    ) -> Option<MirValueId> {
        let operation = MirMemoryOperation::new(kind, operands, operand_types, result_type);

        let committed = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Memory(operation),
                result_type,
            )
            .unwrap_or_else(|error| panic!("memory test operation must be valid: {error:?}"));

        committed.result()
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
        }
    }

    fn memory_target(raw_memory: bool, allocation: bool) -> bray_codegen::CodegenTarget {
        let profile = test_target_profile();

        let facts = profile
            .facts()
            .clone()
            .with_operations(TargetOperationFacts::new(raw_memory, allocation));

        let profile = TargetProfile::try_new(
            profile.identity().clone(),
            profile.machine().clone(),
            facts,
        )
        .unwrap_or_else(|error| panic!("memory test target profile must be valid: {error:?}"));

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
            CodegenTypeMapping::new(
                types.boolean,
                layout(1, align1),
                CodegenTypeKind::Boolean,
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
                        CodegenUnionVariantLayout::new(
                            overflow,
                            IntegerConstant::from_u64(0),
                            [],
                        ),
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

        let Some(file) = CodegenSourceFile::try_new("memory-operations.bray") else {
            panic!("memory test source file must be valid");
        };

        let debug = CodegenDebugLocation::new(source, file, NonZeroU32::MIN, NonZeroU32::MIN);

        CodegenMappings::try_new(
            unit,
            target,
            type_mappings,
            [symbol],
            [],
            [],
            [],
            [],
            [],
            [debug],
        )
        .unwrap_or_else(|error| panic!("memory test mappings must be valid: {error:?}"))
    }
}
