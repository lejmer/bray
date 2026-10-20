use bray_ir::{MirBinaryOperator as Op, MirBlockId, MirBlockKind, MirEdge, MirOperand, MirOperationKind, MirTerminatorKind, MirUnaryOperator};
use bray_symbols::{ConstantBinaryOperation, TypeId};
use super::super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(in crate::predicate) fn integer(&self, ty: TypeId, value: u64) -> Result<MirOperand, C::Error> {
        crate::operand::integer_constant(self.context.semantic_values(), ty, value).map_err(|cause| crate::SyntheticLoweringError::SemanticValue(cause).into())
    }

    pub(in crate::predicate) fn machine(&mut self, block: MirBlockId, operator: Op, left: MirOperand, right: MirOperand, ty: TypeId) -> Result<MirOperand, C::Error> {
        self.value(block, MirOperationKind::Binary { operator, left, right }, ty)
    }

    pub(in crate::predicate) fn division(&mut self, mut block: MirBlockId, operation: ConstantBinaryOperation, left: MirOperand, right: MirOperand) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (left_bits, left_signed) = self.integer_shape(&left).expect("checked integer division retains integer operands");
        let (right_bits, right_signed) = self.integer_shape(&right).expect("checked integer division retains integer operands");
        let width = u64::from(left_bits.max(right_bits));
        let unsigned = self.integer_type(width, false)?;
        let signed = self.integer_type(width + 1, true)?;
        let mut negative = Vec::new();
        let mut magnitudes = Vec::new();
        for (value, is_signed) in [(left, left_signed), (right, right_signed)] {
            if is_signed {
                let value = self.convert(block, value, signed)?;
                let zero = self.integer(signed, 0)?;
                let sign = self.machine(block, Op::LessThan, value.clone(), zero, self.boolean)?;
                let (continued, magnitude) = self.branch_values(block, sign.clone(), signed,
                    |lowerer, block| { let value = lowerer.value(block, MirOperationKind::Unary { operator: MirUnaryOperator::Negate, operand: value.clone() }, signed)?; Ok((block, value)) },
                    |_, block| Ok((block, value.clone())))?;
                block = continued;
                magnitudes.push(self.convert(block, magnitude, unsigned)?);
                negative.push(sign);
            } else { magnitudes.push(self.convert(block, value, unsigned)?); negative.push(self.boolean_value(false)); }
        }
        let divisor = magnitudes.pop().expect("division retains divisor");
        let dividend = magnitudes.pop().expect("division retains dividend");
        let result = if width <= 128 {
            self.machine(block, if operation == ConstantBinaryOperation::Divide { Op::Divide } else { Op::Remainder }, dividend, divisor, unsigned)?
        } else {
            let (continued, quotient, remainder) = self.long_division(block, dividend, divisor, unsigned, width)?;
            block = continued;
            if operation == ConstantBinaryOperation::Divide { quotient } else { remainder }
        };
        if !left_signed && !right_signed { return Ok((block, result)); }
        let sign = if operation == ConstantBinaryOperation::Divide { self.machine(block, Op::BitwiseXor, negative[0].clone(), negative[1].clone(), self.boolean)? } else { negative.remove(0) };
        let result = self.convert(block, result, signed)?;
        self.branch_values(block, sign, signed,
            |lowerer, block| { let value = lowerer.value(block, MirOperationKind::Unary { operator: MirUnaryOperator::Negate, operand: result.clone() }, signed)?; Ok((block, value)) },
            |_, block| Ok((block, result.clone())))
    }

    fn long_division(&mut self, block: MirBlockId, dividend: MirOperand, divisor: MirOperand, ty: TypeId, width: u64) -> Result<(MirBlockId, MirOperand, MirOperand), C::Error> {
        let counter = self.integer_type(16, false)?;
        let remainder_type = self.integer_type(width + 1, false)?;
        let divisor = self.convert(block, divisor, remainder_type)?;
        let header = self.builder.push_block(self.source.clone(), MirBlockKind::Ordinary).map_err(capacity)?;
        let body = self.builder.push_block(self.source.clone(), MirBlockKind::Ordinary).map_err(capacity)?;
        let done = self.builder.push_block(self.source.clone(), MirBlockKind::Ordinary).map_err(capacity)?;
        let count = MirOperand::Value(self.builder.push_block_parameter(header, self.source.clone(), counter).map_err(capacity)?);
        let quotient = MirOperand::Value(self.builder.push_block_parameter(header, self.source.clone(), ty).map_err(capacity)?);
        let remainder = MirOperand::Value(self.builder.push_block_parameter(header, self.source.clone(), remainder_type).map_err(capacity)?);
        self.builder.set_terminator(block, self.source.clone(), MirTerminatorKind::Goto(MirEdge::new(header, [self.integer(counter, width)?, self.integer(ty, 0)?, self.integer(remainder_type, 0)?])));
        let active = self.machine(header, Op::NotEqual, count.clone(), self.integer(counter, 0)?, self.boolean)?;
        self.builder.set_terminator(header, self.source.clone(), MirTerminatorKind::Branch { condition: active, then_edge: MirEdge::new(body, []), else_edge: MirEdge::new(done, []) });
        let count = self.machine(body, Op::Subtract, count, self.integer(counter, 1)?, counter)?;
        let shift = self.convert(body, count.clone(), ty)?;
        let digit = self.machine(body, Op::ShiftRight, dividend, shift.clone(), ty)?;
        let digit = self.machine(body, Op::BitwiseAnd, digit, self.integer(ty, 1)?, ty)?;
        let digit = self.convert(body, digit, remainder_type)?;
        let remainder = self.machine(body, Op::ShiftLeft, remainder.clone(), self.integer(remainder_type, 1)?, remainder_type)?;
        let remainder = self.machine(body, Op::BitwiseOr, remainder, digit, remainder_type)?;
        let subtract = self.machine(body, Op::GreaterThanOrEqual, remainder.clone(), divisor.clone(), self.boolean)?;
        let (continued, next_remainder) = self.branch_values(body, subtract.clone(), remainder_type,
            |lowerer, block| { let result = lowerer.machine(block, Op::Subtract, remainder.clone(), divisor, remainder_type)?; Ok((block, result)) },
            |_, block| Ok((block, remainder.clone())))?;
        let bit = self.convert(continued, subtract, ty)?;
        let bit = self.machine(continued, Op::ShiftLeft, bit, shift, ty)?;
        let next_quotient = self.machine(continued, Op::BitwiseOr, quotient.clone(), bit, ty)?;
        self.builder.set_terminator(continued, self.source.clone(), MirTerminatorKind::Goto(MirEdge::new(header, [count, next_quotient, next_remainder])));
        let remainder = self.convert(done, remainder, ty)?;
        Ok((done, quotient, remainder))
    }

    pub(in crate::predicate) fn shift_right(&mut self, block: MirBlockId, left: MirOperand, right: MirOperand) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (bits, signed) = self.integer_shape(&left).expect("checked right shift retains integer subject");
        let left_type = self.builder.operand_type(&left);
        let count_shape = self.integer_shape(&right).expect("checked right shift retains integer count");
        let compare_type = self.integer_type(u64::from(count_shape.0).max(17), count_shape.1)?;
        let count = self.convert(block, right, compare_type)?;
        let large = self.machine(block, Op::GreaterThanOrEqual, count.clone(), self.integer(compare_type, u64::from(bits))?, self.boolean)?;
        self.branch_values(block, large, left_type,
            |lowerer, block| {
                let value = if signed { lowerer.machine(block, Op::ShiftRight, left.clone(), lowerer.integer(left_type, u64::from(bits-1))?, left_type)? } else { lowerer.integer(left_type, 0)? };
                Ok((block, value))
            },
            |lowerer, block| { let count = lowerer.convert(block, count, left_type)?; let value = lowerer.machine(block, Op::ShiftRight, left.clone(), count, left_type)?; Ok((block, value)) })
    }
}
