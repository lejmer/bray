use bray_codegen::CodegenFailure;
use bray_ir::{
    MirHelperReference, MirMemoryOperation, MirOperation, MirOperationId, MirStandardLibraryHelper,
};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, llvm, next_helper};
use super::support::LoadedMemoryAggregate;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_raw_buffer_push(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        let [buffer, value] = memory.operands() else {
            panic!("checked raw buffer append retains its owner and value");
        };

        let (buffer, llvm_type, owner, fields) =
            self.load_raw_buffer(buffer, memory.operand_types()[0])?;

        let pointer = self.memory_aggregate_pointer(owner, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(owner, &fields, 2)?;

        let pointer =
            self.dynamic_offset_pointer(pointer, initialized, self.memory_layout(element).size())?;

        let value = self.operand(value)?;

        self.store_value(pointer, value, element)?;

        let initialized = llvm(self.builder.build_int_add(
            initialized,
            self.pointer_integer_type().const_int(1, false),
            "memory.buffer.appended",
        ))?;

        self.store_raw_buffer_initialized_count(
            buffer,
            llvm_type,
            &fields,
            initialized.into(),
            "memory.buffer.initialized",
        )
    }

    pub(super) fn translate_raw_buffer_pop(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            panic!("checked raw buffer removal retains its owner");
        };

        let (buffer, llvm_type, owner, fields) =
            self.load_raw_buffer(buffer, memory.operand_types()[0])?;

        let pointer = self.memory_aggregate_pointer(owner, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(owner, &fields, 2)?;

        let initialized = llvm(self.builder.build_int_sub(
            initialized,
            self.pointer_integer_type().const_int(1, false),
            "memory.buffer.removed",
        ))?;

        let pointer =
            self.dynamic_offset_pointer(pointer, initialized, self.memory_layout(element).size())?;

        let value = self.load_value(pointer, element, "memory.buffer.value")?;

        self.store_raw_buffer_initialized_count(
            buffer,
            llvm_type,
            &fields,
            initialized.into(),
            "memory.buffer.initialized",
        )?;

        Ok(value)
    }

    pub(super) fn translate_raw_buffer_field(
        &mut self,
        memory: &MirMemoryOperation,
        field: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [buffer_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let element = self.aggregate_element(&fields, field)?;

        extract_value(&self.builder, value, element)
    }

    pub(super) fn translate_raw_buffer_slice(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [buffer_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let result = self.operation_result_type(operation);

        self.construct_positional_product(result, &[pointer.into(), initialized.into()])
    }

    pub(super) fn translate_raw_buffer_spare_pointer(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let [buffer] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [buffer_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let (_, _, value, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
        let stride = self.memory_layout(element).size();

        self.dynamic_offset_pointer(pointer, initialized, stride)
    }

    pub(super) fn translate_raw_buffer_set_initialized_count(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [buffer, count] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [buffer_type, _] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let (buffer, llvm_type, _, fields) = self.load_raw_buffer(buffer, *buffer_type)?;

        let count = self.operand(count)?;

        self.store_raw_buffer_initialized_count(
            buffer,
            llvm_type,
            &fields,
            count,
            "memory.buffer.initialized",
        )
    }

    pub(super) fn translate_raw_buffer_release(
        &mut self,
        operation: MirOperationId,
        buffer: &bray_ir::MirOperand,
        buffer_type: bray_symbols::TypeId,
        element: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        // MIR has already resolved the initialized prefix and its retained outcomes.
        let (buffer, llvm_type, value, fields) = self.load_raw_buffer(buffer, buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let capacity = self.memory_aggregate_integer(value, &fields, 1)?;
        let stride = self.memory_layout(element).size();

        let alignment = self
            .pointer_integer_type()
            .const_int(self.memory_layout(element).alignment().get(), false);

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

        // Even the empty branch publishes the initialized checked-call context.
        let outcome = self.checked_call_panic_report_context()?;

        self.set_pending_call_context(outcome)?;

        llvm(
            self.builder
                .build_conditional_branch(present, release, done),
        )?;

        self.builder.position_at_end(release);

        let helpers = self.operation_helpers(operation)?;

        let helper = next_helper(
            &mut helpers.iter(),
            &MirHelperReference::StandardLibrary(MirStandardLibraryHelper::MemoryDeallocate),
        );

        self.invoke_helper_with_panic_report_context(
            helper,
            &[pointer.into(), bytes.into(), alignment.into()],
            outcome,
        )?;

        llvm(self.builder.build_unconditional_branch(done))?;
        self.builder.position_at_end(done);
        llvm(self.builder.build_store(buffer, llvm_type.const_zero()))?;

        Ok(())
    }

    pub(super) fn translate_raw_buffer_replace(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<(), CodegenFailure> {
        let [destination, source] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [_, source_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let destination = self.memory_pointer(destination)?;

        let (source, llvm_type, value, _) = self.load_raw_buffer(source, *source_type)?;

        llvm(self.builder.build_store(destination, value))?;
        llvm(self.builder.build_store(source, llvm_type.const_zero()))?;

        Ok(())
    }

    pub(super) fn translate_raw_buffer_relocate(
        &mut self,
        memory: &MirMemoryOperation,
        element: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        let [source, destination] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [source_type, destination_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let (source, source_llvm_type, source_value, source_fields) =
            self.load_raw_buffer(source, *source_type)?;

        let (destination, destination_llvm_type, destination_value, destination_fields) =
            self.load_raw_buffer(destination, *destination_type)?;

        let source_pointer = self.memory_aggregate_pointer(source_value, &source_fields, 0)?;
        let initialized = self.memory_aggregate_integer(source_value, &source_fields, 2)?;

        let destination_pointer =
            self.memory_aggregate_pointer(destination_value, &destination_fields, 0)?;

        let layout = self.memory_layout(element);

        let bytes = llvm(self.builder.build_int_mul(
            initialized,
            self.pointer_integer_type().const_int(layout.size(), false),
            "memory.buffer.relocate.bytes",
        ))?;

        let alignment =
            crate::conversion::target_value(layout.alignment().get(), "memory_buffer_alignment")?;

        llvm(self.builder.build_memcpy(
            destination_pointer,
            alignment,
            source_pointer,
            alignment,
            bytes,
        ))?;

        self.observe_memory_copy(bytes)?;

        self.store_raw_buffer_initialized_count(
            destination,
            destination_llvm_type,
            &destination_fields,
            initialized.into(),
            "memory.buffer.relocate.destination",
        )?;

        self.store_raw_buffer_initialized_count(
            source,
            source_llvm_type,
            &source_fields,
            self.pointer_integer_type().const_zero().into(),
            "memory.buffer.relocate.source",
        )
    }

    fn store_raw_buffer_initialized_count(
        &self,
        buffer: PointerValue<'context>,
        llvm_type: BasicTypeEnum<'context>,
        fields: &[bray_codegen::CodegenFieldLayout],
        initialized: BasicValueEnum<'context>,
        name: &str,
    ) -> Result<(), CodegenFailure> {
        let BasicTypeEnum::StructType(llvm_type) = llvm_type else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let initialized_field = self.aggregate_element(fields, 2)?;

        let initialized_pointer =
            llvm(
                self.builder
                    .build_struct_gep(llvm_type, buffer, initialized_field, name),
            )?;

        llvm(self.builder.build_store(initialized_pointer, initialized))?;

        Ok(())
    }

    fn load_raw_buffer(
        &mut self,
        buffer: &bray_ir::MirOperand,
        buffer_type: bray_symbols::TypeId,
    ) -> Result<LoadedMemoryAggregate<'context>, CodegenFailure> {
        self.load_owned_memory(buffer, buffer_type, 3, "memory.buffer")
    }
}
