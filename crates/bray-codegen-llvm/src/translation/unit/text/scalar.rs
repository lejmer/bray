use bray_codegen::CodegenFailure;
use bray_ir::MirTextOperation;
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::llvm;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn text_scalar_count(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (data, length) = self.only_string_view(operation)?;

        let integer = self.pointer_integer_type();

        let function = self.text_function(
            bray_runtime_interface::STRING_SCALAR_COUNT_SYMBOL,
            Some(integer.into()),
            &[data.get_type().into(), integer.into()],
        );

        self.call_value(function, &[data.into(), length.into()], "string.scalar_count")
    }

    pub(super) fn text_is_empty(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (_, length) = self.only_string_view(operation)?;

        let empty = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            length,
            length.get_type().const_zero(),
            "string.empty",
        ))?;

        Ok(empty.into())
    }

    pub(super) fn text_equals(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(left, left_type), (right, right_type)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (left_data, left_length, _) = self.borrowed_string_parts(*left, *left_type)?;

        let (right_data, right_length, _) = self.borrowed_string_parts(*right, *right_type)?;

        let integer = self.pointer_integer_type();
        let byte = self.types.context().i8_type();

        let function = self.text_function(
            bray_runtime_interface::STRING_EQUALS_SYMBOL,
            Some(byte.into()),
            &[
                left_data.get_type().into(),
                integer.into(),
                right_data.get_type().into(),
                integer.into(),
            ],
        );

        let equal = self.call_value(
            function,
            &[
                left_data.into(),
                left_length.into(),
                right_data.into(),
                right_length.into(),
            ],
            "string.equals",
        )?;

        let equal = equal.into_int_value();

        llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            equal,
            equal.get_type().const_zero(),
            "string.equal",
        ))
        .map(Into::into)
    }

    pub(super) fn text_scalar_at(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(text, text_type), (index, _)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (data, length, _) = self.borrowed_string_parts(*text, *text_type)?;

        let index = self.pointer_sized_integer(*index)?;
        let scalar_type = self.types.context().i32_type();
        let scalar = llvm(self.builder.build_alloca(scalar_type, "string.scalar"))?;
        let byte = self.types.context().i8_type();

        let function = self.text_function(
            bray_runtime_interface::STRING_SCALAR_AT_SYMBOL,
            Some(byte.into()),
            &[
                data.get_type().into(),
                length.get_type().into(),
                index.get_type().into(),
                scalar.get_type().into(),
            ],
        );

        let present = self
            .call_value(
                function,
                &[data.into(), length.into(), index.into(), scalar.into()],
                "string.scalar_at",
            )?
            .into_int_value();

        let present = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            present,
            present.get_type().const_zero(),
            "string.scalar.present",
        ))?;

        let scalar = llvm(self.builder.build_load(scalar_type, scalar, "string.scalar.value"))?;

        self.nullable_value(operation.result_type(), present, scalar)
    }

    pub(super) fn text_scalar_slice(
        &mut self,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(text, text_type), (start, _), (end, _)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (data, length, _) = self.borrowed_string_parts(*text, *text_type)?;

        let start = self.pointer_sized_integer(*start)?;
        let end = self.pointer_sized_integer(*end)?;

        let (result_data, result_length, result_owner) = self.string_outputs(data.get_type())?;

        let function = self.text_function(
            bray_runtime_interface::STRING_SCALAR_SLICE_SYMBOL,
            None,
            &[
                data.get_type().into(),
                length.get_type().into(),
                start.get_type().into(),
                end.get_type().into(),
                result_data.get_type().into(),
                result_length.get_type().into(),
                result_owner.get_type().into(),
            ],
        );

        llvm(self.builder.build_call(
            function,
            &[
                data.into(),
                length.into(),
                start.into(),
                end.into(),
                result_data.into(),
                result_length.into(),
                result_owner.into(),
            ],
            "string.scalar_slice",
        ))?;

        self.load_owned_string(operation, result_data, result_length, result_owner)
    }
}
