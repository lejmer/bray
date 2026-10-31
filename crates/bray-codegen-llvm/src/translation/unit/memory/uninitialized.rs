use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation};
use bray_symbols::TypeId;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::undefined_value;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_uninit_new(
        &mut self,
        operation: &MirOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation);
        let ty = self.types.map(result)?;

        self.initialize_value(undefined_value(ty), result, "uninit.value")
    }

    pub(super) fn translate_uninit_pointer(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let [storage] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        self.memory_pointer(storage)
    }

    pub(super) fn translate_uninit_write(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let [storage, value] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let storage = self.memory_pointer(storage)?;
        let ty = self.operand_type(value);
        let value = self.operand(value)?;

        self.store_value(storage, value, ty)?;

        Ok(storage)
    }

    pub(super) fn translate_protected_value(
        &mut self,
        operation: &MirOperation,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [storage] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let value = self.operand(storage)?;

        self.snapshot_value(
            value,
            self.operation_result_type(operation),
            "protected.value",
        )
    }

    pub(super) fn translate_uninit_move(
        &mut self,
        memory: &MirMemoryOperation,
        element: TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [storage] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        let storage = self.memory_pointer(storage)?;

        self.load_value(storage, element, "memory.uninit.move")
    }

    pub(super) fn translate_anchored_borrow(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let [authority, pointer] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        self.operand(authority)?;

        self.memory_pointer(pointer)
    }
}
