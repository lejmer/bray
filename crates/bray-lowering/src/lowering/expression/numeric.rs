use bray_bound_tree::{BoundExpressionId, SelectedArgument, SelectedCall};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBlockId, MirNumericConversionKind, MirOperand, MirOperationKind, MirSourceAnchor,
};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_numeric_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        selection: &SelectedCall,
    ) -> Result<Option<LoweredExpression>, LoweringError> {
        let Some(kind) = numeric_conversion_kind(selection.implementation_hook()) else {
            return Ok(None);
        };

        let [
            SelectedArgument::Explicit {
                expression,
                conversion,
                ..
            },
        ] = selection.arguments()
        else {
            return Err(LoweringError::MissingSemanticSelection(id));
        };

        let lowered = self.lower_expression(*expression, current)?;

        let Some(current) = lowered.block else {
            return Ok(Some(lowered));
        };

        let Some(operand) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(*expression));
        };

        let (current, operand) = self.convert_operand(
            id,
            current,
            Self::retained_source(&source),
            operand,
            conversion,
        )?;

        let result_type = self.expression_type(id)?;

        let commit = self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::NumericConversion { kind, operand },
            Some(result_type),
        )?;

        let result = commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(id))?;

        Ok(Some(LoweredExpression::continuing(
            current,
            Some(result),
            source,
        )))
    }
}

fn numeric_conversion_kind(hook: Option<ImplementationHook>) -> Option<MirNumericConversionKind> {
    match hook {
        Some(ImplementationHook::NumericTruncate) => Some(MirNumericConversionKind::Truncate),
        _ => None,
    }
}
