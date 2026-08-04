use bray_bound_tree::{BoundExpressionId, SelectedArgument, SelectedCall};
use bray_compiler_known::ImplementationHook;
use bray_ir::{MirBlockId, MirPanicCause, MirSourceAnchor};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_testing_call(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        selection: &SelectedCall,
    ) -> Result<Option<LoweredExpression>, LoweringError> {
        if selection.implementation_hook() != Some(ImplementationHook::TestingFail) {
            return Ok(None);
        }

        let [SelectedArgument::Explicit {
            expression: message,
            conversion,
            ..
        }] = selection.arguments()
        else {
            return Err(LoweringError::UnsupportedExpression(expression));
        };

        let lowered = self.lower_expression(*message, current)?;

        let Some(current) = lowered.block else {
            return Ok(Some(lowered));
        };

        let Some(message) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(*message));
        };

        let message = self.convert_operand(
            expression,
            current,
            Self::retained_source(&source),
            message,
            conversion,
        )?;

        let report_type = self.panic_report_type()?;

        let report = self.push_panic_report(
            expression,
            current,
            &source,
            MirPanicCause::ExplicitTestFailure(message),
            report_type,
        )?;

        self.finish_panic_to_active_catch(current, &source, report, report_type)?;

        Ok(Some(LoweredExpression::terminated(source)))
    }
}
