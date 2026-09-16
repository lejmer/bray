use bray_codegen::CodegenFailure;
use bray_ir::{MirMemoryOperation, MirOperation};
use bray_symbols::TypeId;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::super::core::UnitTranslator;
use super::super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_uninit_new(
        &mut self,
        operation: &MirOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let result = self.operation_result_type(operation);
        let ty = self.types.map(result)?;

        Ok(undefined_value(ty))
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
        let value = self.operand(value)?;

        llvm(self.builder.build_store(storage, value))?;

        Ok(storage)
    }

    pub(super) fn translate_assume_initialized(
        &mut self,
        memory: &MirMemoryOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let [storage] = memory.operands() else {
            panic!("checked MIR memory translation violated an established compiler contract");
        };

        self.operand(storage)
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

        llvm(
            self.builder
                .build_load(self.types.map(element)?, storage, "memory.uninit.move"),
        )
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

fn undefined_value<'context>(ty: BasicTypeEnum<'context>) -> BasicValueEnum<'context> {
    match ty {
        BasicTypeEnum::ArrayType(ty) => ty.get_undef().into(),
        BasicTypeEnum::FloatType(ty) => ty.get_undef().into(),
        BasicTypeEnum::IntType(ty) => ty.get_undef().into(),
        BasicTypeEnum::PointerType(ty) => ty.get_undef().into(),
        BasicTypeEnum::StructType(ty) => ty.get_undef().into(),
        BasicTypeEnum::VectorType(ty) => ty.get_undef().into(),
        BasicTypeEnum::ScalableVectorType(ty) => ty.get_undef().into(),
    }
}
