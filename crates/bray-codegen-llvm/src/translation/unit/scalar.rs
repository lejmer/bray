use super::core::UnitTranslator;
use super::support::{float_predicate, integer_predicate, llvm, pointer_value};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenTypeKind, IntrinsicCall};
use bray_ir::{MirBinaryOperator, MirOperand, MirUnaryOperator};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_intrinsic_call(
        &mut self,
        intrinsic: &IntrinsicCall,
        operand_type: bray_symbols::TypeId,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match (intrinsic, arguments) {
            (IntrinsicCall::Unary(operator), [operand]) => {
                let (_, operand) = self.intrinsic_operand(operand_type, *operand)?;

                self.translate_unary_value(*operator, operand)
            }
            (IntrinsicCall::Binary(operator), [left, right]) => {
                let (concrete_type, left) = self.intrinsic_operand(operand_type, *left)?;

                let (_, right) = self.intrinsic_operand(operand_type, *right)?;

                self.translate_binary_values(*operator, left, right, concrete_type)
            }
            (IntrinsicCall::Conversion(conversion), [operand]) => {
                let mut helpers = std::iter::empty::<&CodegenHelperMapping>();

                self.translate_conversion_plan(*operand, conversion, &mut helpers)
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    fn intrinsic_operand(
        &mut self,
        ty: bray_symbols::TypeId,
        value: BasicValueEnum<'context>,
    ) -> Result<(bray_symbols::TypeId, BasicValueEnum<'context>), CodegenFailure> {
        let mapping = self
            .type_mapping(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Pointer { target, .. } = mapping.kind() else {
            return Ok((ty, value));
        };

        let target = *target;
        let pointer = pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let value = llvm(self.builder.build_load(
            self.types.map(target)?,
            pointer,
            "intrinsic.operand",
        ))?;

        Ok((target, value))
    }

    pub(super) fn translate_unary(
        &mut self,
        operator: MirUnaryOperator,
        operand: &MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operand_type = self.operand_type(operand)?;
        let value = self.operand(operand)?;

        let (_, value) = self.intrinsic_operand(operand_type, value)?;

        self.translate_unary_value(operator, value)
    }

    fn translate_unary_value(
        &self,
        operator: MirUnaryOperator,
        value: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match (operator, value) {
            (MirUnaryOperator::Negate, BasicValueEnum::IntValue(value)) => {
                llvm(self.builder.build_int_neg(value, "negate")).map(Into::into)
            }
            (MirUnaryOperator::Negate, BasicValueEnum::FloatValue(value)) => {
                llvm(self.builder.build_float_neg(value, "negate")).map(Into::into)
            }
            (
                MirUnaryOperator::Not | MirUnaryOperator::BitwiseNot,
                BasicValueEnum::IntValue(value),
            ) => llvm(self.builder.build_not(value, "not")).map(Into::into),
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn translate_binary(
        &mut self,
        operator: MirBinaryOperator,
        left: &MirOperand,
        right: &MirOperand,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let operand_type = self.operand_type(left)?;
        let right_type = self.operand_type(right)?;
        let left = self.operand(left)?;
        let right = self.operand(right)?;

        let (operand_type, left) = self.intrinsic_operand(operand_type, left)?;

        let (_, right) = self.intrinsic_operand(right_type, right)?;

        self.translate_binary_values(operator, left, right, operand_type)
    }

    fn translate_binary_values(
        &self,
        operator: MirBinaryOperator,
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
        operand_type: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        match (left, right) {
            (BasicValueEnum::IntValue(left), BasicValueEnum::IntValue(right)) => {
                self.integer_binary(operator, left, right, self.signed_integer(operand_type)?)
            }
            (BasicValueEnum::FloatValue(left), BasicValueEnum::FloatValue(right)) => {
                self.float_binary(operator, left, right)
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn integer_binary(
        &self,
        operator: MirBinaryOperator,
        left: inkwell::values::IntValue<'context>,
        right: inkwell::values::IntValue<'context>,
        signed: bool,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        use MirBinaryOperator as Operator;

        let value = match operator {
            Operator::Add => llvm(self.builder.build_int_add(left, right, "add"))?.into(),
            Operator::Subtract => llvm(self.builder.build_int_sub(left, right, "subtract"))?.into(),
            Operator::Multiply => llvm(self.builder.build_int_mul(left, right, "multiply"))?.into(),
            Operator::Divide => {
                if signed {
                    llvm(self.builder.build_int_signed_div(left, right, "divide"))?.into()
                } else {
                    llvm(self.builder.build_int_unsigned_div(left, right, "divide"))?.into()
                }
            }
            Operator::Remainder => {
                if signed {
                    llvm(self.builder.build_int_signed_rem(left, right, "remainder"))?.into()
                } else {
                    llvm(
                        self.builder
                            .build_int_unsigned_rem(left, right, "remainder"),
                    )?
                    .into()
                }
            }
            Operator::BitwiseAnd => llvm(self.builder.build_and(left, right, "and"))?.into(),
            Operator::BitwiseOr => llvm(self.builder.build_or(left, right, "or"))?.into(),
            Operator::BitwiseXor => llvm(self.builder.build_xor(left, right, "xor"))?.into(),
            Operator::ShiftLeft => {
                llvm(self.builder.build_left_shift(left, right, "shift.left"))?.into()
            }
            Operator::ShiftRight => {
                llvm(
                    self.builder
                        .build_right_shift(left, right, signed, "shift.right"),
                )?
                .into()
            }
            Operator::Equal
            | Operator::NotEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual => llvm(self.builder.build_int_compare(
                integer_predicate(operator, signed),
                left,
                right,
                "compare",
            ))?
            .into(),
        };

        Ok(value)
    }

    pub(super) fn float_binary(
        &self,
        operator: MirBinaryOperator,
        left: inkwell::values::FloatValue<'context>,
        right: inkwell::values::FloatValue<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        use MirBinaryOperator as Operator;

        let value = match operator {
            Operator::Add => llvm(self.builder.build_float_add(left, right, "add"))?.into(),
            Operator::Subtract => {
                llvm(self.builder.build_float_sub(left, right, "subtract"))?.into()
            }
            Operator::Multiply => {
                llvm(self.builder.build_float_mul(left, right, "multiply"))?.into()
            }
            Operator::Divide => llvm(self.builder.build_float_div(left, right, "divide"))?.into(),
            Operator::Remainder => {
                llvm(self.builder.build_float_rem(left, right, "remainder"))?.into()
            }
            Operator::Equal
            | Operator::NotEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual => llvm(self.builder.build_float_compare(
                float_predicate(operator),
                left,
                right,
                "compare",
            ))?
            .into(),
            Operator::BitwiseAnd
            | Operator::BitwiseOr
            | Operator::BitwiseXor
            | Operator::ShiftLeft
            | Operator::ShiftRight => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        Ok(value)
    }
}
