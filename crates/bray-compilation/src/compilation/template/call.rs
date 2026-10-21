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
            Some(SemanticSelection::Call(call)) => self.callable_call(expression, call),
            Some(SemanticSelection::Predicate(predicate)) => self.predicate_call(predicate),
            Some(SemanticSelection::Operation(
                bray_bound_tree::SelectedOperation::Construction(construction),
            )) => self.construction(construction),
            _ => Err(incomplete("invalid_call_selection")),
        }
    }

    fn callable_call(
        &mut self,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let evidence = self
            .execution
            .and_then(|execution| execution.dependency_evidence(expression.into(), call.target()));

        let target = call
            .resolution()
            .declaration_source()
            .map(BoundCallableTarget::Declaration)
            .unwrap_or(call.target());

        if matches!(
            target,
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_)
        ) {
            let body = call
                .resolution()
                .anonymous_body()
                .map(|(body, _)| body)
                .or_else(|| {
                    evidence.and_then(bray_checker::ExecutionCallEvidence::anonymous_target)
                })
                .ok_or_else(|| {
                    incomplete("checked callable value lacks its selected implementation")
                })?;

            // The application and cache retain the same immutable anonymous body key.
            let body = self
                .context
                .compilation()
                .constant_body_template(body.clone(), self.context.cancellation())?;

            let template = body
                .result()
                .value()
                .as_ref()
                .expect("admitted anonymous const call retains its checked body");

            let mut arguments = vec![None; call.arguments().len()];

            for argument in call.arguments() {
                let SelectedArgument::Explicit {
                    expression,
                    ordinal,
                    conversion,
                    ..
                } = argument
                else {
                    return Err(incomplete(
                        "anonymous const default lacks its checked provider",
                    ));
                };

                arguments[usize::try_from(*ordinal).expect("argument ordinal fits host")] =
                    Some(self.converted_expression(*expression, conversion)?);
            }

            let result = self.apply_template(template, &arguments, None)?;

            let target = self
                .types
                .expression(expression)
                .expect("admitted call retains its result type")
                .ty();

            return Ok(CheckedTemplateOperation::Convert {
                value: result,
                target,
            });
        }

        let (declaration, substitution) = match target {
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
                continue;
            };

            arguments.push(self.converted_expression(*expression, conversion)?);

            ordinals.push(
                ordinal
                    .checked_add(offset)
                    .expect("checked callable ordinal includes its receiver"),
            );
        }

        for argument in call.arguments() {
            let SelectedArgument::Default {
                provider,
                ordinal,
                ty,
                ..
            } = argument
            else {
                continue;
            };

            let count = ordinal
                .checked_add(offset)
                .expect("checked default ordinal includes its receiver");

            let mut preceding =
                vec![None; usize::try_from(count).expect("checked default input count fits host")];

            for (argument, ordinal) in arguments.iter().zip(&ordinals) {
                if *ordinal < count {
                    preceding
                        [usize::try_from(*ordinal).expect("checked argument ordinal fits host")] =
                        Some(*argument);
                }
            }

            let preceding = preceding
                .into_iter()
                .map(|value| value.expect("default invocation has every earlier argument"));

            let provider = self.symbol_key((*provider).into())?;

            let value = self.push_node(
                CheckedTemplateOperation::ordered_call(
                    provider,
                    substitution,
                    preceding,
                    0..count,
                    None,
                ),
                *ty,
            );

            arguments.push(value);
            ordinals.push(count);
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

    pub(super) fn apply_template(
        &mut self,
        template: &bray_bound_tree::CheckedTemplate,
        arguments: &[Option<CheckedTemplateNodeId>],
        substitution: Option<bray_symbols::GenericSubstitutionId>,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        assert!(
            template.temporaries().is_empty(),
            "source const template has expression-owned materialization"
        );

        let resolver = crate::compilation::constant::CompilationConstantTemplateResolver::new(
            self.context.compilation(),
            self.context.cancellation(),
            self.context
                .imported_symbols()
                .map_err(crate::compilation::binder::binding_query_error)?,
        );

        let mut nodes = Vec::with_capacity(template.nodes().len());

        for node in template.nodes() {
            let result = if let CheckedTemplateOperation::Input(input) = node.operation() {
                let input = &template.inputs()
                    [usize::try_from(input.raw()).expect("checked input index fits host")];

                match input.kind() {
                    bray_bound_tree::CheckedTemplateInputKind::Parameter(ordinal) => arguments
                        [ordinal.to_index().expect("argument ordinal fits host")
                            + usize::from(template.inputs().iter().any(|input| {
                                input.kind() == &bray_bound_tree::CheckedTemplateInputKind::Receiver
                            }))]
                    .expect("checked template application supplies its parameter"),
                    bray_bound_tree::CheckedTemplateInputKind::Receiver => {
                        arguments[0].expect("checked template application supplies its receiver")
                    }
                    bray_bound_tree::CheckedTemplateInputKind::GenericConstant(key) => {
                        let Some(bray_symbols::AnySymbolId::GenericConstParameter(parameter)) =
                            bray_checker::ConstantTemplateResolver::symbol(&resolver, key)
                        else {
                            panic!("checked generic constant input resolves to its parameter");
                        };

                        let values = self.context.semantic_values();

                        let term = values.intern_constant_term(
                            bray_symbols::ConstantTermData::Parameter(parameter),
                        )?;

                        let term = substitution
                            .map(|substitution| values.substitute_constant_term(term, substitution))
                            .transpose()?
                            .unwrap_or(term);

                        let ty = substitution
                            .map(|substitution| values.substitute_type(input.ty(), substitution))
                            .transpose()?
                            .unwrap_or(input.ty());

                        self.push_node(
                            CheckedTemplateOperation::Constant {
                                term,
                                usage: Default::default(),
                            },
                            ty,
                        )
                    }
                    kind => {
                        let ordinal = self
                            .inputs
                            .iter()
                            .position(|input| input.kind() == kind)
                            .expect("applied const template retains its enclosing generic input");

                        self.push_node(
                            CheckedTemplateOperation::Input(
                                bray_bound_tree::CheckedTemplateInputId::new(index(ordinal)),
                            ),
                            input.ty(),
                        )
                    }
                }
            } else {
                let values = self.context.semantic_values();

                let mut operation = if let Some(substitution) = substitution {
                    node.operation().try_map_references(
                        |term| values.substitute_constant_term(*term, substitution),
                        |ty| values.substitute_type(*ty, substitution),
                        // Stable keys are shared by the cached graph and its rebased application.
                        |key| Ok(key.clone()),
                        |application| {
                            values.substitute_generic_substitution(*application, substitution)
                        },
                        |key| Ok(key.clone()),
                    )?
                } else {
                    // Lexical anonymous application shares all stable semantic references.
                    node.operation().clone()
                };

                operation.map_node_references(|node| {
                    nodes[usize::try_from(node.raw()).expect("checked node index fits host")]
                });

                let ty = substitution
                    .map(|substitution| values.substitute_type(node.ty(), substitution))
                    .transpose()?
                    .unwrap_or(node.ty());

                let result = CheckedTemplateNodeId::new(index(self.nodes.len()));
                self.nodes.push(CheckedTemplateNode::new(operation, ty)
                    .with_unsigned_integer_bits(node.unsigned_integer_bits()));
                result
            };

            let result = if matches!(node.operation(), CheckedTemplateOperation::Input(_))
                && let Some(bits) = node.unsigned_integer_bits()
            {
                let ty = self.nodes[usize::try_from(result.raw()).expect("checked input fits host")].ty();
                let bounded = CheckedTemplateNodeId::new(index(self.nodes.len()));
                self.nodes.push(CheckedTemplateNode::new(
                    CheckedTemplateOperation::Convert { value: result, target: ty }, ty,
                ).with_unsigned_integer_bits(Some(bits)));
                bounded
            } else {
                result
            };

            nodes.push(result);
        }

        Ok(nodes
            [usize::try_from(template.result().raw()).expect("checked template result fits host")])
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
