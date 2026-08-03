use std::sync::Arc;

use bray_bound_tree::CheckedMemoryOperationKind;
use bray_codegen::{CodegenFailure, CodegenFieldLayout};
use bray_ir::{MirMemoryOperation, MirOperation, MirOperationId};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::{
    aggregate_element, aggregate_value_element, extract_value, insert_value, llvm,
};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_raw_buffer_field(
        &mut self,
        memory: &MirMemoryOperation,
        field: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let element = aggregate_element(self.request.mappings(), &fields, field)?;

        extract_value(&self.builder, value, element)
    }

    pub(super) fn translate_raw_buffer_slice(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let result = self.operation_result_type(operation)?;

        self.construct_positional_product(result, &[pointer.into(), initialized.into()])
    }

    pub(super) fn translate_raw_buffer_spare_pointer(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let stride = self.memory_layout(element)?.size();

        self.dynamic_offset_pointer(pointer, initialized, stride)
    }

    pub(super) fn translate_raw_buffer_set_initialized_count(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [buffer, count] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type, _] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (buffer, llvm_type, _, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let BasicTypeEnum::StructType(llvm_type) = llvm_type else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let initialized = aggregate_element(self.request.mappings(), &fields, 2)?;
        let count = self.operand(count)?;

        let initialized = llvm(self.builder.build_struct_gep(
            llvm_type,
            buffer,
            initialized,
            "memory.buffer.initialized",
        ))?;

        llvm(self.builder.build_store(initialized, count))?;

        Ok(())
    }

    pub(super) fn translate_raw_buffer_release(
        &mut self,
        operation: MirOperationId,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        let [buffer] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [buffer_type] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (buffer, llvm_type, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let capacity = self.memory_aggregate_integer(value, &fields, 1)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let stride = self.memory_layout(element)?.size();

        let alignment = self
            .pointer_integer_type()
            .const_int(self.memory_layout(element)?.alignment().get(), false);

        let bytes = llvm(self.builder.build_int_mul(
            capacity,
            self.pointer_integer_type().const_int(stride, false),
            "memory.buffer.bytes",
        ))?;

        let present = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            capacity,
            self.pointer_integer_type().const_zero(),
            "memory.buffer.owns_storage",
        ))?;

        let release = self
            .types
            .context()
            .append_basic_block(self.function, "memory.buffer.release");

        let done = self
            .types
            .context()
            .append_basic_block(self.function, "memory.buffer.released");

        llvm(
            self.builder
                .build_conditional_branch(present, release, done),
        )?;

        self.builder.position_at_end(release);

        self.destroy_raw_buffer_elements(operation, pointer, initialized, stride)?;

        let function = self.memory_deallocation_function(pointer.get_type());

        llvm(self.builder.build_call(
            function,
            &[pointer.into(), bytes.into(), alignment.into()],
            "memory.buffer.release",
        ))?;

        llvm(self.builder.build_unconditional_branch(done))?;
        self.builder.position_at_end(done);

        llvm(self.builder.build_store(buffer, llvm_type.const_zero()))?;

        Ok(())
    }

    pub(super) fn translate_raw_buffer_replace(
        &mut self,
        operation: MirOperationId,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        let [destination, source, initialized] = memory.operands() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [destination_type, source_type, _] = memory.operand_types() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let release = MirMemoryOperation::new(
            CheckedMemoryOperationKind::RawBufferRelease { element },
            [destination.clone()],
            [*destination_type],
            None,
        );

        self.translate_raw_buffer_release(operation, &release, element)?;

        let destination = self.memory_pointer(destination)?;

        let (source, llvm_type, value, fields) = self.load_raw_buffer(source, *source_type)?;

        let initialized_field = aggregate_value_element(self.request.mappings(), &fields, 2)?;
        let initialized = self.operand(initialized)?;
        let value = insert_value(&self.builder, value, initialized, initialized_field)?;

        llvm(self.builder.build_store(destination, value))?;
        llvm(self.builder.build_store(source, llvm_type.const_zero()))?;

        Ok(())
    }

    fn destroy_raw_buffer_elements(
        &mut self,
        operation: MirOperationId,
        pointer: PointerValue<'context>,
        initialized: IntValue<'context>,
        stride: u64,
    ) -> Result<(), CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [helper] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.symbol().is_none() {
            return Ok(());
        }

        let entry = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let condition = self
            .types
            .context()
            .append_basic_block(self.function, "memory.buffer.destroy.condition");

        let body = self
            .types
            .context()
            .append_basic_block(self.function, "memory.buffer.destroy.element");

        let done = self
            .types
            .context()
            .append_basic_block(self.function, "memory.buffer.destroy.done");

        llvm(self.builder.build_unconditional_branch(condition))?;
        self.builder.position_at_end(condition);

        let index = llvm(
            self.builder
                .build_phi(self.pointer_integer_type(), "memory.buffer.index"),
        )?;

        index.add_incoming(&[(&self.pointer_integer_type().const_zero(), entry)]);

        let current = index.as_basic_value().into_int_value();

        let remaining = llvm(self.builder.build_int_compare(
            IntPredicate::ULT,
            current,
            initialized,
            "memory.buffer.has_element",
        ))?;

        llvm(self.builder.build_conditional_branch(remaining, body, done))?;
        self.builder.position_at_end(body);

        let element = self.dynamic_offset_pointer(pointer, current, stride)?;

        self.invoke_helper(helper, &[element.into()])?;

        let next = llvm(self.builder.build_int_add(
            current,
            self.pointer_integer_type().const_int(1, false),
            "memory.buffer.next_index",
        ))?;

        let body_end = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        llvm(self.builder.build_unconditional_branch(condition))?;
        index.add_incoming(&[(&next, body_end)]);

        self.builder.position_at_end(done);

        Ok(())
    }

    fn load_raw_buffer(
        &mut self,
        buffer: &bray_ir::MirOperand,
        buffer_type: bray_symbols::TypeId,
    ) -> Result<
        (
            PointerValue<'context>,
            BasicTypeEnum<'context>,
            BasicValueEnum<'context>,
            Arc<[CodegenFieldLayout]>,
        ),
        CodegenFailure,
    > {
        self.load_owned_memory(buffer, buffer_type, 3, "memory.buffer")
    }
}
