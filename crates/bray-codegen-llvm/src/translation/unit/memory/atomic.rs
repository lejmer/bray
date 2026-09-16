use bray_bound_tree::{AtomicFetchKind, CheckedMemoryOperationKind, MemoryOrder};
use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::MirMemoryOperation;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValue, BasicValueEnum, IntValue, PointerValue};
use inkwell::{AtomicRMWBinOp, IntPredicate};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, insert_value, int_value, llvm, pointer_value};
use super::target::llvm_memory_order;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in super::super) fn translate_atomic_memory(
        &mut self,
        operation: &MirMemoryOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        match operation.kind() {
            CheckedMemoryOperationKind::AtomicInitialize { value: value_type } => {
                let [value_operand] = operation.operands() else {
                    panic!(
                        "checked MIR memory translation violated an established compiler contract"
                    );
                };

                let value = self.operand(value_operand)?;

                self.atomic_encode_value(value, value_type).map(Some)
            }
            CheckedMemoryOperationKind::AtomicLoad { value, order } => {
                self.atomic_load(operation, value, order).map(Some)
            }
            CheckedMemoryOperationKind::AtomicStore {
                value: value_type,
                order,
            } => {
                let [storage, value_operand] = operation.operands() else {
                    panic!(
                        "checked MIR memory translation violated an established compiler contract"
                    );
                };

                let storage = self.memory_pointer(storage)?;
                let value = self.operand(value_operand)?;
                let value = self.atomic_encode_value(value, value_type)?;
                let instruction = llvm(self.builder.build_store(storage, value))?;

                instruction
                    .set_atomic_ordering(llvm_memory_order(order))
                    .map_err(CodegenFailure::backend_library)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::AtomicExchange { value, order } => self
                .atomic_read_modify_write(operation, value, AtomicRMWBinOp::Xchg, order)
                .map(Some),
            CheckedMemoryOperationKind::AtomicCompareExchange {
                weak: _,
                success,
                failure,
                value,
            } => {
                let [storage, expected, desired] = operation.operands() else {
                    panic!(
                        "checked MIR memory translation violated an established compiler contract"
                    );
                };

                let storage = self.memory_pointer(storage)?;
                let expected = self.operand(expected)?;
                let desired = self.operand(desired)?;
                let expected = self.atomic_encode_value(expected, value)?;
                let desired = self.atomic_encode_value(desired, value)?;

                let result = llvm(self.builder.build_cmpxchg(
                    storage,
                    expected,
                    desired,
                    llvm_memory_order(success),
                    llvm_memory_order(failure),
                ))?;

                self.atomic_decode_compare_exchange(operation, value, result.into())
                    .map(Some)
            }
            CheckedMemoryOperationKind::AtomicFetch { value, kind, order } => self
                .atomic_read_modify_write(operation, value, llvm_fetch_kind(kind), order)
                .map(Some),
            CheckedMemoryOperationKind::AtomicWait { value, order } => {
                self.atomic_wait(operation, value, order)?;

                Ok(None)
            }
            CheckedMemoryOperationKind::AtomicNotify { .. } => Ok(None),
            unexpected => panic!(
                "checked MIR memory translation violated an established compiler contract: {unexpected:?}"
            ),
        }
    }

    fn atomic_load(
        &mut self,
        operation: &MirMemoryOperation,
        value: bray_symbols::TypeId,
        order: MemoryOrder,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [storage] = operation.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let storage = self.memory_pointer(storage)?;
        let loaded = self.atomic_load_storage(storage, value, order)?;

        self.atomic_decode_value(loaded, value)
    }

    fn atomic_load_storage(
        &mut self,
        storage: PointerValue<'context>,
        value: bray_symbols::TypeId,
        order: MemoryOrder,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let storage_type = self.atomic_storage_type(value)?;

        let loaded = llvm(
            self.builder
                .build_load(storage_type, storage, "atomic.load"),
        )?;

        let instruction = loaded
            .as_instruction_value()
            .expect("checked MIR memory translation requires an established mapping or value");

        instruction
            .set_atomic_ordering(llvm_memory_order(order))
            .map_err(CodegenFailure::backend_library)?;

        Ok(loaded)
    }

    fn atomic_read_modify_write(
        &mut self,
        operation: &MirMemoryOperation,
        value_type: bray_symbols::TypeId,
        kind: AtomicRMWBinOp,
        order: MemoryOrder,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [storage, value] = operation.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let storage = self.memory_pointer(storage)?;
        let value = self.operand(value)?;
        let value = self.atomic_encode_value(value, value_type)?;

        let (integer, pointer_type) = self.atomic_integer(value)?;

        let previous =
            llvm(
                self.builder
                    .build_atomicrmw(kind, storage, integer, llvm_memory_order(order)),
            )?;

        match pointer_type {
            Some(pointer_type) => llvm(self.builder.build_int_to_ptr(
                previous,
                pointer_type,
                "atomic.pointer.previous",
            ))
            .map(BasicValueEnum::from),
            None => {
                let expected = self.atomic_storage_type(value_type)?;

                if previous.get_type().as_basic_type_enum() != expected {
                    panic!(
                        "checked MIR memory translation violated an established compiler contract"
                    );
                }

                self.atomic_decode_value(previous.into(), value_type)
            }
        }
    }

    fn atomic_integer(
        &self,
        value: BasicValueEnum<'context>,
    ) -> Result<
        (
            IntValue<'context>,
            Option<inkwell::types::PointerType<'context>>,
        ),
        CodegenFailure,
    > {
        if let Some(value) = int_value(value) {
            return Ok((value, None));
        }

        let pointer = pointer_value(value)
            .expect("checked MIR memory translation requires an established mapping or value");

        let integer = llvm(self.builder.build_ptr_to_int(
            pointer,
            self.pointer_integer_type(),
            "atomic.pointer.value",
        ))?;

        Ok((integer, Some(pointer.get_type())))
    }

    fn atomic_wait(
        &mut self,
        operation: &MirMemoryOperation,
        value: bray_symbols::TypeId,
        order: MemoryOrder,
    ) -> Result<(), CodegenFailure> {
        let [storage, expected] = operation.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let storage = self.memory_pointer(storage)?;
        let expected = self.operand(expected)?;
        let expected = self.atomic_encode_value(expected, value)?;

        let waiting = self
            .types
            .context()
            .append_basic_block(self.function, "atomic.wait");

        let complete = self
            .types
            .context()
            .append_basic_block(self.function, "atomic.wait.complete");

        llvm(self.builder.build_unconditional_branch(waiting))?;
        self.builder.position_at_end(waiting);

        let observed = self.atomic_load_storage(storage, value, order)?;
        let equal = self.atomic_values_equal(observed, expected)?;

        llvm(
            self.builder
                .build_conditional_branch(equal, waiting, complete),
        )?;

        self.builder.position_at_end(complete);

        Ok(())
    }

    fn atomic_storage_type(
        &mut self,
        value_type: bray_symbols::TypeId,
    ) -> Result<BasicTypeEnum<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(value_type)
            .expect("checked MIR memory translation requires an established mapping or value");

        match mapping.kind() {
            CodegenTypeKind::Boolean => Ok(self.types.context().i8_type().into()),
            CodegenTypeKind::SignedInteger(_)
            | CodegenTypeKind::UnsignedInteger(_)
            | CodegenTypeKind::Pointer { .. } => self.types.map(value_type),
            CodegenTypeKind::Aggregate(_)
            | CodegenTypeKind::Array { .. }
            | CodegenTypeKind::Union { .. } => {
                let bits = mapping
                    .layout()
                    .and_then(|layout| layout.size().checked_mul(8))
                    .and_then(|bits| u32::try_from(bits).ok())
                    .and_then(std::num::NonZeroU32::new)
                    .expect(
                        "checked MIR memory translation requires an established mapping or value",
                    );

                self.types
                    .context()
                    .custom_width_int_type(bits)
                    .map(BasicTypeEnum::from)
                    .map_err(CodegenFailure::backend_library)
            }
            unexpected => panic!(
                "checked MIR memory translation violated an established compiler contract: {unexpected:?}"
            ),
        }
    }

    fn atomic_encode_value(
        &mut self,
        value: BasicValueEnum<'context>,
        value_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let storage_type = self.atomic_storage_type(value_type)?;

        if value.get_type() == storage_type {
            return Ok(value);
        }

        if self.atomic_value_is_boolean(value_type) {
            let value = int_value(value)
                .expect("checked MIR memory translation requires an established mapping or value");

            return llvm(self.builder.build_int_z_extend(
                value,
                self.types.context().i8_type(),
                "atomic.boolean.storage",
            ))
            .map(BasicValueEnum::from);
        }

        crate::translation::reinterpret_value(
            self.types.context(),
            &self.builder,
            value,
            storage_type,
            storage_type,
            "atomic.value.bits",
        )
    }

    fn atomic_decode_value(
        &mut self,
        value: BasicValueEnum<'context>,
        value_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let semantic_type = self.types.map(value_type)?;

        if value.get_type() == semantic_type {
            return Ok(value);
        }

        if self.atomic_value_is_boolean(value_type) {
            let value = int_value(value)
                .expect("checked MIR memory translation requires an established mapping or value");

            return llvm(self.builder.build_int_truncate(
                value,
                self.types.context().bool_type(),
                "atomic.boolean.value",
            ))
            .map(BasicValueEnum::from);
        }

        let storage_type = self.atomic_storage_type(value_type)?;

        crate::translation::reinterpret_value(
            self.types.context(),
            &self.builder,
            value,
            semantic_type,
            storage_type,
            "atomic.bits.value",
        )
    }

    fn atomic_decode_compare_exchange(
        &mut self,
        operation: &MirMemoryOperation,
        value_type: bray_symbols::TypeId,
        result: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let previous = extract_value(&self.builder, result, 0)?;
        let previous = self.atomic_decode_value(previous, value_type)?;
        let succeeded = extract_value(&self.builder, result, 1)?;

        let result_type = operation
            .result_type()
            .expect("checked MIR memory translation requires an established mapping or value");

        let fields = self.aggregate_fields(result_type);

        if fields.len() != 2 {
            panic!("checked MIR memory translation violated an established compiler contract");
        }

        let mut decoded = self.types.map(result_type)?.const_zero();
        let previous_field = self.aggregate_value_element(&fields, 0)?;
        let succeeded_field = self.aggregate_value_element(&fields, 1)?;

        decoded = insert_value(&self.builder, decoded, previous, previous_field)?;

        insert_value(&self.builder, decoded, succeeded, succeeded_field)
    }

    fn atomic_value_is_boolean(&self, value_type: bray_symbols::TypeId) -> bool {
        self.type_mapping(value_type)
            .map(|mapping| matches!(mapping.kind(), CodegenTypeKind::Boolean))
            .expect("atomic value types must have codegen mappings")
    }

    fn atomic_values_equal(
        &self,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        if let (Some(left), Some(right)) = (int_value(left), int_value(right)) {
            return llvm(self.builder.build_int_compare(
                IntPredicate::EQ,
                left,
                right,
                "atomic.wait.equal",
            ));
        }

        let left = pointer_value(left)
            .expect("checked MIR memory translation requires an established mapping or value");

        let right = pointer_value(right)
            .expect("checked MIR memory translation requires an established mapping or value");

        llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            self.pointer_address(left)?,
            self.pointer_address(right)?,
            "atomic.wait.equal",
        ))
    }
}

const fn llvm_fetch_kind(kind: AtomicFetchKind) -> AtomicRMWBinOp {
    match kind {
        AtomicFetchKind::Add => AtomicRMWBinOp::Add,
        AtomicFetchKind::Subtract => AtomicRMWBinOp::Sub,
        AtomicFetchKind::And => AtomicRMWBinOp::And,
        AtomicFetchKind::Or => AtomicRMWBinOp::Or,
        AtomicFetchKind::Xor => AtomicRMWBinOp::Xor,
    }
}
