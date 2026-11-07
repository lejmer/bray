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

        let [argument @ SelectedArgument::Explicit { .. }] = selection.arguments() else {
            panic!("testing failure retains its selected explicit message input");
        };
        let lowered = self.lower_selected_argument_input(expression, argument, current, &source)?;
        let Some(current) = lowered.block else { return Ok(Some(lowered)); };
        let message = lowered.value.expect("testing failure retains its staged message value");

        let current = self.lower_primitive_requirements(
            expression,
            current,
            &source,
            selection,
            std::slice::from_ref(&message),
        )?;

        let report_type =
            self.representation_type(bray_compiler_known::RepresentationRole::PanicReport)?;

        let report = self.push_panic_report(
            expression,
            current,
            &source,
            MirPanicCause::ExplicitTestFailure(message),
            report_type,
        )?;

        self.finish_panic_to_active_catch(expression, current, &source, report, report_type)?;

        Ok(Some(LoweredExpression::terminated(source)))
    }
}
