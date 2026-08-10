use std::sync::Arc;

use bray_codegen::{CodegenFailure, CodegenFieldLayout, CodegenTypeKind};
use bray_ir::{MirMemoryOperation, MirOperation};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, insert_value, int_value, llvm, pointer_value};
use super::support::LoadedMemoryAggregate;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_raw_memory_allocation(
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

    pub(super) fn translate_raw_memory_deallocation(
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

    pub(super) fn translate_owned_memory_allocation(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [layout] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [layout_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (layout, layout_fields) = self.memory_aggregate_operand(layout, *layout_type, 2)?;

        let bytes = self.memory_aggregate_integer(layout, &layout_fields, 0)?;
        let alignment = self.memory_aggregate_integer(layout, &layout_fields, 1)?;
        let result = self.operation_result_type(operation)?;

        let result_fields = self.aggregate_fields(result)?;

        if result_fields.len() != 3 {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let BasicTypeEnum::PointerType(pointer_type) = self.types.map(result_fields[0].ty())?
        else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let function = self.memory_allocation_function(pointer_type);

        let pointer = llvm(self.builder.build_call(
            function,
            &[bytes.into(), alignment.into()],
            "memory.allocate",
        ))?
        .try_as_basic_value()
        .basic()
        .and_then(pointer_value)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mut allocation = self.types.map(result)?.const_zero();

        for (index, value) in [pointer.into(), bytes.into(), alignment.into()]
            .into_iter()
            .enumerate()
        {
            let element = self.aggregate_value_element(&result_fields, index)?;

            allocation = insert_value(&self.builder, allocation, value, element)?;
        }

        Ok(allocation)
    }

    pub(super) fn translate_owned_memory_deallocation(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [allocation] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [allocation_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (allocation, fields) =
            self.memory_aggregate_operand(allocation, *allocation_type, 3)?;

        let pointer = self.memory_aggregate_pointer(allocation, &fields, 0)?;
        let bytes = self.memory_aggregate_integer(allocation, &fields, 1)?;
        let alignment = self.memory_aggregate_integer(allocation, &fields, 2)?;
        let function = self.memory_deallocation_function(pointer.get_type());

        llvm(self.builder.build_call(
            function,
            &[pointer.into(), bytes.into(), alignment.into()],
            "memory.deallocate",
        ))?;

        Ok(())
    }

    pub(super) fn memory_aggregate_operand(
        &mut self,
        operand: &bray_ir::MirOperand,
        ty: bray_symbols::TypeId,
        field_count: usize,
    ) -> Result<(BasicValueEnum<'context>, Arc<[CodegenFieldLayout]>), CodegenFailure> {
        let fields = self.aggregate_fields(ty)?;

        if fields.len() != field_count {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let value = self.operand(operand)?;

        Ok((value, fields))
    }

    pub(super) fn memory_aggregate_integer(
        &self,
        value: BasicValueEnum<'context>,
        fields: &[CodegenFieldLayout],
        index: usize,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        extract_value(&self.builder, value, self.aggregate_element(fields, index)?)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))
            .and_then(|value| self.pointer_sized_integer(value.into()))
    }

    pub(super) fn memory_aggregate_pointer(
        &self,
        value: BasicValueEnum<'context>,
        fields: &[CodegenFieldLayout],
        index: usize,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        extract_value(&self.builder, value, self.aggregate_element(fields, index)?)
            .and_then(|value| pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))
    }

    pub(super) fn load_owned_memory(
        &mut self,
        owner: &bray_ir::MirOperand,
        owner_type: bray_symbols::TypeId,
        field_count: usize,
        name: &str,
    ) -> Result<LoadedMemoryAggregate<'context>, CodegenFailure> {
        let owner = self.memory_pointer(owner)?;

        let value_type = self
            .type_mapping(owner_type)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Pointer { target, .. } => Some(*target),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let fields = self
            .type_mapping(value_type)
            .and_then(|mapping| match mapping.kind() {
                CodegenTypeKind::Aggregate(fields) => Some(fields),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if fields.len() != field_count {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let llvm_type = self.types.map(value_type)?;
        let value = llvm(self.builder.build_load(llvm_type, owner, name))?;

        Ok((owner, llvm_type, value, Arc::clone(fields)))
    }

    pub(super) fn memory_allocation_function(
        &self,
        pointer: inkwell::types::PointerType<'context>,
    ) -> inkwell::values::FunctionValue<'context> {
        let integer = self.pointer_integer_type();
        let ty = pointer.fn_type(&[integer.into(), integer.into()], false);

        self.module
            .get_function(bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL)
            .unwrap_or_else(|| {
                self.module
                    .add_function(bray_runtime_abi::MEMORY_ALLOCATION_SYMBOL, ty, None)
            })
    }

    pub(in crate::translation::unit) fn memory_deallocation_function(
        &self,
        pointer: inkwell::types::PointerType<'context>,
    ) -> inkwell::values::FunctionValue<'context> {
        let integer = self.pointer_integer_type();

        let ty = self
            .types
            .context()
            .void_type()
            .fn_type(&[pointer.into(), integer.into(), integer.into()], false);

        self.module
            .get_function(bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL)
            .unwrap_or_else(|| {
                self.module
                    .add_function(bray_runtime_abi::MEMORY_DEALLOCATION_SYMBOL, ty, None)
            })
    }

    pub(super) fn memory_pointer(
        &mut self,
        operand: &bray_ir::MirOperand,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        self.operand(operand)
            .and_then(|value| pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))
    }

    pub(super) fn pointer_sized_memory_operand(
        &mut self,
        operand: &bray_ir::MirOperand,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let value = self.operand(operand)?;

        self.pointer_sized_integer(value)
    }

    pub(super) fn pointer_address(
        &self,
        pointer: PointerValue<'context>,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        llvm(
            self.builder
                .build_ptr_to_int(pointer, self.pointer_integer_type(), "memory.address"),
        )
    }

    pub(in crate::translation::unit) fn pointer_integer_type(
        &self,
    ) -> inkwell::types::IntType<'context> {
        self.types
            .context()
            .ptr_sized_int_type(self.types.target_data(), None)
    }

    pub(super) fn memory_layout(
        &self,
        ty: bray_symbols::TypeId,
    ) -> Result<bray_target::TargetValueLayout, CodegenFailure> {
        self.type_mapping(ty)
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}
