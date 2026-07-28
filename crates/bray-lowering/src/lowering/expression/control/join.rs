use bray_bound_tree::{BoundExpressionId, BoundNodeOrigin};
use bray_ir::{MirBlockId, MirBlockKind, MirEdge, MirTerminatorKind, MirValueId};
use bray_symbols::TypeId;

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn push_result_join(
        &mut self,
        expression: BoundExpressionId,
        origin: BoundNodeOrigin,
    ) -> Result<(MirBlockId, MirValueId, TypeId), LoweringError> {
        let source = self.source(origin);
        let result_type = self.expression_type(expression)?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let result = self
            .builder
            .push_block_parameter(join, source, result_type)?;

        Ok((join, result, result_type))
    }

    pub(in crate::lowering::expression) fn finish_result_edge(
        &mut self,
        completion: LoweredExpression,
        join: MirBlockId,
        result_type: TypeId,
    ) -> Result<(), LoweringError> {
        let Some(block) = completion.block else {
            return Ok(());
        };

        let value = completion
            .value
            .unwrap_or_else(|| self.unit_operand(result_type));

        self.builder.set_terminator(
            block,
            completion.source,
            MirTerminatorKind::Goto(MirEdge::new(join, [value])),
        )?;

        Ok(())
    }

    pub(in crate::lowering::expression) fn finish_edge(
        &mut self,
        completion: LoweredExpression,
        target: MirBlockId,
    ) -> Result<(), LoweringError> {
        let Some(block) = completion.block else {
            return Ok(());
        };

        self.builder.set_terminator(
            block,
            completion.source,
            MirTerminatorKind::Goto(MirEdge::new(target, [])),
        )?;

        Ok(())
    }
}
