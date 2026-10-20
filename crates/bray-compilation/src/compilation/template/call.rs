use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundCallableTarget, BoundExpressionId, CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation, SelectedArgument, SemanticSelection};
use crate::fact::FactQueryError;
use super::build::{SourceTemplateBuilder, incomplete, index};

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn call(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        match self.selections.expression(expression) {
            Some(SemanticSelection::Call(call)) => self.callable_call(call),
            Some(SemanticSelection::Predicate(predicate)) => self.predicate_call(predicate),
            _ => Err(incomplete("invalid_call_selection")),
        }
    }

    fn callable_call(
        &mut self,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let (declaration, substitution) = match call.target() {
            BoundCallableTarget::Declaration(callable) => {
                (callable.definition().symbol(), callable.substitution())
            }
            BoundCallableTarget::Predicate(predicate) => {
                (predicate.definition().into_any(), predicate.substitution())
            }
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_) => {
                return Err(incomplete("unsupported_callable_target"));
            }
        };

        let (callable, substitution) = self.declaration_reference(declaration, substitution)?;

        let mut arguments = Vec::new();
        let mut ordinals = Vec::new();
        let offset = u32::from(call.receiver().is_some());

        if let Some(receiver) = call.receiver() {
            let operand = self.expression(receiver.expression())?;
            let operand = match receiver.mode() {
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable => {
                    let kind = if receiver.mode() == bray_symbols::ReceiverMode::Shared { bray_symbols::BorrowKind::Shared } else { bray_symbols::BorrowKind::Mutable };
                    let ty = receiver.input_type(self.context.semantic_values())?;
                    let node = CheckedTemplateNodeId::new(index(self.nodes.len()));
                    self.nodes.push(CheckedTemplateNode::new(CheckedTemplateOperation::Borrow { kind, operand }, ty));
                    node
                }
                bray_symbols::ReceiverMode::Consuming | bray_symbols::ReceiverMode::ConsumingMutable => operand,
            };
            arguments.push(operand);
            ordinals.push(0);
        }

        for argument in call.arguments() {
            let SelectedArgument::Explicit {
                expression,
                conversion,
                ordinal,
                ..
            } = argument
            else {
                return Err(incomplete("non_explicit_template_argument"));
            };

            arguments.push(self.converted_expression(*expression, conversion)?);
            ordinals.push(ordinal.checked_add(offset).expect("checked callable ordinal includes its receiver"));
        }

        let implementation = match call.witnesses() {
            [] => None,
            [witness] => Some(
                self.implementation_reference(witness.witness())?,
            ),
            _ => return Err(incomplete("invalid_predicate_dispatch")),
        };

        Ok(CheckedTemplateOperation::ordered_call(
            callable,
            substitution,
            arguments,
            ordinals,
            implementation,
        ))
    }

    fn predicate_call(
        &mut self,
        predicate: &bray_bound_tree::SelectedPredicateApplication,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let callable = self.symbol_key(predicate.predicate().into_any())?;

        let substitution = predicate.substitution();

        let mut arguments = Vec::with_capacity(predicate.arguments().len());
        let mut ordinals = Vec::with_capacity(predicate.arguments().len());
        for argument in predicate.arguments() {
            let value = self.expression(argument.expression())?;
            let node = CheckedTemplateNodeId::new(index(self.nodes.len()));
            self.nodes.push(CheckedTemplateNode::new(CheckedTemplateOperation::Convert { value, target: argument.ty() }, argument.ty()));
            arguments.push(node);
            let parameter = self.context.symbols().predicate_parameter(argument.parameter())
                .expect("checked predicate argument retains its selected parameter");
            ordinals.push(parameter.ordinal());
        }

        Ok(CheckedTemplateOperation::ordered_call(callable, substitution, arguments, ordinals, None))
    }

}
