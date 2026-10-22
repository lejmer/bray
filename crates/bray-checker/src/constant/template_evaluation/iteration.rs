use bray_bound_tree::{CheckedTemplateLoopBody, CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_symbols::{ConstantValueId, ConstantValueKind, TypeId};

use super::evaluator::TemplateEvaluator;
use super::support::{TemplateEvaluationFailure, template_index};
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> TemplateEvaluator<'_, C> {
    pub(super) fn evaluate_loop(
        &mut self,
        owner: CheckedTemplateNodeId,
        operation: &CheckedTemplateOperation,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let CheckedTemplateOperation::Loop {
            start,
            bindings,
            condition,
            body,
            next,
            completed,
            outputs,
        } = operation
        else {
            unreachable!("loop evaluator receives its checked kernel");
        };

        let mut current = Vec::with_capacity(bindings.len());

        for binding in bindings.iter() {
            let CheckedTemplateOperation::LoopValue { initial } =
                self.template.nodes()[template_index(binding.raw())].operation()
            else {
                panic!("checked loop binding retains its initial value");
            };

            current.push(self.evaluate_node(*initial)?);
        }

        let first = template_index(start.raw());
        let end = template_index(owner.raw());

        let (result, states) = loop {
            self.observe_cancellation()?;

            // Charge the control step even when every body value is already memoized.
            self.budget
                .try_charge_step()
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            self.values[first..end].fill(None);

            for (binding, value) in bindings.iter().zip(&current) {
                self.values[template_index(binding.raw())] = Some(*value);
            }

            let condition = self.evaluate_node(*condition)?;

            if !self.boolean(condition)? {
                let result = self.evaluate_node(*completed)?;

                break (result, self.evaluate_nodes(outputs)?);
            }

            let value = self.evaluate_node(body.node())?;
            let states = self.evaluate_nodes(next)?;

            if matches!(body, CheckedTemplateLoopBody::Return(_)) {
                match self
                    .context
                    .semantic_values()
                    .constant_value_data(value)
                    .kind()
                {
                    ConstantValueKind::NullablePresent(_) => break (value, states),
                    ConstantValueKind::NullableAbsent => {}
                    other => {
                        panic!("checked loop completion retains its nullable result: {other:?}")
                    }
                }
            }

            current = states;
        };

        self.values[first..end].fill(None);

        self.outputs
            .insert(owner, states.into_iter().map(Some).collect());

        Ok(result)
    }

    pub(super) fn evaluate_array_transform(
        &mut self,
        subject: CheckedTemplateNodeId,
        element: CheckedTemplateNodeId,
        value: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> Result<ConstantValueId, TemplateEvaluationFailure> {
        let subject = self.evaluate_node(subject)?;
        let subject = self.observed_value(subject)?;

        let ConstantValueKind::Array(elements) = subject.kind() else {
            panic!("checked array transform receives its array value");
        };

        self.budget
            .try_charge_elements(elements.len())
            .map_err(TemplateEvaluationFailure::Diagnostic)?;

        let mut converted = Vec::with_capacity(elements.len());
        let first = template_index(element.raw());
        let end = template_index(value.raw()) + 1;

        for input in elements.iter() {
            // The checked body occupies this contiguous range. Keep outer values
            // memoized while rebinding only this element and its computation.
            self.values[first..end].fill(None);
            self.values[first] = Some(*input);
            converted.push(self.evaluate_node(value)?);
        }

        self.values[first..end].fill(None);

        Ok(self.intern_value(ty, ConstantValueKind::array(converted)))
    }
}
