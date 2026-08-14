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
        let mut arguments = Vec::with_capacity(operation.arguments().len());

        for argument in selection.arguments() {
            let SelectedArgument::Explicit {
                expression,
                ordinal,
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

            let operand = self.convert_operand(
                id,
                current,
                Self::retained_source(&source),
                operand,
                conversion,
            )?;

            arguments.push((*ordinal, *expression, operand, conversion.target_type()));
        }

        arguments.sort_unstable_by_key(|(ordinal, _, _, _)| *ordinal);

        if arguments.len() != operation.arguments().len()
            || arguments
                .iter()
                .map(|(_, expression, _, _)| expression)
                .ne(operation.arguments())
        {
            return Err(LoweringError::MissingSemanticSelection(id));
        }

        let (operands, operand_types): (Vec<_>, Vec<_>) = arguments
            .into_iter()
            .map(|(_, _, operand, ty)| (operand, ty))
            .unzip();

        let result_type = self.expression_type(id)?;
        let kind = operation.kind();
        let result = kind.produces_value().then_some(result_type);

        let commit = self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Memory(MirMemoryOperation::new(
                kind,
                operands,
                operand_types,
                result,
            )),
            result,
        )?;

        if matches!(
            kind,
            bray_bound_tree::CheckedMemoryOperationKind::CatastrophicAbort
                | bray_bound_tree::CheckedMemoryOperationKind::UnreachableTermination
                | bray_bound_tree::CheckedMemoryOperationKind::InlineAssembly {
                    output: None,
                    ..
                }
        ) {
            self.builder.set_terminator(
                current,
                Self::retained_source(&source),
                bray_ir::MirTerminatorKind::Unreachable,
            )?;

            return Ok(LoweredExpression::terminated(source));
        }

        let value = match commit.result() {
            Some(value) => MirOperand::Value(value),
            None => self.unit_operand(result_type),
        };

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }
}
