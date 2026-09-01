use super::core::UnitTranslator;
use super::support::{
    extract_value, float_predicate, int_value, integer_predicate, llvm, pointer_value,
};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenTypeKind, IntrinsicCall};
use bray_ir::{MirBinaryOperator, MirOperand, MirUnaryOperator};
use inkwell::IntPredicate;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{BasicValueEnum, IntValue, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn nullable_present(
        &self,
        subject: BasicValueEnum<'context>,
        subject_type: bray_symbols::TypeId,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match (mapping.kind(), subject) {
            (CodegenTypeKind::Pointer { .. }, BasicValueEnum::PointerValue(pointer)) => {
                llvm(self.builder.build_is_not_null(pointer, "nullable.present"))
            }
            (CodegenTypeKind::Aggregate(fields), subject) if fields.len() > 1 => {
                let tag =
                    extract_value(&self.builder, subject, self.aggregate_element(fields, 0)?)?;

                let tag = int_value(tag).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_int_compare(
                    IntPredicate::NE,
                    tag,
                    tag.get_type().const_zero(),
                    "nullable.present",
                ))
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn nullable_present_at(
        &mut self,
        subject: PointerValue<'context>,
        subject_type: bray_symbols::TypeId,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let mapping = self
            .type_mapping(subject_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        match mapping.kind() {
            CodegenTypeKind::Pointer { .. } => {
                let value = llvm(self.builder.build_load(
                    self.types.map(subject_type)?,
                    subject,
                    "nullable.state",
                ))?;

                let pointer =
                    pointer_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_is_not_null(pointer, "nullable.present"))
            }
            CodegenTypeKind::Aggregate(fields) if fields.len() > 1 => {
                let state = fields
                    .first()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let state_pointer = llvm(self.builder.build_struct_gep(
                    self.types.map(subject_type)?,
                    subject,
                    self.aggregate_element(fields, 0)?,
                    "nullable.state.address",
                ))?;

                let state = llvm(self.builder.build_load(
                    self.types.map(state.ty())?,
                    state_pointer,
                    "nullable.state",
                ))?;

                let state = int_value(state).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(self.builder.build_int_compare(
                    IntPredicate::NE,
                    state,
                    state.get_type().const_zero(),
                    "nullable.present",
                ))
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    pub(super) fn translate_intrinsic_call(
        &mut self,
        intrinsic: &IntrinsicCall,
        operand_type: bray_symbols::TypeId,
        result_type: bray_symbols::TypeId,
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
            (
                IntrinsicCall::Comparison {
                    less,
                    equal,
                    greater,
                },
                [left, right],
            ) => self.translate_comparison(
                operand_type,
                result_type,
                [*less, *equal, *greater],
                *left,
                *right,
            ),
            (IntrinsicCall::Conversion(conversion), [operand]) => {
                let mut helpers = std::iter::empty::<&CodegenHelperMapping>();

                self.translate_conversion_plan(*operand, conversion, &mut helpers)
            }
            _ => Err(CodegenFailure::GeneratedModuleInvariant),
        }
    }

    fn translate_comparison(
        &mut self,
        operand_type: bray_symbols::TypeId,
        result_type: bray_symbols::TypeId,
        variants: [bray_symbols::UnionVariantSymbolId; 3],
        left: BasicValueEnum<'context>,
        right: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let (concrete_type, left) = self.intrinsic_operand(operand_type, left)?;

        let (_, right) = self.intrinsic_operand(operand_type, right)?;

        let (less, greater) = match (left, right) {
            (BasicValueEnum::IntValue(left), BasicValueEnum::IntValue(right)) => {
                let signed = self.signed_integer(concrete_type)?;

                (
                    llvm(self.builder.build_int_compare(
                        integer_predicate(MirBinaryOperator::LessThan, signed),
                        left,
                        right,
                        "compare.less",
                    ))?,
                    llvm(self.builder.build_int_compare(
                        integer_predicate(MirBinaryOperator::GreaterThan, signed),
                        left,
                        right,
                        "compare.greater",
                    ))?,
                )
            }
            (BasicValueEnum::FloatValue(left), BasicValueEnum::FloatValue(right)) => (
                llvm(self.builder.build_float_compare(
                    float_predicate(MirBinaryOperator::LessThan),
                    left,
                    right,
                    "compare.less",
                ))?,
                llvm(self.builder.build_float_compare(
                    float_predicate(MirBinaryOperator::GreaterThan),
                    left,
                    right,
                    "compare.greater",
                ))?,
            ),
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let mapping = self
            .type_mapping(result_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag = tag.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let [less_variant, equal_variant, greater_variant] = variants;

        let less_tag = self.union_variant_tag(result_type, less_variant, tag_type)?;
        let equal_tag = self.union_variant_tag(result_type, equal_variant, tag_type)?;
        let greater_tag = self.union_variant_tag(result_type, greater_variant, tag_type)?;

        let not_less =
            llvm(
                self.builder
                    .build_select(greater, greater_tag, equal_tag, "compare.not_less"),
            )?;

        let not_less = int_value(not_less).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let selected_tag =
            llvm(
                self.builder
                    .build_select(less, less_tag, not_less, "compare.ordering.tag"),
            )?;

        let llvm_type = self.types.map(result_type)?;
        let storage = self.allocate_temporary(llvm_type, "compare.ordering")?;

        llvm(self.builder.build_store(storage, llvm_type.const_zero()))?;
        llvm(self.builder.build_store(storage, selected_tag))?;

        llvm(
            self.builder
                .build_load(llvm_type, storage, "compare.ordering.value"),
        )
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
