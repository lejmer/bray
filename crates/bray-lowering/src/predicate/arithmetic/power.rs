use super::super::core::{PredicateLowerer, PredicateLoweringContext, capacity};
use bray_ir::{
    MirBinaryOperator as Op, MirBlockId, MirBlockKind, MirEdge, MirOperand, MirTerminatorKind,
};
use bray_symbols::TypeId;

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
        let exponent_type = self.builder.operand_type(&exponent);
        let base = self.convert(block, base, ty)?;

        let header = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let body = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let square = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let done = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let remaining = MirOperand::Value(self.parameter(header, exponent_type)?);
        let result = MirOperand::Value(self.parameter(header, ty)?);
        let factor = MirOperand::Value(self.parameter(header, ty)?);
        let completed = MirOperand::Value(self.parameter(done, ty)?);

        self.terminate(
            block,
            MirTerminatorKind::Goto(MirEdge::new(header, [exponent, self.integer(ty, 1)?, base])),
        )?;

        let active = self.machine(
            header,
            Op::NotEqual,
            remaining.clone(),
            self.integer(exponent_type, 0)?,
            self.boolean,
        )?;

        self.terminate(
            header,
            MirTerminatorKind::Branch {
                condition: active,
                then_edge: MirEdge::new(body, []),
                else_edge: MirEdge::new(done, [result.clone()]),
            },
        )?;

        let low_bit = self.machine(
            body,
            Op::BitwiseAnd,
            remaining.clone(),
            self.integer(exponent_type, 1)?,
            exponent_type,
        )?;

        let odd = self.machine(
            body,
            Op::NotEqual,
            low_bit,
            self.integer(exponent_type, 0)?,
            self.boolean,
        )?;

        // Branches and loop edges share immutable SSA operands, never an owned runtime value.
        let (continued, product) = self.branch_values(
            body,
            odd,
            ty,
            |lowerer, block| {
                let product =
                    lowerer.machine(block, Op::Multiply, result.clone(), factor.clone(), ty)?;

                Ok((block, product))
            },
            |_, block| Ok((block, result.clone())),
        )?;

        let count = self.machine(
            continued,
            Op::ShiftRight,
            remaining,
            self.integer(exponent_type, 1)?,
            exponent_type,
        )?;

        let active = self.machine(
            continued,
            Op::NotEqual,
            count.clone(),
            self.integer(exponent_type, 0)?,
            self.boolean,
        )?;

        self.terminate(
            continued,
            MirTerminatorKind::Branch {
                condition: active,
                then_edge: MirEdge::new(square, []),
                else_edge: MirEdge::new(done, [product.clone()]),
            },
        )?;

        let next_factor = self.machine(square, Op::Multiply, factor.clone(), factor, ty)?;

        self.terminate(
            square,
            MirTerminatorKind::Goto(MirEdge::new(header, [count, product, next_factor])),
        )?;

        Ok((done, completed))
    }
}
