use super::build::{SourceTemplateBuilder, incomplete, index};
use crate::fact::FactQueryError;
use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundCallableTarget, BoundExpressionId, CheckedTemplateNode, CheckedTemplateNodeId,
    CheckedTemplateOperation, SelectedArgument, SemanticSelection,
};

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
                    let kind = if receiver.mode() == bray_symbols::ReceiverMode::Shared {
                        bray_symbols::BorrowKind::Shared
                    } else {
                        bray_symbols::BorrowKind::Mutable
                    };

                    let ty = receiver.input_type(self.context.semantic_values())?;
                    let node = CheckedTemplateNodeId::new(index(self.nodes.len()));

                    self.nodes.push(CheckedTemplateNode::new(
                        CheckedTemplateOperation::Borrow { kind, operand },
                        ty,
                    ));

                    node
                }
                bray_symbols::ReceiverMode::Consuming
                | bray_symbols::ReceiverMode::ConsumingMutable => operand,
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

            ordinals.push(
                ordinal
                    .checked_add(offset)
                    .expect("checked callable ordinal includes its receiver"),
            );
        }

        let dispatch = if let Some(dispatch) = call.resolution().trait_dispatch() {
            Some(self.constraint_dispatch(dispatch)?)
        } else {
            match call.witnesses() {
                [] => None,
                [witness] => {
                    let (key, substitution) = self.implementation_reference(witness.witness())?;

                    Some(bray_bound_tree::CheckedTemplateDispatch::Implementation(
                        key,
                        substitution,
                    ))
                }
                _ => return Err(incomplete("invalid_predicate_dispatch")),
            }
        };

        Ok(CheckedTemplateOperation::ordered_call(
            callable,
            substitution,
            arguments,
            ordinals,
            dispatch,
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

            self.nodes.push(CheckedTemplateNode::new(
                CheckedTemplateOperation::Convert {
                    value,
                    target: argument.ty(),
                },
                argument.ty(),
            ));

            arguments.push(node);

            let parameter = self
                .context
                .symbols()
                .predicate_parameter(argument.parameter())
                .expect("checked predicate argument retains its selected parameter");

            ordinals.push(parameter.ordinal());
        }

        Ok(CheckedTemplateOperation::ordered_call(
            callable,
            substitution,
            arguments,
            ordinals,
            None,
        ))
    }
}

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn constraint_dispatch(
        &self,
        dispatch: bray_symbols::TraitConstraintDispatch,
    ) -> Result<bray_bound_tree::CheckedTemplateDispatch, FactQueryError> {
        Ok(match dispatch {
            bray_symbols::TraitConstraintDispatch::Constraint { owner, ordinal } => {
                bray_bound_tree::CheckedTemplateDispatch::Constraint {
                    owner: self.symbol_key(owner.symbol())?,
                    ordinal,
                }
            }
            bray_symbols::TraitConstraintDispatch::TraitDefault(requirement) => {
                let (subject, substitution) = self.trait_default_requirement(requirement)?;

                bray_bound_tree::CheckedTemplateDispatch::TraitDefault {
                    subject,
                    substitution,
                }
            }
        })
    }

    pub(super) fn operator(
        &mut self,
        id: BoundExpressionId,
        operator: bray_bound_tree::BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        use bray_bound_tree::{BoundOperator, OperatorTarget, SelectedOperation};

        let Some(SemanticSelection::Operation(SelectedOperation::Operator {
            target,
            result_type,
            ..
        })) = self.selections.expression(id)
        else {
            panic!("checked operator {id:?} retains its selected implementation");
        };

        let target = *target;
        let result_type = *result_type;

        let (callable, dispatch) = match target {
            OperatorTarget::BuiltIn(_) => {
                return match operands {
                    [_] => self.unary(operator, operands),
                    [_, _] => self.binary(operator, operands),
                    _ => panic!("checked operator {id:?} retains unary or binary operands"),
                };
            }
            OperatorTarget::Trait {
                fulfillment,
                witness,
                ..
            } => {
                let (implementation, substitution) = self.implementation_reference(witness)?;

                (
                    fulfillment,
                    bray_bound_tree::CheckedTemplateDispatch::Implementation(
                        implementation,
                        substitution,
                    ),
                )
            }
            OperatorTarget::TraitConstraint {
                member, dispatch, ..
            } => (member, self.constraint_dispatch(dispatch)?),
        };

        let (callable, substitution) =
            self.declaration_reference(callable.definition().symbol(), callable.substitution())?;

        let arguments = operands
            .iter()
            .map(|operand| self.expression(*operand))
            .collect::<Result<Vec<_>, _>>()?;

        let call =
            CheckedTemplateOperation::call(callable, substitution, arguments, Some(dispatch));

        let relation = operator.ordering_test((), ()).is_some();

        if !relation && operator != BoundOperator::NotEqual {
            return Ok(call);
        }

        let ordering = relation.then(|| {
            self.context
                .compilation()
                .available_compiler_known_symbols()
                .ordering_representation()
                .expect("checked relational operator retains Ordering representation")
        });

        let call_type = match ordering {
            Some(ordering) => self
                .context
                .semantic_values()
                .intern_non_generic_named_type(bray_symbols::NamedTypeSymbolId::Union(
                    ordering.definition(),
                ))?,
            None => result_type,
        };

        let call_id = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.nodes.push(CheckedTemplateNode::new(call, call_type));

        let Some(ordering) = ordering else {
            return Ok(CheckedTemplateOperation::Unary {
                operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                operand: call_id,
            });
        };

        let (variant, matched_result) = operator
            .ordering_test(ordering.less_variant(), ordering.greater_variant())
            .expect("relational operator retains its Ordering test");

        let test = CheckedTemplateOperation::VariantTest {
            subject: call_id,
            variant: self.symbol_key(variant.into())?,
        };

        if matched_result {
            return Ok(test);
        }

        let test_id = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.nodes.push(CheckedTemplateNode::new(test, result_type));

        Ok(CheckedTemplateOperation::Unary {
            operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
            operand: test_id,
        })
    }
}
