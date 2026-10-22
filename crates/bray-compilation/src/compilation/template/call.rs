use super::build::{SourceTemplateBuilder, incomplete, index};
use crate::fact::FactQueryError;
use bray_binder::{BindingQueryContext, SymbolQueryProvider};
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

    // rust-style: allow(function-too-large, reason = "one normalized invocation shares staged input values and exact projected state paths through call and completion")
    fn callable_call(
        &mut self,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        if self.kind == bray_bound_tree::CheckedTemplateKind::ConstantCallableBody
            && call.implementation_hook()
                == Some(bray_compiler_known::ImplementationHook::RangeNext)
        {
            return self.advance_range(expression);
        }

        if matches!(
            call.implementation_hook(),
            Some(
                bray_compiler_known::ImplementationHook::RangeSharedIterate
                    | bray_compiler_known::ImplementationHook::RangeMoveIterate
            )
        ) {
            let receiver = call
                .receiver()
                .expect("selected range iteration retains its receiver");

            let value = self.expression(receiver.expression())?;

            let target = self
                .types
                .expression(expression)
                .expect("range iteration retains its result")
                .ty();

            return Ok(CheckedTemplateOperation::Convert { value, target });
        }

        let target = call
            .resolution()
            .declaration_source()
            .map(BoundCallableTarget::Declaration)
            .unwrap_or(call.target());

        if matches!(
            target,
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_)
        ) {
            return self.anonymous_call(expression, call);
        }

        let (declaration, substitution) = match target {
            BoundCallableTarget::Declaration(callable) => {
                (callable.definition().symbol(), callable.substitution())
            }
            BoundCallableTarget::Predicate(predicate) => {
                (predicate.definition().into_any(), predicate.substitution())
            }
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_) => {
                unreachable!(
                    "anonymous and indirect template calls were handled before declaration lookup"
                );
            }
        };

        let observational = self.kind == bray_bound_tree::CheckedTemplateKind::ConstantCallableBody
            && call.constness() != bray_symbols::CallableConstness::Constant;

        let state_paths = if observational {
            self.mutable_input_paths(expression, declaration)?
        } else {
            std::collections::BTreeMap::new()
        };

        let (callable, substitution) = self.declaration_reference(declaration, substitution)?;

        let mut prepared_states = Vec::new();
        let mut arguments = Vec::new();
        let mut ordinals = Vec::new();
        let offset = u32::from(call.receiver().is_some());

        if let Some(receiver) = call.receiver() {
            let operand = if let Some(path) = state_paths.get(&0) {
                let (base, component) = self.prepare_state_path(path)?;

                prepared_states.push((0, base, component));

                component
            } else {
                self.expression(receiver.expression())?
            };

            let operand = match receiver.mode() {
                bray_symbols::ReceiverMode::Mutable if state_paths.contains_key(&0) => operand,
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

            let ordinal = ordinal
                .checked_add(offset)
                .expect("checked callable ordinal includes its receiver");

            let value = if let Some(path) = state_paths.get(&ordinal) {
                let (base, component) = self.prepare_state_path(path)?;

                prepared_states.push((ordinal, base, component));

                component
            } else {
                self.converted_expression(*expression, conversion)?
            };

            arguments.push(value);
            ordinals.push(ordinal);
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

        // Borrowed inputs observe their reached owner after every argument/default has evaluated.
        for (ordinal, _, _) in &prepared_states {
            let path = &state_paths[ordinal];
            let root = self.state_values[&path.root];
            let value = self.project_path(root, &path.projections)?;

            let index = ordinals
                .iter()
                .position(|actual| actual == ordinal)
                .expect("mutable formal retains its source input");

            arguments[index] = value;
        }

        let operation = CheckedTemplateOperation::ordered_call(
            callable,
            substitution,
            arguments,
            ordinals,
            dispatch,
        );

        if !observational {
            return Ok(operation);
        }

        let operation = operation.with_state_inputs(state_paths.keys().copied());

        let result_type = self
            .types
            .expression(expression)
            .expect("checked call retains its result type")
            .ty();

        let call = self.push_node(operation, result_type);

        for (ordinal, base, component) in prepared_states {
            let path = &state_paths[&ordinal];

            let component_type =
                self.nodes[usize::try_from(component.raw()).expect("component fits host")].ty();

            let value = self.push_node(
                CheckedTemplateOperation::StateAfter {
                    operation: call,
                    ordinal,
                },
                component_type,
            );

            let subject = self.state_values[&path.root];
            let ty = self.nodes[usize::try_from(subject.raw()).expect("owner fits host")].ty();

            let state = self.push_node(
                CheckedTemplateOperation::Update {
                    subject,
                    base,
                    component,
                    value,
                },
                ty,
            );

            self.assign_state(path.root, state);
        }

        Ok(CheckedTemplateOperation::Sequence {
            before: call,
            value: call,
        })
    }

    pub(super) fn apply_template(
        &mut self,
        template: &bray_bound_tree::CheckedTemplate,
        arguments: &[Option<CheckedTemplateNodeId>],
        substitution: Option<bray_symbols::GenericSubstitutionId>,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let resolver = crate::compilation::constant::CompilationConstantTemplateResolver::new(
            self.context.compilation(),
            self.context.cancellation(),
            self.context
                .imported_symbols()
                .map_err(crate::compilation::binder::binding_query_error)?,
        );

        let mut nodes = Vec::with_capacity(template.nodes().len());
        let mut temporaries = vec![None; template.temporaries().len()];

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

                if let CheckedTemplateOperation::Temporary(temporary) = &mut operation {
                    let old =
                        usize::try_from(temporary.raw()).expect("checked temporary fits host");

                    let rebased =
                        match temporaries[old] {
                            Some(rebased) => rebased,
                            None => {
                                let initializer = nodes[usize::try_from(
                                    template.temporaries()[old].initializer().raw(),
                                )
                                .expect("checked initializer fits host")];

                                let rebased = bray_bound_tree::CheckedTemplateTemporaryId::new(
                                    index(self.temporaries.len()),
                                );

                                self.temporaries.push(
                                    bray_bound_tree::CheckedTemplateTemporary::new(initializer, ty),
                                );

                                temporaries[old] = Some(rebased);

                                rebased
                            }
                        };

                    *temporary = rebased;
                }

                let result = CheckedTemplateNodeId::new(index(self.nodes.len()));

                self.nodes.push(
                    CheckedTemplateNode::new(operation, ty)
                        .with_unsigned_integer_bits(node.unsigned_integer_bits()),
                );

                result
            };

            let result = if matches!(node.operation(), CheckedTemplateOperation::Input(_))
                && let Some(bits) = node.unsigned_integer_bits()
            {
                let ty = self.nodes
                    [usize::try_from(result.raw()).expect("checked input fits host")]
                .ty();

                let bounded = CheckedTemplateNodeId::new(index(self.nodes.len()));

                self.nodes.push(
                    CheckedTemplateNode::new(
                        CheckedTemplateOperation::Convert {
                            value: result,
                            target: ty,
                        },
                        ty,
                    )
                    .with_unsigned_integer_bits(Some(bits)),
                );

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
    fn anonymous_call(
        &mut self,
        expression: BoundExpressionId,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let evidence = self
            .execution
            .and_then(|execution| execution.dependency_evidence(expression.into(), call.target()));

        let body = call
            .resolution()
            .anonymous_body()
            .map(|(body, _)| body)
            .or_else(|| evidence.and_then(bray_checker::ExecutionCallEvidence::anonymous_target))
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

        Ok(CheckedTemplateOperation::Convert {
            value: result,
            target,
        })
    }

    fn mutable_input_paths(
        &self,
        expression: BoundExpressionId,
        declaration: bray_symbols::AnySymbolId,
    ) -> Result<std::collections::BTreeMap<u32, bray_bound_tree::StoragePath>, FactQueryError> {
        let definition = bray_symbols::CallableDefinitionId::try_new(declaration)
            .expect("observational invocation retains a callable declaration");

        let signature = self
            .context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableSignatureQuery,
            >::new(definition.callable_symbol()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        let inputs = super::callable_template_inputs(
            self.context.compilation(),
            signature.value(),
            Vec::new(),
        )?;

        let mut paths = std::collections::BTreeMap::new();

        for (ordinal, input) in inputs.iter().enumerate() {
            if !matches!(
                self.context.semantic_values().type_data(input.ty).as_ref(),
                bray_symbols::TypeData::Borrow {
                    kind: bray_symbols::BorrowKind::Mutable,
                    ..
                }
            ) {
                continue;
            }

            let ordinal = u32::try_from(ordinal).expect("formal ordinal fits u32");

            let target = self
                .flow
                .invocation_input_target(
                    expression.into(),
                    bray_symbols::SymbolOrdinal::new(ordinal),
                )
                .expect("admitted mutable input retains its exact checked referent");

            let path = target
                .inline_state_path()
                .expect("admitted mutable input stays within its inline observation frame");

            paths.insert(ordinal, path);
        }

        Ok(paths)
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
