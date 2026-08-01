use bray_codegen::CodegenFailure;
use bray_ir::MirTextOperation;
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn character_scalar_value(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let value = self.only_scalar_operand(operation)?;

        let function = self.text_function(
            bray_runtime_interface::CHARACTER_SCALAR_VALUE_SYMBOL,
            Some(value.get_type().into()),
            &[value.get_type().into()],
        );

        self.call_value(function, &[value.into()], "character.scalar_value")
    }

    pub(super) fn character_from_scalar_value(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let value = self.only_scalar_operand(operation)?;
        let scalar = llvm(self.builder.build_alloca(value.get_type(), "character.scalar"))?;
        let byte = self.types.context().i8_type();

        let function = self.text_function(
            bray_runtime_interface::CHARACTER_FROM_SCALAR_VALUE_SYMBOL,
            Some(byte.into()),
            &[value.get_type().into(), scalar.get_type().into()],
        );

        let present = self
            .call_value(
                function,
                &[value.into(), scalar.into()],
                "character.from_scalar_value",
            )?
            .into_int_value();

        let present = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            present,
            present.get_type().const_zero(),
            "character.scalar.present",
        ))?;

        let scalar = llvm(self.builder.build_load(
            value.get_type(),
            scalar,
            "character.scalar.value",
        ))?;

        self.nullable_value(operation.result_type(), present, scalar)
    }

    pub(super) fn character_utf8_length(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let value = self.only_scalar_operand(operation)?;
        let result = self.pointer_integer_type();

        let function = self.text_function(
            bray_runtime_interface::CHARACTER_UTF8_LENGTH_SYMBOL,
            Some(result.into()),
            &[value.get_type().into()],
        );

        self.call_value(function, &[value.into()], "character.utf8_length")
    }

    pub(super) fn character_predicate(
        &mut self,
        operation: &MirTextOperation,
        symbol: &str,
        name: &str,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let value = self.only_scalar_operand(operation)?;
        let byte = self.types.context().i8_type();
        let function = self.text_function(symbol, Some(byte.into()), &[value.get_type().into()]);

        let result = self
            .call_value(function, &[value.into()], name)?
            .into_int_value();

        llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            result,
            result.get_type().const_zero(),
            name,
        ))
        .map(Into::into)
    }
}
