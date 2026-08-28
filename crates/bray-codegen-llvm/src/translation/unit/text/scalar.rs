use bray_codegen::{CodegenFailure, CodegenResultMapping, CodegenTypeKind};
use bray_ir::{
    MirHelperReference, MirOperationId, MirStandardLibraryHelper, MirTextOperation,
};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, llvm, next_helper, pointer_value};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn text_scalar_count(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (data, length) = self.only_string_view(operation)?;

        self.invoke_text_helper(
            operation_id,
            MirStandardLibraryHelper::StringScalarCount,
            &[data.into(), length.into()],
        )
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
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(left, left_type), (right, right_type)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (left_data, left_length, _) = self.string_view_parts(*left, *left_type)?;

        let (right_data, right_length, _) = self.string_view_parts(*right, *right_type)?;

        self.invoke_text_helper(
            operation_id,
            MirStandardLibraryHelper::StringEquals,
            &[
                left_data.into(),
                left_length.into(),
                right_data.into(),
                right_length.into(),
            ],
        )
    }

    pub(super) fn text_scalar_at(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(text, text_type), (index, _)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (data, length, _) = self.string_view_parts(*text, *text_type)?;

        let index = self.pointer_sized_integer(*index)?;

        self.invoke_text_helper(
            operation_id,
            MirStandardLibraryHelper::StringScalarAt,
            &[data.into(), length.into(), index.into()],
        )
    }

    pub(super) fn text_scalar_slice(
        &mut self,
        operation_id: MirOperationId,
        operation: &MirTextOperation,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operands = self.text_operands(operation)?;

        let [(text, text_type), (start, _), (end, _)] = operands.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let (data, length, _) = self.string_view_parts(*text, *text_type)?;

        let start = self.pointer_sized_integer(*start)?;
        let end = self.pointer_sized_integer(*end)?;

        let (text, text_type) = self.invoke_text_helper_result(
            operation_id,
            MirStandardLibraryHelper::StringScalarSlice,
            &[
                data.into(),
                length.into(),
                start.into(),
                end.into(),
            ],
        )?;

        self.owned_text(operation, text, text_type)
    }

    pub(super) fn invoke_text_helper(
        &mut self,
        operation_id: MirOperationId,
        helper: MirStandardLibraryHelper,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.invoke_text_helper_result(operation_id, helper, arguments)
            .map(|(value, _)| value)
    }

    pub(super) fn invoke_text_helper_result(
        &mut self,
        operation_id: MirOperationId,
        helper: MirStandardLibraryHelper,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<(BasicValueEnum<'context>, bray_symbols::TypeId), CodegenFailure> {
        let reference = MirHelperReference::StandardLibrary(helper);
        let helpers = self.operation_helpers(operation_id)?;
        let mut helpers = helpers.iter();
        let helper = next_helper(&mut helpers, &reference)?;

        if helpers.next().is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result_type = helper
            .symbol()
            .and_then(|key| self.request.mappings().symbol(key))
            .and_then(|symbol| match symbol.signature().result() {
                CodegenResultMapping::Direct { ty, .. } => Some(*ty),
                CodegenResultMapping::Indirect { pointee, .. } => Some(*pointee),
                CodegenResultMapping::Void => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let result = self
            .invoke_helper(helper, arguments)?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok((result, result_type))
    }

    pub(super) fn owned_text(
        &mut self,
        operation: &MirTextOperation,
        text: BasicValueEnum<'context>,
        text_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let fields = match self
            .type_mapping(text_type)
            .map(bray_codegen::CodegenTypeMapping::kind)
        {
            Some(CodegenTypeKind::Aggregate(fields)) => fields.clone(),
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let data = extract_value(
            &self.builder,
            text,
            self.aggregate_element(&fields, 0)?,
        )
            .and_then(|value| pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let length = extract_value(
            &self.builder,
            text,
            self.aggregate_element(&fields, 1)?,
        )?
        .into_int_value();

        let owner = extract_value(
            &self.builder,
            text,
            self.aggregate_element(&fields, 2)?,
        )
            .and_then(|value| pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let result = operation
            .result_type()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let string = if self.string_type(result) {
            result
        } else {
            self.result_string_type(result)?
        };

        self.string_value(string, data, length, owner)
    }
}
