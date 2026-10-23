use std::num::NonZeroU16;

use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_ir::{MirBinaryOperator, MirBlockId, MirOperand, MirOperationKind, MirUnaryOperator};
use bray_symbols::{
    ConstantBinaryOperation, ConstantUnaryOperation, ConstantValueKind, IntegerSign, TypeData,
    TypeId,
};

use super::super::core::{PredicateLowerer, PredicateLoweringContext};
use crate::SyntheticLoweringError;

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(in crate::predicate) fn power(
        &mut self,
        block: MirBlockId,
        base: MirOperand,
        exponent: MirOperand,
        machine_type: Option<TypeId>,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (bits, signed) = self
            .integer_shape(&base)
            .expect("checked exact exponentiation retains integer base");

        let ty = match machine_type {
            Some(ty) => ty,
            None => {
                let maximum = self.maximum_shift(&exponent);

                self.integer_type(u64::from(bits).saturating_mul(maximum).max(2), signed)?
            }
        };

        let base = self.convert(block, base, ty)?;

        let value = self.machine(
            block,
            bray_ir::MirBinaryOperator::Exponentiate,
            base,
            exponent,
            ty,
        )?;

        Ok((block, value))
    }

    pub(in crate::predicate) fn integer_type(
        &self,
        bits: u64,
        signed: bool,
    ) -> Result<TypeId, C::Error> {
        let width = u16::try_from(bits).map_err(|_| {
            C::Error::from(SyntheticLoweringError::ExactIntegerCapacity { minimum_bits: bits })
        })?;

        let width = NonZeroU16::new(width)
            .expect("exact predicate integers retain a nonzero representation width");

        self.context
            .semantic_values()
            .intern_type(TypeData::ExactInteger {
                bits: width,
                signed,
            })
            .map_err(|cause| SyntheticLoweringError::SemanticValue(cause).into())
    }

    pub(in crate::predicate) fn integer_shape(&self, operand: &MirOperand) -> Option<(u16, bool)> {
        if let MirOperand::Constant { value, .. } = operand
            && let ConstantValueKind::Integer(integer) = self
                .context
                .semantic_values()
                .constant_value_data(*value)
                .kind()
        {
            let bits = integer.magnitude_bits().max(1);
            let signed = integer.sign() == IntegerSign::Negative;

            return Some((u16::try_from(bits + usize::from(signed)).ok()?, signed));
        }

        self.integer_type_shape(self.builder.operand_type(operand))
    }

    pub(in crate::predicate) fn integer_type_shape(&self, ty: TypeId) -> Option<(u16, bool)> {
        let data = self.context.semantic_values().type_data(ty);

        match data.as_ref() {
            TypeData::ExactInteger { bits, signed } => Some((bits.get(), *signed)),
            TypeData::Named { definition, .. } => {
                let representation = self
                    .context
                    .representation_role(*definition)?
                    .integer_representation()?;

                let pointer = self
                    .builder
                    .target()
                    .profile()
                    .machine()
                    .pointer_width_bits()
                    .get();

                Some(match representation {
                    IntegerRepresentation::Signed(bits) => (bits, true),
                    IntegerRepresentation::Unsigned(bits) => (bits, false),
                    IntegerRepresentation::TargetSigned => (pointer, true),
                    IntegerRepresentation::TargetUnsigned => (pointer, false),
                })
            }
            _ => None,
        }
    }

    pub(in crate::predicate) fn common_type(
        &self,
        left: &MirOperand,
        right: &MirOperand,
        expected: TypeId,
    ) -> Result<TypeId, C::Error> {
        let left_type = self.builder.operand_type(left);
        let right_type = self.builder.operand_type(right);

        if left_type == right_type {
            return Ok(left_type);
        }

        let (Some((left_bits, left_signed)), Some((right_bits, right_signed))) =
            (self.integer_shape(left), self.integer_shape(right))
        else {
            assert_eq!(
                left_type, expected,
                "checked predicate branch retains its result type"
            );

            assert_eq!(
                right_type, expected,
                "checked predicate branch retains its result type"
            );

            return Ok(expected);
        };

        let signed = left_signed || right_signed;
        let left_bits = u64::from(left_bits) + u64::from(signed && !left_signed);
        let right_bits = u64::from(right_bits) + u64::from(signed && !right_signed);

        self.integer_type(left_bits.max(right_bits), signed)
    }

    pub(in crate::predicate) fn unary(
        &mut self,
        block: MirBlockId,
        operation: ConstantUnaryOperation,
        operand: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, C::Error> {
        let operand = self.observe(block, operand)?;

        let operator = match operation {
            ConstantUnaryOperation::Identity
            | ConstantUnaryOperation::BorrowObservation
            | ConstantUnaryOperation::EntryCondition => return Ok(operand),
            ConstantUnaryOperation::LogicalNot => MirUnaryOperator::Not,
            ConstantUnaryOperation::Negate => MirUnaryOperator::Negate,
            ConstantUnaryOperation::BitwiseNot => MirUnaryOperator::BitwiseNot,
            ConstantUnaryOperation::PredicateTrust => {
                panic!("opaque trusted evidence has no runtime predicate evaluator")
            }
        };

        let result = match (operator, self.integer_shape(&operand)) {
            (MirUnaryOperator::Negate, Some((bits, _))) => {
                self.integer_type(u64::from(bits) + 1, true)?
            }
            _ => ty,
        };

        let operand = self.convert(block, operand, result)?;

        self.value(block, MirOperationKind::Unary { operator, operand }, result)
    }

    pub(in crate::predicate) fn machine_unary(
        &mut self,
        block: MirBlockId,
        operation: ConstantUnaryOperation,
        operand: MirOperand,
        ty: TypeId,
    ) -> Result<MirOperand, C::Error> {
        let operand = self.observe(block, operand)?;

        if self.integer_shape(&operand).is_none() {
            return self.unary(block, operation, operand, ty);
        }

        let operator = match operation {
            ConstantUnaryOperation::Identity
            | ConstantUnaryOperation::BorrowObservation
            | ConstantUnaryOperation::EntryCondition => return self.convert(block, operand, ty),
            ConstantUnaryOperation::LogicalNot => MirUnaryOperator::Not,
            ConstantUnaryOperation::Negate => MirUnaryOperator::Negate,
            ConstantUnaryOperation::BitwiseNot => MirUnaryOperator::BitwiseNot,
            ConstantUnaryOperation::PredicateTrust => {
                panic!("trusted evidence has no executable unary value")
            }
        };

        let operand = self.convert(block, operand, ty)?;

        self.value(block, MirOperationKind::Unary { operator, operand }, ty)
    }

    pub(in crate::predicate) fn machine_binary(
        &mut self,
        block: MirBlockId,
        operation: ConstantBinaryOperation,
        left: MirOperand,
        right: MirOperand,
        operand_type: TypeId,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let left = self.observe(block, left)?;
        let right = self.observe(block, right)?;

        if self.integer_shape(&left).is_none() {
            return self.binary(block, operation, left, right, ty);
        }

        let left = self.convert(block, left, operand_type)?;

        if operation == ConstantBinaryOperation::Exponentiate {
            return self.power(block, left, right, Some(ty));
        }

        let right = self.convert(block, right, operand_type)?;

        Ok((
            block,
            self.machine(block, machine_operator(operation), left, right, ty)?,
        ))
    }

    pub(in crate::predicate) fn binary(
        &mut self,
        block: MirBlockId,
        operation: ConstantBinaryOperation,
        left: MirOperand,
        right: MirOperand,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        use ConstantBinaryOperation as Operation;

        let left = self.observe(block, left)?;
        let right = self.observe(block, right)?;

        let operand_type = self.builder.operand_type(&left);

        if matches!(operation, Operation::Equal | Operation::NotEqual) {
            let subject =
                if crate::operand::nullable_presence(self.context.semantic_values(), &right)
                    == Some(false)
                {
                    Some(&left)
                } else if crate::operand::nullable_presence(self.context.semantic_values(), &left)
                    == Some(false)
                {
                    Some(&right)
                } else {
                    None
                };

            if let Some(subject) = subject {
                let subject_type = self.builder.operand_type(subject);

                assert!(
                    matches!(
                        self.context
                            .semantic_values()
                            .type_data(subject_type)
                            .as_ref(),
                        TypeData::Nullable(_)
                    ),
                    "checked nullable presence comparison retains its nullable subject"
                );

                let kind = if operation == Operation::Equal {
                    bray_ir::MirNullableQueryKind::IsAbsent
                } else {
                    bray_ir::MirNullableQueryKind::IsPresent
                };

                let value = self.value(
                    block,
                    MirOperationKind::NullableQuery(bray_ir::MirNullableQuery::new(
                        kind,
                        subject.clone(),
                        subject_type,
                        subject_type,
                        ty,
                    )),
                    ty,
                )?;

                return Ok((block, value));
            }
        }

        if matches!(operation, Operation::Equal | Operation::NotEqual)
            && matches!(self.context.semantic_values().type_data(operand_type).as_ref(),
                TypeData::Named { definition, .. } if self.context.representation_role(*definition) == Some(RepresentationRole::String))
        {
            let equal = self.value(
                block,
                crate::lowering::expression::text::string_equality_operation(
                    [left, right],
                    operand_type,
                    ty,
                ),
                ty,
            )?;

            let result = if operation == Operation::NotEqual {
                self.value(
                    block,
                    MirOperationKind::Unary {
                        operator: MirUnaryOperator::Not,
                        operand: equal,
                    },
                    ty,
                )?
            } else {
                equal
            };

            return Ok((block, result));
        }

        if operation == Operation::Exponentiate {
            return self.power(block, left, right, None);
        }

        let operator = machine_operator(operation);
        let shapes = self.integer_shape(&left).zip(self.integer_shape(&right));

        let comparison = matches!(
            operation,
            Operation::Equal
                | Operation::NotEqual
                | Operation::Less
                | Operation::LessOrEqual
                | Operation::Greater
                | Operation::GreaterOrEqual
        );

        let Some(((left_bits, left_signed), (right_bits, right_signed))) = shapes else {
            return Ok((
                block,
                self.value(
                    block,
                    MirOperationKind::Binary {
                        operator,
                        left,
                        right,
                    },
                    ty,
                )?,
            ));
        };

        if matches!(operation, Operation::Divide | Operation::Remainder) {
            return self.division(block, operation, left, right);
        }

        if operation == Operation::ShiftRight {
            return self.shift_right(block, left, right);
        }

        let signed = left_signed || right_signed || operation == Operation::Subtract;
        let left_width = u64::from(left_bits) + u64::from(signed && !left_signed);
        let right_width = u64::from(right_bits) + u64::from(signed && !right_signed);

        let result = match operation {
            Operation::Subtract if !left_signed && !right_signed => {
                self.integer_type(u64::from(left_bits.max(right_bits)) + 1, true)?
            }
            Operation::Add | Operation::Subtract => {
                self.integer_type(left_width.max(right_width) + 1, signed)?
            }
            Operation::Multiply => {
                self.integer_type(u64::from(left_bits) + u64::from(right_bits), signed)?
            }
            Operation::ShiftLeft => self.integer_type(
                u64::from(left_bits).saturating_add(self.maximum_shift(&right)),
                left_signed,
            )?,
            _ => self.common_type(&left, &right, ty)?,
        };

        let left = self.convert(block, left, result)?;
        let right = self.convert(block, right, result)?;
        let result_type = if comparison { self.boolean } else { result };

        let value = self.value(
            block,
            MirOperationKind::Binary {
                operator,
                left,
                right,
            },
            result_type,
        )?;

        Ok((block, value))
    }

    pub(in crate::predicate) fn maximum_shift(&self, value: &MirOperand) -> u64 {
        if let MirOperand::Constant { value, .. } = value
            && let ConstantValueKind::Integer(integer) = self
                .context
                .semantic_values()
                .constant_value_data(*value)
                .kind()
        {
            return integer.to_u64().unwrap_or(u64::MAX);
        }

        let (bits, signed) = self
            .integer_shape(value)
            .expect("checked shift count is integer-valued");

        let bits = u32::from(bits) - u32::from(signed);

        1_u64.checked_shl(bits).map_or(u64::MAX, |limit| limit - 1)
    }
}

fn machine_operator(operation: ConstantBinaryOperation) -> MirBinaryOperator {
    use ConstantBinaryOperation as Operation;

    match operation {
        Operation::Add => MirBinaryOperator::Add,
        Operation::Subtract => MirBinaryOperator::Subtract,
        Operation::Multiply => MirBinaryOperator::Multiply,
        Operation::Divide => MirBinaryOperator::Divide,
        Operation::Remainder => MirBinaryOperator::Remainder,
        Operation::Equal => MirBinaryOperator::Equal,
        Operation::NotEqual => MirBinaryOperator::NotEqual,
        Operation::Less => MirBinaryOperator::LessThan,
        Operation::LessOrEqual => MirBinaryOperator::LessThanOrEqual,
        Operation::Greater => MirBinaryOperator::GreaterThan,
        Operation::GreaterOrEqual => MirBinaryOperator::GreaterThanOrEqual,
        Operation::BitwiseAnd | Operation::LogicalAnd => MirBinaryOperator::BitwiseAnd,
        Operation::BitwiseOr | Operation::LogicalOr => MirBinaryOperator::BitwiseOr,
        Operation::BitwiseXor => MirBinaryOperator::BitwiseXor,
        Operation::ShiftLeft => MirBinaryOperator::ShiftLeft,
        Operation::ShiftRight => MirBinaryOperator::ShiftRight,
        Operation::Exponentiate => {
            unreachable!("exact exponentiation lowers to bounded integer operations")
        }
    }
}
