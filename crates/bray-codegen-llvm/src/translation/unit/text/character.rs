use bray_codegen::CodegenFailure;
use bray_ir::{MirOperationId, MirStandardLibraryHelper, MirTextOperation};
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn character_scalar_value(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.invoke_character_helper(
            operation_id,
            operation,
            MirStandardLibraryHelper::CharacterScalarValue,
        )
    }

    pub(super) fn character_from_scalar_value(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.invoke_character_helper(
            operation_id,
            operation,
            MirStandardLibraryHelper::CharacterFromScalarValue,
        )
    }

    pub(super) fn character_utf8_length(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.invoke_character_helper(
            operation_id,
            operation,
            MirStandardLibraryHelper::CharacterUtf8Length,
        )
    }

    pub(super) fn character_utf8_byte(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.invoke_character_helper(
            operation_id,
            operation,
            MirStandardLibraryHelper::CharacterUtf8Byte,
        )
    }

    pub(super) fn character_predicate(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
        helper: MirStandardLibraryHelper,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.invoke_character_helper(operation_id, operation, helper)
    }

    fn invoke_character_helper(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
        helper: MirStandardLibraryHelper,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let arguments = operands.iter().map(|(value, _)| *value).collect::<Vec<_>>();

        self.invoke_text_helper(operation_id, helper, &arguments)
    }
}
