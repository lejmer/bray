use bray_bound_tree::{BoundExpressionId, SelectedArgument, SelectedCall};
use bray_ir::{MirBlockId, MirSourceAnchor};
use bray_symbols::NumericTruncation;

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_numeric_call(
        &mut self,
        id: BoundExpressionId,
        mut current: MirBlockId,
        source: MirSourceAnchor,
        selection: &SelectedCall,
    ) -> Result<Option<LoweredExpression>, LoweringError> {
        let Some(hook) = selection
            .implementation_hook()
            .filter(|hook| hook.is_numeric_policy())
        else {
            return Ok(None);
        };

        let Some(callable) = selection.resolution().declaration_source() else {
            // Callable values invoke their demanded compiler-provided body through the ordinary call path.
            return Ok(None);
        };

        if !crate::numeric::can_lower_directly(self.input.semantic_values(), callable, hook) {
            return Ok(None);
        }

        let plan = NumericTruncation::for_instance(
            self.input.semantic_values(),
            self.input.available_compiler_known_symbols(),
            callable,
            self.input.target().machine().pointer_width_bits().get(),
        );

        let mut operand = match selection.arguments() {
            [] => None,
            [
                SelectedArgument::Explicit {
                    expression,
                    conversion,
                    reborrow,
                    ..
                },
            ] => {
                let lowered = self.lower_call_argument(*expression, *reborrow, current)?;

                let Some(block) = lowered.block else {
                    return Ok(Some(lowered));
                };

                let operand = lowered
                    .value
                    .expect("checked numeric input produces its scalar value");

                let (block, operand) = self.convert_operand(
                    id,
                    block,
                    Self::retained_source(&source),
                    operand,
                    conversion,
                )?;

                current = block;

                Some(operand)
            }
            _ => panic!("recognized numeric policy retains zero or one explicit scalar input"),
        };

        current = self.lower_primitive_requirements(
            id,
            current,
            &source,
            selection,
            operand.as_mut_slice(),
        )?;

        let result_type = self.expression_type(id);

        let value = crate::numeric::lower_numeric_value(
            &mut self.builder,
            self.input.semantic_values(),
            current,
            &source,
            hook,
            plan,
            operand.as_slice(),
            result_type,
        )?;

        Ok(Some(LoweredExpression::continuing(
            current,
            Some(value),
            source,
        )))
    }
}
