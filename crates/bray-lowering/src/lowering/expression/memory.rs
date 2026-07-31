use bray_bound_tree::{BoundExpressionId, CheckedMemoryOperation, SelectedArgument};
use bray_ir::{MirBlockId, MirMemoryOperation, MirOperand, MirOperationKind, MirSourceAnchor};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_memory_call(
        &mut self,
        id: BoundExpressionId,
        mut current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        operation: CheckedMemoryOperation,
    ) -> Result<LoweredExpression, LoweringError> {
        let mut operands = Vec::with_capacity(operation.arguments().len());

        for argument in selection.arguments() {
            let SelectedArgument::Explicit {
                expression,
                conversion,
                ..
            } = argument
            else {
                return Err(LoweringError::MissingSemanticSelection(id));
            };

            let lowered = self.lower_expression(*expression, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(*expression));
            };

            operands.push(self.convert_operand(
                id,
                current,
                Self::retained_source(&source),
                operand,
                conversion,
            )?);
        }

        if operands.len() != operation.arguments().len() {
            return Err(LoweringError::MissingSemanticSelection(id));
        }

        let result_type = self.expression_type(id)?;
        let kind = operation.kind();
        let result = kind.produces_value().then_some(result_type);

        let commit = self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Memory(MirMemoryOperation::new(kind, operands)),
            result,
        )?;

        let value = match commit.result() {
            Some(value) => MirOperand::Value(value),
            None => self.unit_operand(result_type),
        };

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }
}
