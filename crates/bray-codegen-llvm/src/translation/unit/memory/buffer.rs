use bray_codegen::{CodegenFailure, CodegenHelperMapping};
use bray_ir::{
    MirHelperReference, MirMemoryOperation, MirOperation, MirOperationId, MirStandardLibraryHelper,
};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, llvm, next_helper};
use super::support::LoadedMemoryAggregate;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
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
        assert!(
            self.checked_call_operations.contains(&operation),
            "raw-buffer release requires a checked MIR call outcome"
        );

        let outcome = self.checked_call_panic_report_context()?;
        self.set_pending_call_context(outcome)?;

        let (buffer, llvm_type, value, fields) = self.load_raw_buffer(buffer, buffer_type)?;

        let pointer = self.memory_aggregate_pointer(value, &fields, 0)?;
        let capacity = self.memory_aggregate_integer(value, &fields, 1)?;
        let initialized = self.memory_aggregate_integer(value, &fields, 2)?;
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

        llvm(
            self.builder
                .build_conditional_branch(present, release, done),
        )?;

        self.builder.position_at_end(release);

        let helpers = self.operation_helpers(operation)?;
        let mut helpers = helpers.iter();

        let cleanup = next_helper(
            &mut helpers,
            &MirHelperReference::Cleanup {
                phase: bray_ir::MirCleanupPhase::LifecycleResolution,
                ty: element,
            },
        );

        // Deallocation can use the operation context directly when no element cleanup can fail first.
        let has_destructor = cleanup.symbol().is_some();

        self.destroy_raw_buffer_elements(
            cleanup,
            buffer,
            llvm_type,
            &fields,
            pointer,
            initialized,
            stride,
            outcome,
        )?;

        let deallocate = next_helper(
            &mut helpers,
            &MirHelperReference::StandardLibrary(MirStandardLibraryHelper::MemoryDeallocate),
        );

        self.invoke_buffer_cleanup_helper(
            deallocate,
            &[pointer.into(), bytes.into(), alignment.into()],
            outcome,
            has_destructor,
        )?;

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
        let [destination, source] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let [destination_type, source_type] = memory.operand_types() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        self.translate_raw_buffer_release(operation, destination, *destination_type, element)?;

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

    fn destroy_raw_buffer_elements(
        &mut self,
        helper: &CodegenHelperMapping,
        buffer: PointerValue<'context>,
        llvm_type: BasicTypeEnum<'context>,
        fields: &[bray_codegen::CodegenFieldLayout],
        pointer: PointerValue<'context>,
        initialized: IntValue<'context>,
        stride: u64,
        outcome: PointerValue<'context>,
    ) -> Result<(), CodegenFailure> {
        if helper.symbol().is_none() {
            return Ok(());
        }

        let entry = self
            .builder
            .get_insert_block()
            .expect("checked MIR memory translation requires an established mapping or value");

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

        index.add_incoming(&[(&initialized, entry)]);

        let current = index.as_basic_value().into_int_value();

        let remaining = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            current,
            self.pointer_integer_type().const_zero(),
            "memory.buffer.has_element",
        ))?;

        llvm(self.builder.build_conditional_branch(remaining, body, done))?;
        self.builder.position_at_end(body);

        let previous = llvm(self.builder.build_int_sub(
            current,
            self.pointer_integer_type().const_int(1, false),
            "memory.buffer.previous_index",
        ))?;

        let element = self.dynamic_offset_pointer(pointer, previous, stride)?;

        self.store_raw_buffer_initialized_count(
            buffer,
            llvm_type,
            fields,
            previous.into(),
            "memory.buffer.destroy.progress",
        )?;

        self.invoke_buffer_cleanup_helper(helper, &[element.into()], outcome, true)?;

        let body_end = self
            .builder
            .get_insert_block()
            .expect("checked MIR memory translation requires an established mapping or value");

        llvm(self.builder.build_unconditional_branch(condition))?;
        index.add_incoming(&[(&previous, body_end)]);

        self.builder.position_at_end(done);

        Ok(())
    }

    fn invoke_buffer_cleanup_helper(
        &mut self,
        helper: &CodegenHelperMapping,
        arguments: &[BasicValueEnum<'context>],
        outcome: PointerValue<'context>,
        merge: bool,
    ) -> Result<(), CodegenFailure> {
        let Some(key) = helper.symbol() else {
            return Ok(());
        };

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .expect("raw-buffer cleanup helper must have a mapped symbol");

        if !symbol.signature().has_panic_report_context() {
            self.invoke_helper(helper, arguments)?;

            return Ok(());
        }

        let incident = if merge {
            self.allocate_panic_report_context()?
        } else {
            outcome
        };

        self.invoke_helper_with_panic_report_context(helper, arguments, incident)?;

        if merge {
            crate::translation::merge_pending_outcomes(
                self.module,
                self.types.context(),
                &self.builder,
                self.request.target(),
                self.unit.target().runtime_abi(),
                outcome,
                incident,
            )?;
        }

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
