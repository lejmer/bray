use bray_binder::SymbolQueryProvider;
use bray_bound_tree::{
    BoundCallableTarget, BoundUnit, BoundUnitKey, CheckedExpressionSemantics, SemanticSelection,
};
use bray_checker::{ExecutionCondition, TrustedCallContract, TrustedContractInputs};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn trusted_contract_inputs(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<TrustedContractInputs>, FactQueryError> {
        let mut inputs = TrustedContractInputs::default();
        let mut diagnostics = DiagnosticBag::new();

        if !matches!(
            key.kind(),
            bray_bound_tree::BoundUnitKind::CallableBody
                | bray_bound_tree::BoundUnitKind::AnonymousCallable
        ) {
            return Ok(DiagnosticResult::new(inputs, diagnostics));
        }

        let trusted = self.trusted_predicate_producer(key)?;

        if key.kind() == bray_bound_tree::BoundUnitKind::AnonymousCallable {
            self.trusted_anonymous_inputs(
                key,
                bound,
                trusted,
                &mut inputs,
                &mut diagnostics,
                cancellation,
            )?;
        } else {
            let declared = self.execution_declaration(key.source().syntax())?;

            let requirements = self.predicate_condition_inputs(
                key,
                declared.value().requirements(),
                cancellation,
            )?;

            diagnostics.add_range(requirements.diagnostics().iter().cloned());

            for (condition, trusted, _) in requirements.into_parts().0 {
                if trusted {
                    inputs.requirements.push(condition);
                } else {
                    inputs.preconditions.push(condition);
                }
            }

            if !trusted {
                for domain in declared.value().domains() {
                    let guards =
                        self.predicate_condition_inputs(key, &domain.guards, cancellation)?;

                    let guarantees =
                        self.predicate_condition_inputs(key, &domain.postconditions, cancellation)?;

                    diagnostics.add_range(guards.diagnostics().iter().cloned());
                    diagnostics.add_range(guarantees.diagnostics().iter().cloned());

                    inputs
                        .guarantees
                        .extend(guarantees.into_parts().0.into_iter().filter_map(
                            |(mut condition, trusted, span)| {
                                if !trusted {
                                    return None;
                                }

                                for (guard, _, _) in guards.value() {
                                    condition = ExecutionCondition::Operation(
                                        bray_bound_tree::BoundOperator::LogicalOr,
                                        vec![
                                            ExecutionCondition::Operation(
                                                bray_bound_tree::BoundOperator::LogicalNot,
                                                vec![ExecutionCondition::Entry {
                                                    condition: std::sync::Arc::new(guard.clone()),
                                                    captured: false,
                                                }]
                                                .into(),
                                            ),
                                            condition,
                                        ]
                                        .into(),
                                    );
                                }

                                Some((condition, span))
                            },
                        ));
                }
            }
        }

        for (expression, node) in bound.tree().expressions() {
            let Some(SemanticSelection::Call(call)) = semantics.selections().expression(expression)
            else {
                if let Some(SemanticSelection::Operation(operation)) =
                    semantics.selections().expression(expression)
                    && let Some(contract) = self.trusted_operation_contract(
                        operation,
                        node,
                        cancellation,
                        &mut diagnostics,
                    )?
                {
                    inputs.calls.insert(expression, contract);
                }

                continue;
            };

            let behavior = call.phase_behaviors().invocation();

            let completion = call
                .phase_behaviors()
                .deferred_execution()
                .unwrap_or(behavior);

            if behavior.predicate_requirements().is_empty()
                && completion.predicate_guarantees().is_empty()
            {
                continue;
            }

            let count = match call.target() {
                BoundCallableTarget::Declaration(callable) => self
                    .execution_callable_inputs(callable.definition().symbol(), cancellation)?
                    .len(),
                BoundCallableTarget::Indirect(ty) => {
                    let data = self.semantic_value_store()?.type_data(ty);

                    let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                        panic!("selected indirect call must retain its callable type");
                    };

                    callable.parameters().len()
                }
                BoundCallableTarget::Anonymous(_) => {
                    let bray_bound_tree::BoundExpression::Call(source) = node else {
                        panic!("selected anonymous call must retain its source call");
                    };

                    let ty = semantics
                        .types()
                        .expression(source.callee())
                        .expect("selected anonymous callee must retain its type")
                        .ty();

                    let data = self.semantic_value_store()?.type_data(ty);

                    let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                        panic!("selected anonymous callee must have a callable type");
                    };

                    callable.parameters().len()
                }
                BoundCallableTarget::Predicate(_) => continue,
            };

            let values = self.semantic_value_store()?;

            let decode =
                |predicate: &bray_symbols::PredicateSemanticSummary| -> Result<_, FactQueryError> {
                    Ok(predicate
                        .condition()
                        .map(|term| {
                            bray_checker::execution_condition_from_type_term(values, term, count)
                        })
                        .transpose()?
                        .unwrap_or(ExecutionCondition::Unknown))
                };

            let guarantees = completion
                .predicate_guarantees()
                .iter()
                .filter(|predicate| predicate.is_trusted())
                .map(decode)
                .collect::<Result<Vec<_>, _>>()?;

            let result_is_witness = match call.resolution().result() {
                bray_bound_tree::BoundCallResult::Immediate(ty) => {
                    self.trusted_result_is_witness(ty, &guarantees, cancellation, &mut diagnostics)?
                }
                _ => true,
            };

            let contract = TrustedCallContract {
                arguments: Default::default(),
                completes: matches!(
                    call.resolution().result(),
                    bray_bound_tree::BoundCallResult::Immediate(_)
                ),
                result_is_witness,
                witness_subjects: self.trusted_predicate_subjects(
                    &guarantees,
                    cancellation,
                    &mut diagnostics,
                )?,
                requirements: behavior
                    .predicate_requirements()
                    .iter()
                    .filter(|predicate| predicate.is_trusted())
                    .map(decode)
                    .collect::<Result<Vec<_>, _>>()?,
                preconditions: behavior
                    .predicate_requirements()
                    .iter()
                    .filter(|predicate| !predicate.is_trusted())
                    .map(decode)
                    .collect::<Result<Vec<_>, _>>()?,
                guarantees,
                postconditions: completion
                    .predicate_guarantees()
                    .iter()
                    .filter(|predicate| !predicate.is_trusted())
                    .map(decode)
                    .collect::<Result<Vec<_>, _>>()?,
            };

            if !contract.requirements.is_empty() || !contract.guarantees.is_empty() {
                inputs.calls.insert(expression, contract);
            }
        }

        let witnesses = self.trusted_required_witnesses(&inputs.requirements, cancellation)?;

        diagnostics.add_range(witnesses.diagnostics().iter().cloned());
        inputs.required_witnesses = witnesses.into_parts().0;

        Ok(DiagnosticResult::new(inputs, diagnostics))
    }

    fn trusted_required_witnesses(
        &self,
        requirements: &[ExecutionCondition],
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<Vec<(ExecutionCondition, bray_checker::ExecutionPlace)>>,
        FactQueryError,
    > {
        let context = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;
        let symbols = self.available_compiler_known_symbols();
        let mut pending = requirements.iter().collect::<Vec<_>>();
        let mut witnesses = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        while let Some(condition) = pending.pop() {
            let ExecutionCondition::Trusted(condition) = condition else {
                if let ExecutionCondition::Operation(_, operands) = condition {
                    pending.extend(operands.iter());
                }

                continue;
            };

            let ExecutionCondition::Predicate(predicate, substitution, arguments) =
                condition.as_ref()
            else {
                continue;
            };

            let signature = context
                .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                    bray_symbols::PredicateSignatureTemplateQuery,
                >::new(*predicate))
                .map_err(crate::compilation::binder::binding_query_error)?;

            diagnostics.add_range(signature.diagnostics().iter().cloned());

            if signature.diagnostics().has_errors() || !signature.value().is_trusted() {
                continue;
            }

            let templates = signature
                .value()
                .parameters()
                .iter()
                .map(|parameter| parameter.ty().clone())
                .collect::<Vec<_>>();

            let checked = self.checked_constant_terms_for_templates_with_cancellation(
                templates.iter(),
                cancellation,
            )?;

            diagnostics.add_range(checked.diagnostics().iter().cloned());

            if checked.diagnostics().has_errors() {
                continue;
            }

            assert_eq!(
                templates.len(),
                arguments.len(),
                "checked predicate retains its signature arity"
            );

            for (template, argument) in templates.iter().zip(arguments.iter()) {
                let ty = bray_checker::resolve_type_expression_template(
                    values,
                    template,
                    checked.value(),
                )
                .map_err(FactQueryError::CheckerInfrastructure)?
                .expect(
                    "checked predicate parameter template must resolve before trusted analysis",
                );

                let ty = values.substitute_type(ty, *substitution)?;

                let owns_authority = match values.type_data(ty).as_ref() {
                    bray_symbols::TypeData::Named { definition, .. } => {
                        let role = match definition {
                            bray_symbols::NamedTypeSymbolId::Struct(symbol) => {
                                symbols.symbol_representation(*symbol)
                            }
                            bray_symbols::NamedTypeSymbolId::Union(symbol) => {
                                symbols.symbol_representation(*symbol)
                            }
                        };

                        !role.is_some_and(|role| {
                            role.numeric_kind().is_some()
                                || matches!(
                                    role,
                                    bray_compiler_known::RepresentationRole::ScalarBool
                                        | bray_compiler_known::RepresentationRole::ScalarChar
                                        | bray_compiler_known::RepresentationRole::Unit
                                        | bray_compiler_known::RepresentationRole::Never
                                )
                        }) && self.trusted_result_is_witness(
                            ty,
                            std::slice::from_ref(&ExecutionCondition::Trusted(condition.clone())),
                            cancellation,
                            &mut diagnostics,
                        )?
                    }
                    _ => true,
                };

                if owns_authority {
                    witnesses.extend(argument.inputs().into_iter().map(|place| {
                        (
                            ExecutionCondition::Trusted(condition.clone()),
                            place.clone(),
                        )
                    }));
                }
            }
        }

        Ok(DiagnosticResult::new(witnesses, diagnostics))
    }

    fn trusted_predicate_subjects(
        &self,
        conditions: &[ExecutionCondition],
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Vec<(ExecutionCondition, ExecutionCondition)>, FactQueryError> {
        let context = self.binding_context(cancellation)?;

        let mut pending = conditions
            .iter()
            .map(|condition| (condition, condition))
            .collect::<Vec<_>>();

        let mut subjects = Vec::new();

        while let Some((guarantee, condition)) = pending.pop() {
            match condition {
                ExecutionCondition::Trusted(condition) => {
                    let ExecutionCondition::Predicate(predicate, _, arguments) = condition.as_ref()
                    else {
                        continue;
                    };

                    let signature = context
                        .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                            bray_symbols::PredicateSignatureTemplateQuery,
                        >::new(*predicate))
                        .map_err(crate::compilation::binder::binding_query_error)?;

                    diagnostics.add_range(signature.diagnostics().iter().cloned());

                    if !signature.diagnostics().has_errors() && signature.value().is_trusted() {
                        subjects.extend(
                            arguments
                                .iter()
                                .map(|argument| (guarantee.clone(), argument.clone())),
                        );
                    }
                }
                ExecutionCondition::Operation(_, operands) => {
                    pending.extend(operands.iter().map(|operand| (guarantee, operand)))
                }
                _ => {}
            }
        }

        Ok(subjects)
    }

    fn trusted_result_is_witness(
        &self,
        ty: bray_symbols::TypeId,
        guarantees: &[ExecutionCondition],
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        if guarantees.is_empty() {
            return Ok(false);
        }

        let values = self.semantic_value_store()?;

        if !matches!(values.type_data(ty).as_ref(), bray_symbols::TypeData::Named { definition, .. }
            if matches!(super::foreign::compiler_known_representation(self, *definition),
                Some(bray_compiler_known::RepresentationRole::RawPointer | bray_compiler_known::RepresentationRole::DevicePointer)))
        {
            return Ok(true);
        }

        let provider = self.available_compiler_known_symbols().provider();

        let storage_predicates = [
            "ValidRead",
            "ValidWrite",
            "AlignedFor",
            "InitializedAs",
            "InitializedRangeAs",
            "NonOverlapping",
            "SharedAliasValid",
            "ExclusiveAliasValid",
            "EpochCurrent",
            "SynchronizedAccess",
            "MovementStable",
            "FinalizationPending",
            "OwnedAllocation",
            "DeviceValidRead",
            "DeviceValidWrite",
            "DeviceAlignedFor",
            "DeviceInitializedAs",
        ]
        .into_iter()
        .filter_map(|name| {
            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                .expect("closed memory predicate key must be valid");

            provider
                .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                .map(bray_symbols::PredicateDefinitionSymbolId::from)
        })
        .collect::<std::collections::BTreeSet<_>>();

        // A checked address view retains storage authority through its source dependencies.
        // Custom predicates on the address still introduce an ordinary witness value.
        let context = self.binding_context(cancellation)?;
        let mut pending = guarantees.iter().collect::<Vec<_>>();

        while let Some(condition) = pending.pop() {
            match condition {
                ExecutionCondition::Trusted(condition) => {
                    let ExecutionCondition::Predicate(predicate, _, _) = condition.as_ref() else {
                        continue;
                    };

                    if storage_predicates.contains(predicate) {
                        continue;
                    }

                    let signature = context
                        .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                            bray_symbols::PredicateSignatureTemplateQuery,
                        >::new(*predicate))
                        .map_err(crate::compilation::binder::binding_query_error)?;

                    diagnostics.add_range(signature.diagnostics().iter().cloned());

                    if signature.diagnostics().has_errors() || signature.value().is_trusted() {
                        return Ok(true);
                    }
                }
                ExecutionCondition::Operation(_, operands) => pending.extend(operands.iter()),
                ExecutionCondition::Unknown => return Ok(true),
                _ => {}
            }
        }

        Ok(false)
    }

    pub(in crate::compilation) fn trusted_predicate_producer(
        &self,
        key: &BoundUnitKey,
    ) -> Result<bool, FactQueryError> {
        let syntax = self.execution_declaration_node(key.source().syntax())?;
        let mut trusted = false;

        bray_syntax::walk_direct_child_nodes(&syntax, |node| {
            if matches!(
                node.kind(),
                bray_syntax::SyntaxKind::CallableModifiers
                    | bray_syntax::SyntaxKind::FunctionModifiers
                    | bray_syntax::SyntaxKind::TypeCallableMemberModifiers
                    | bray_syntax::SyntaxKind::TraitCallableMemberModifiers
                    | bray_syntax::SyntaxKind::ConstructorMemberModifiers
                    | bray_syntax::SyntaxKind::AsyncCapableLifecycleMemberModifiers
                    | bray_syntax::SyntaxKind::SyncLifecycleMemberModifiers
                    | bray_syntax::SyntaxKind::ScopeEnterMemberModifiers
            ) {
                trusted |= node
                    .tokens()
                    .any(|token| token.kind() == bray_syntax::SyntaxKind::TrustedKeyword);
            }

            bray_syntax::SyntaxWalkControl::SkipChildren
        });

        Ok(trusted)
    }

    fn trusted_anonymous_inputs(
        &self,
        key: &BoundUnitKey,
        bound: &BoundUnit,
        trusted: bool,
        inputs: &mut TrustedContractInputs,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<(), FactQueryError> {
        let declared =
            self.declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

        let template = declared
            .result()
            .value()
            .callable_type()
            .expect("anonymous unit has its callable type");

        let constants =
            self.checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

        let values = self.semantic_value_store()?;

        diagnostics.add_range(declared.result().diagnostics().iter().cloned());
        diagnostics.add_range(constants.diagnostics().iter().cloned());

        let Some(ty) =
            bray_checker::resolve_type_expression_template(values, template, constants.value())
                .map_err(FactQueryError::CheckerInfrastructure)?
        else {
            return Ok(());
        };

        let data = values.type_data(ty);

        let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
            panic!("anonymous unit type must retain its callable shape");
        };

        let bray_bound_tree::BoundUnitRoot::AnonymousCallable {
            callable: identity, ..
        } = bound.root()
        else {
            panic!("anonymous callable unit must retain its local declaration");
        };

        let references = bound
            .local_symbols()
            .anonymous_callable(identity)
            .expect("anonymous unit owns its local callable")
            .parameters()
            .iter()
            .map(|parameter| bray_bound_tree::BoundReferenceTarget::Local((*parameter).into()))
            .collect::<Vec<_>>();

        let invocation = callable.phase_behaviors().invocation();

        let completion = callable
            .phase_behaviors()
            .deferred_execution()
            .unwrap_or(invocation);

        let span = bray_source::SourceSpan::new(
            key.source().syntax().source_id(),
            key.source().syntax().full_range(),
        );

        for (predicates, postcondition) in [
            (invocation.predicate_requirements(), false),
            (completion.predicate_guarantees(), true),
        ] {
            for predicate in predicates {
                let condition = predicate
                    .condition()
                    .map(|term| {
                        bray_checker::execution_condition_from_term(values, term, &references)
                    })
                    .transpose()?
                    .unwrap_or(ExecutionCondition::Unknown);

                match (postcondition, predicate.is_trusted()) {
                    (false, true) => inputs.requirements.push(condition),
                    (false, false) => inputs.preconditions.push(condition),
                    (true, true) if !trusted => inputs.guarantees.push((condition, span)),
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn trusted_operation_contract(
        &self,
        operation: &bray_bound_tree::SelectedOperation,
        expression: &bray_bound_tree::BoundExpression,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TrustedCallContract>, FactQueryError> {
        use bray_bound_tree::{
            ConstructionTarget, ConversionTarget, IndexTarget, OperatorTarget, SelectedOperation,
        };

        let target = if let Some(target) = operation.operator_target() {
            match target {
                OperatorTarget::Trait { fulfillment, .. } => Some(fulfillment),
                OperatorTarget::TraitConstraint { member, .. } => Some(member),
                OperatorTarget::BuiltIn(_) => None,
            }
        } else {
            match operation {
                SelectedOperation::Construction(construction) => match construction.target() {
                    ConstructionTarget::TypeForm { callable, .. } => Some(callable),
                    _ => None,
                },
                SelectedOperation::Conversion(conversion) => match conversion.target() {
                    ConversionTarget::Trait { fulfillment, .. } => Some(*fulfillment),
                    ConversionTarget::TraitConstraint { member, .. } => Some(*member),
                    _ => None,
                },
                SelectedOperation::Index {
                    target: IndexTarget::Custom { fulfillment, .. },
                    ..
                } => Some(*fulfillment),
                SelectedOperation::Index {
                    target: IndexTarget::TraitConstraint { member, .. },
                    ..
                } => Some(*member),
                _ => None,
            }
        };

        let Some(target) = target else {
            return Ok(None);
        };

        let context = self.binding_context(cancellation)?;

        let Some(signature) =
            self.resolve_callable_instance_signature(&context, target, diagnostics)?
        else {
            return Ok(None);
        };

        let values = self.semantic_value_store()?;
        let data = values.type_data(signature.callable_type());

        let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
            panic!("selected operation signature must retain a callable type");
        };

        let behavior = callable.phase_behaviors().invocation();

        if behavior.predicate_requirements().is_empty()
            && behavior.predicate_guarantees().is_empty()
        {
            return Ok(None);
        }

        let inputs = self.execution_callable_inputs(target.definition().symbol(), cancellation)?;

        let operands = match operation {
            SelectedOperation::Construction(construction) => construction
                .inputs()
                .iter()
                .filter_map(|input| match input {
                    bray_bound_tree::SelectedConstructionInput::Explicit {
                        expression,
                        ordinal,
                        ..
                    } => Some((*ordinal as usize, *expression)),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => expression.child_expressions().enumerate().collect(),
        };

        let arguments = operands
            .into_iter()
            .filter_map(|(ordinal, expression)| {
                inputs.get(ordinal).map(|input| (*input, expression))
            })
            .collect();

        let decode =
            |predicate: &bray_symbols::PredicateSemanticSummary| -> Result<_, FactQueryError> {
                Ok(predicate
                    .condition()
                    .map(|term| bray_checker::execution_condition_from_term(values, term, &inputs))
                    .transpose()?
                    .unwrap_or(ExecutionCondition::Unknown))
            };

        let guarantees = behavior
            .predicate_guarantees()
            .iter()
            .filter(|predicate| predicate.is_trusted())
            .map(decode)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Some(TrustedCallContract {
            arguments,
            completes: true,
            result_is_witness: true,
            witness_subjects: self.trusted_predicate_subjects(
                &guarantees,
                cancellation,
                diagnostics,
            )?,
            requirements: behavior
                .predicate_requirements()
                .iter()
                .filter(|predicate| predicate.is_trusted())
                .map(decode)
                .collect::<Result<_, _>>()?,
            preconditions: behavior
                .predicate_requirements()
                .iter()
                .filter(|predicate| !predicate.is_trusted())
                .map(decode)
                .collect::<Result<_, _>>()?,
            guarantees,
            postconditions: behavior
                .predicate_guarantees()
                .iter()
                .filter(|predicate| !predicate.is_trusted())
                .map(decode)
                .collect::<Result<_, _>>()?,
        }))
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use crate::test_support::compilation;

    #[test]
    fn closed_writes_preserve_storage_validity_but_opaque_calls_revoke_it() {
        for (intervening, valid) in [("", true), ("opaque();", false)] {
            let source = format!(
                r#"
                trusted module app;
                func opaque() {{}}
                trusted func roundtrip(pos pointer: RawPointer<u8>) -> u8
                    requires(
                        trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
                        trusted core.memory.valid_write<u8>(pointer = pointer, count = 1),
                        trusted core.memory.aligned_for<u8>(pointer = pointer),
                    )
                    uses(raw_memory, unchecked_init)
                {{
                    core.memory.write<u8>(pointer, 1);
                    {intervening}
                    return core.memory.read<u8>(pointer);
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn initialized_storage_requirements_supply_consuming_operations() {
        for (signature, operation) in [
            (
                "pos storage: &mut Uninit<u8>",
                "core.memory.move_initialized<u8>(storage)",
            ),
            (
                "pos mut storage: Uninit<u8>",
                "core.memory.assume_initialized<u8>(storage)",
            ),
        ] {
            let argument = if signature.contains("&mut") {
                "storage"
            } else {
                "&mut storage"
            };

            let source = format!(
                r#"
                trusted module app;
                trusted func consume_initialized({signature}) -> u8
                    requires(trusted core.memory.uninit_initialized<u8>(storage = {argument}))
                    uses(unchecked_init)
                {{
                    return trusted {operation};
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{source}: {diagnostics:?}");
        }
    }

    #[test]
    fn closed_memory_ranges_can_supply_smaller_checked_ranges() {
        for (range, valid) in [(0, false), (1, true), (2, true)] {
            let source = format!(
                r#"
                trusted module app;
                trusted func write_byte(pos pointer: RawPointer<u8>)
                    requires(
                        trusted core.memory.valid_write<u8>(pointer = pointer, count = {range}),
                        trusted core.memory.aligned_for<u8>(pointer = pointer),
                    )
                    uses(raw_memory, unchecked_init)
                {{
                    core.memory.write<u8>(pointer, 1);
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn checked_nonempty_ranges_supply_one_element_obligations() {
        for (guard, valid) in [("count > 0", true), ("count == 0", false)] {
            let source = format!(
                r#"
                trusted module app;
                trusted func write_byte(pos pointer: RawPointer<u8>, count: usize)
                    requires(
                        trusted core.memory.valid_write<u8>(pointer = pointer, count = count),
                        trusted core.memory.aligned_for<u8>(pointer = pointer),
                    )
                    uses(raw_memory, unchecked_init)
                {{
                    if {guard} {{ core.memory.write<u8>(pointer, 1); }}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn borrowed_addresses_establish_live_access_conditions() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted func caller() -> u8 uses(raw_memory, unchecked_init) {
                let mut value: u8 = 1;
                let pointer = core.memory.address_of_mut<u8>(&mut value);
                trusted core.memory.write<u8>(pointer, 2);
                return trusted core.memory.read<u8>(pointer);
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn writing_through_an_address_does_not_extend_its_owner_lifetime() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted func caller() -> u8 uses(raw_memory, unchecked_init) {
                let unrelated: u8 = 1;
                let mut pointer = core.memory.null<u8>();
                {
                    let mut value: u8 = 1;
                    pointer = core.memory.address_of_mut<u8>(&mut value);
                    core.memory.write<u8>(pointer, 2);
                };
                return core.memory.read<u8>(pointer);
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn raw_addresses_cannot_bypass_custom_witness_copy_contracts() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(pointer: RawPointer<u8>);
            trusted func owner() -> RawPointer<u8> ensures(trusted live(result))
            {
                return core.memory.null<u8>();
            }
            func caller() { let pointer = owner(); let copy = pointer; }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        );
    }

    #[test]
    fn conditional_storage_guarantees_preserve_copyable_addresses() {
        for (ty, predicate) in [
            ("RawPointer<u8>", "core.memory.valid_read<u8>"),
            ("DevicePointer<u8>", "core.target.device_valid_read<u8>"),
        ] {
            for guard in ["flag", "enabled(flag)"] {
                let compilation = compilation(&format!(
                    r#"
                    trusted module app;
                    predicate enabled(flag: bool) = flag;
                    extern trusted func address(pos flag: bool) -> {ty}
                        ensures(!{guard} || trusted {predicate}(pointer = result, count = 1));
                    func caller() {{ let pointer = address(true); let copy = pointer; }}
                "#
                ));

                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{ty}, {guard}: {:?}",
                    compilation.check_diagnostics()
                );
            }
        }
    }

    #[test]
    fn device_addresses_cannot_bypass_custom_witness_copy_contracts() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(pointer: DevicePointer<u8>);
            extern trusted func owner() -> DevicePointer<u8> ensures(trusted live(result));
            func caller() { let pointer = owner(); let copy = pointer; }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        );
    }

    #[test]
    fn address_guarantees_expire_with_the_borrowed_storage_owner() {
        for (invocation, valid) in [("observe(pointer);", true), ("", false)] {
            let later = if valid { "" } else { "observe(pointer);" };

            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ byte: Uninit<u8>; }}
                trusted func address_view(pos owner: &Owner) -> RawPointer<u8>
                    executes(pure, total)
                    ensures(trusted core.memory.valid_read<u8>(pointer = result, count = 1))
                {{ return core.memory.uninit_pointer<u8>(&owner.byte); }}
                func observe(pos pointer: RawPointer<u8>)
                    requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1)) {{}}
                func caller() {{
                    let mut pointer = core.memory.null<u8>();
                    {{
                        let owner: Owner = {{ byte = core.memory.uninit<u8>() }};
                        pointer = address_view(&owner);
                        {invocation}
                    }};
                    {later}
                }}
            "#
            ));

            let diagnostics = compilation.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    #[test]
    fn conditional_trusted_guarantees_require_the_selected_entry_guard() {
        for (flag, valid) in [("true", true), ("false", false)] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ byte: u8; }}
                trusted predicate live(value: &Owner);
                trusted func owner(pos flag: bool) -> Owner
                    when(flag) {{ ensures(trusted live(&result)) }}
                {{ return {{ byte = 1 }}; }}
                func observe(pos value: &Owner) requires(trusted live(value)) {{}}
                func caller() {{ let value = owner({flag}); observe(&value); }}
            "#
            ));

            let diagnostics = compilation.check_diagnostics();

            assert_eq!(!diagnostics.has_errors(), valid, "{diagnostics:?}");
        }
    }

    #[test]
    fn vacuous_conditional_guarantees_do_not_restrict_ordinary_copies() {
        for guarantee in [
            "when(flag) { ensures(trusted live(&result)) }",
            "ensures(!flag || trusted live(&result))",
        ] {
            for (argument, copy, valid) in [
                ("false", "let copy = value;", true),
                ("true", "let copy = value;", false),
                ("flag", "let copy = value;", false),
                ("flag", "if !flag { let copy = value; }", true),
            ] {
                let compilation = compilation(&format!(
                    r#"
                    trusted module app;
                    @copy struct Owner {{ epoch: u64; }}
                    trusted predicate live(value: &Owner);
                    trusted func owner(pos flag: bool) -> Owner {guarantee} {{ return {{ epoch = 1 }}; }}
                    func caller(pos flag: bool) {{ let value = owner({argument}); {copy} }}
                "#
                ));

                assert_eq!(
                    !compilation.check_diagnostics().has_errors(),
                    valid,
                    "{guarantee}, {argument}, {copy}: {:?}",
                    compilation.check_diagnostics()
                );
            }
        }
    }

    #[test]
    fn output_authority_copy_checks_follow_current_completion_guards() {
        for guarantee in [
            "when(flag) { ensures(trusted live(value)) }",
            "ensures(!flag || trusted live(value))",
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                @copy struct Owner {{ epoch: u64; }}
                trusted predicate live(value: &mut Owner);
                trusted func initialize(pos flag: bool, pos value: &mut Owner) {guarantee} {{}}
                func caller(pos flag: bool) {{
                    let mut value: Owner = {{ epoch = 0 }};
                    initialize(flag, &mut value);
                    if !flag {{ let copy = value; }}
                }}
            "#
            ));

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{guarantee}: {:?}",
                compilation.check_diagnostics()
            );
        }

        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            trusted predicate live(value: &mut Owner);
            trusted func initialize(pos value: &mut Owner) -> u32
                ensures(result != 0 || trusted live(value)) { return 0; }
            func caller() {
                let mut value: Owner = { epoch = 0 };
                let status = initialize(&mut value);
                if status != 0 { let copy = value; }
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn structural_storage_preserves_whole_moved_and_fresh_owner_guarantees() {
        for body in [
            "let value = owner(); let holder: Holder = { owner = value }; observe(&holder.owner);",
            "let holder: Holder = { owner = owner() }; observe(&holder.owner);",
            "let holder: Holder = { owner = { let value = owner(); yield value; } }; observe(&holder.owner);",
            "let nested: Nested = { holder = { owner = owner() } }; observe(&nested.holder.owner);",
            "let holder: Holder = { owner = owner() }; let moved = holder; observe(&moved.owner);",
            "let holder: Holder = { owner = owner() }; let moved = holder.owner; observe(&moved);",
            "let mut holder: Holder = { owner = { epoch = 0 } }; holder.owner = owner(); observe(&holder.owner);",
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ epoch: u64; }}
                struct Holder {{ mut owner: Owner; }}
                struct Nested {{ holder: Holder; }}
                trusted predicate live(value: &Owner);
                trusted func owner() -> Owner ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
                func observe(pos value: &Owner) requires(trusted live(value)) {{}}
                func caller() {{ {body} }}
            "#
            ));

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn copying_a_structural_owner_cannot_duplicate_nested_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            @copy struct Holder { owner: Owner; }
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
            func caller() { let holder: Holder = { owner = owner() }; let copy = holder; }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        );
    }

    #[test]
    fn destructured_storage_preserves_whole_owner_guarantees() {
        for body in [
            "let number: u8 = 0; let (value, copied) = (owner(), number); observe(&value);",
            "let [value]: [Owner; 1] = [owner()]; observe(&value);",
            "let holder = Holder.Owned(owner()); match consume holder { case .Owned(value) { observe(&value); } }",
            "let value: Owner = owner(); let holder: Owner? = value; match consume holder { case ?present { observe(&present); } case none {} }",
            "let value = (owner(), owner()); observe(&value.0);",
            "let value = (owner(), owner()); observe(&value.1);",
            "let value: [Owner; 1] = [owner()]; observe(&value[0]);",
            "let number: u8 = 0; let (_, value) = (number, owner()); observe(&value);",
            "let [first, .., last]: [Owner; 2] = [owner(), owner()]; observe(&first);",
            "let [first, .., last]: [Owner; 2] = [owner(), owner()]; observe(&last);",
            "let value = { let number: u8 = 0; let pair = (owner(), number); let (value, copied) = pair; yield value; }; observe(&value);",
            "let value = { let values: [Owner; 1] = [owner()]; let [value] = values; yield value; }; observe(&value);",
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ epoch: u64; }}
                union Holder {{ Owned(pos value: Owner); }}
                trusted predicate live(value: &Owner);
                trusted func owner() -> Owner executes(pure, total)
                    ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
                func observe(pos value: &Owner) executes(pure, total) requires(trusted live(value)) {{}}
                func caller() {{ {body} }}
            "#
            ));

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn aggregate_copy_checks_apply_to_the_transferred_component() {
        for (body, valid) in [
            (
                "let number: u8 = 0; let pair = (owner(), number); let (value, copied) = pair;",
                false,
            ),
            (
                "let number: u8 = 0; let (value, copied) = (owner(), number);",
                true,
            ),
            (
                "let values: [Owner; 1] = [owner()]; let copy = values;",
                false,
            ),
            (
                "let number: u8 = 0; let pair = (owner(), number); let copied = pair.1;",
                true,
            ),
            (
                "let number: u8 = 0; let pair = (owner(), number); let copy = pair.0;",
                false,
            ),
            (
                "let values: [Owner; 1] = [owner()]; let copy = values[0];",
                false,
            ),
            ("let values: [Owner; 2] = [owner(); 2];", false),
            ("let values: [Owner; 1] = [owner(); 1];", true),
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                @copy struct Owner {{ epoch: u64; }}
                trusted predicate live(value: &Owner);
                trusted func owner() -> Owner ensures(trusted live(&result)) {{ return {{ epoch = 1 }}; }}
                func caller() {{ {body} }}
            "#
            ));

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{body}: {:?}",
                compilation.check_diagnostics()
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
                );
            }
        }
    }

    #[test]
    fn equal_aggregate_elements_cannot_share_borrowed_authority() {
        for body in [
            "let first: [u8; 1] = [0]; let second: [u8; 1] = [0]; establish(&first[0]); observe(&second[0]);",
            "let value = (false, false); establish_bool(&value.0); observe_bool(&value.1);",
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                trusted predicate live(value: &u8);
                trusted predicate live_bool(value: &bool);
                trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
                trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
                func observe(pos value: &u8) requires(trusted live(value)) {{}}
                func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
                func caller() {{ {body} }}
            "#
            ));

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    #[test]
    fn repeated_borrows_of_one_scalar_component_preserve_authority() {
        for body in [
            "let first: [u8; 1] = [0]; establish(&first[0]); observe(&first[0]);",
            "let value = (false, false); establish_bool(&value.0); observe_bool(&value.0);",
            "let values: [[u8; 1]; 1] = [[0]]; establish(&values[0][0]); observe(&values[0][0]);",
            "let value = ((false, false), false); establish_bool(&(value.0).0); observe_bool(&(value.0).0);",
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                trusted predicate live(value: &u8);
                trusted predicate live_bool(value: &bool);
                trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
                trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
                func observe(pos value: &u8) requires(trusted live(value)) {{}}
                func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
                func caller() {{ {body} }}
            "#
            ));

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{body}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn unknown_array_selectors_cannot_borrow_another_components_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {}
            func observe(pos value: &u8) requires(trusted live(value)) {}
            func caller(pos index: usize) {
                let values: [u8; 2] = [0, 0];
                establish(&values[0]);
                observe(&values[index]);
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn ordinary_predicate_assumptions_cannot_discharge_trusted_disjunctions() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func observe(pos value: u64, pos flag: bool) requires((trusted live(value)) || flag) {}
            func caller(pos value: u64, pos flag: bool) requires(live(value)) { observe(value, flag); }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn mixed_boolean_requirements_preserve_trusted_occurrences_and_ordinary_guards() {
        for (requirement, precondition, valid) in [
            ("(trusted live(value)) || flag", "live(value)", false),
            ("flag || trusted live(value)", "live(value)", false),
            ("(trusted live(value)) && flag", "live(value), flag", false),
            ("!(trusted live(value)) || flag", "!live(value)", false),
            ("flag || trusted live(value)", "flag", true),
            ("(trusted live(value)) || flag", "flag", true),
            ("ready(value) || trusted live(value)", "ready(value)", true),
            (
                "!(trusted live(value)) || ready(value)",
                "ready(value)",
                true,
            ),
            ("(trusted live(value)) || flag", "trusted live(value)", true),
        ] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: u64);
                trusted predicate ready(value: u64);
                func observe(pos value: u64, pos flag: bool) requires({requirement}) {{}}
                func caller(pos value: u64, pos flag: bool) requires({precondition}) {{ observe(value, flag); }}
            "#
            );

            let compilation = compilation(&source);

            if valid {
                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{source}: {:?}",
                    compilation.check_diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    #[test]
    fn selecting_an_ordinary_disjunct_does_not_grant_its_trusted_qualification() {
        for requirement in [
            "live(value) || trusted ready(value)",
            "(trusted ready(value)) || live(value)",
        ] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: u64);
                trusted predicate ready(value: u64);
                func observe(pos value: u64) requires(trusted live(value)) {{}}
                func caller(pos value: u64) requires({requirement}, !(trusted ready(value))) {{ observe(value); }}
            "#
            );

            let compilation = compilation(&source);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    #[test]
    fn trusted_disjunctions_retain_evidence_on_either_side() {
        for requirement in [
            "(trusted live(value)) || flag",
            "flag || trusted live(value)",
        ] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: u64);
                func observe(pos value: u64) requires(trusted live(value)) {{}}
                func caller(pos value: u64, pos flag: bool) requires({requirement}, !flag) {{ observe(value); }}
            "#
            );

            let compilation = compilation(&source);

            assert!(
                !compilation.check_diagnostics().has_errors(),
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn negative_entry_evidence_retains_witness_copy_obligations() {
        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            trusted predicate dangerous(value: &Owner);
            func duplicate(pos value: Owner) requires(!(trusted dangerous(&value))) {
                let copied = value;
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        );
    }

    #[test]
    fn negative_trusted_guarantees_retain_witness_copy_obligations() {
        for (body, valid) in [
            ("let value = direct_owner(); let copied = value;", false),
            ("let value = owner(false); let copied = value;", false),
            ("let value = owner(true); let copied = value;", true),
        ] {
            let source = format!(
                r#"
                trusted module app;
                @copy struct Owner {{ epoch: u64; }}
                trusted predicate dangerous(value: &Owner);
                trusted func direct_owner() -> Owner
                    ensures(!(trusted dangerous(&result))) {{ return {{ epoch = 1 }}; }}
                trusted func owner(pos flag: bool) -> Owner
                    ensures(flag || !(trusted dangerous(&result))) {{ return {{ epoch = 1 }}; }}
                func caller() {{ {body} }}
            "#
            );

            let compilation = compilation(&source);

            if valid {
                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{source}: {:?}",
                    compilation.check_diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
                );
            }
        }
    }

    #[test]
    fn ordinary_predicate_equalities_cannot_rename_trusted_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted predicate other(value: u64);
            func observe(pos value: u64) requires(trusted other(value)) {}
            func caller(pos value: u64) requires(trusted live(value), other(value) == live(value)) {
                observe(value);
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn equal_scalar_contents_do_not_identify_borrowed_predicate_subjects() {
        for requirement in ["trusted live(&first)", "ready(&first)"] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: &u8);
                trusted predicate ready(value: &u8);
                func observe(pos value: &u8) requires(trusted live(value)) {{}}
                trusted func make(pos value: &u8) -> u64 executes(pure, total)
                    when(ready(value)) {{ ensures(trusted result_live(result)) }} {{ return 0; }}
                trusted predicate result_live(value: u64);
                func observe_result(pos value: u64) requires(trusted result_live(value)) {{}}
                func caller(pos first: u8, pos second: u8) requires(first == second, {requirement}) {{
                    {body}
                }}
            "#,
                body = if requirement.starts_with("trusted") {
                    "observe(&second);"
                } else {
                    "observe_result(make(&second));"
                }
            );

            let compilation = compilation(&source);

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    #[test]
    fn mutation_cannot_reuse_an_ordinary_borrowed_guard_to_establish_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate ready(value: &u8);
            trusted predicate live(value: u64);
            trusted func make(pos value: &u8) -> u64 executes(pure, total)
                when(ready(value)) { ensures(trusted live(result)) } { return 0; }
            func observe(pos value: u64) requires(trusted live(value)) {}
            func caller(pos mut value: u8) requires(ready(&value)) {
                value = 1;
                observe(make(&value));
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn ordinary_borrowed_guards_select_the_producers_entry_domain() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate ready(value: &u8);
            trusted predicate live(value: u64);
            trusted func make(pos value: &u8) -> u64
                when(ready(value)) { ensures(trusted live(result)) } { return 0; }
            func observe(pos value: u64) requires(trusted live(value)) {}
            func caller(pos value: u8) requires(ready(&value)) { observe(make(&value)); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn changing_a_guard_does_not_erase_a_safe_producers_entry_obligation() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func make(pos mut flag: bool) -> u64
                when(flag) { ensures(trusted live(result)) } { flag = false; return 0; }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn ordinary_predicate_guards_select_trusted_conditional_guarantees() {
        for (precondition, valid) in [("requires(ready(value))", true), ("", false)] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: u64);
                trusted predicate ready(value: u64);
                trusted func make(pos value: u64) -> u64
                    when(ready(value)) {{ ensures(trusted live(result)) }} {{ return value; }}
                func observe(pos value: u64) requires(trusted live(value)) {{}}
                func caller(pos value: u64) {precondition} {{ observe(make(value)); }}
            "#
            );

            let compilation = compilation(&source);

            if valid {
                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{source}: {:?}",
                    compilation.check_diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    #[test]
    fn scalar_component_mutation_revokes_only_overlapping_authority() {
        for (body, valid) in [
            (
                "let mut values: [u8; 2] = [0, 0]; establish(&values[0]); values[1] = 1; observe(&values[0]);",
                true,
            ),
            (
                "let mut values: [u8; 2] = [0, 0]; establish(&values[0]); values[0] = 1; observe(&values[0]);",
                false,
            ),
            (
                "let mut value = (false, false); establish_bool(&value.0); value.1 = true; observe_bool(&value.0);",
                true,
            ),
            (
                "let mut value = (false, false); establish_bool(&value.0); value.0 = true; observe_bool(&value.0);",
                false,
            ),
            (
                "let mut value = ((false, false), false); establish_bool(&(value.0).0); (value.0).1 = true; observe_bool(&(value.0).0);",
                true,
            ),
            (
                "let mut value = ((false, false), false); establish_bool(&(value.0).0); value.0 = (false, false); observe_bool(&(value.0).0);",
                false,
            ),
        ] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: &u8);
                trusted predicate live_bool(value: &bool);
                trusted func establish(pos value: &u8) executes(pure, total) ensures(trusted live(value)) {{}}
                trusted func establish_bool(pos value: &bool) executes(pure, total) ensures(trusted live_bool(value)) {{}}
                func observe(pos value: &u8) requires(trusted live(value)) {{}}
                func observe_bool(pos value: &bool) requires(trusted live_bool(value)) {{}}
                func caller() {{ {body} }}
            "#
            );

            let compilation = compilation(&source);

            if valid {
                assert!(
                    !compilation.check_diagnostics().has_errors(),
                    "{source}: {:?}",
                    compilation.check_diagnostics()
                );
            } else {
                bray_testing::assert_goal_state_diagnostic_kind(
                    compilation.check_diagnostics(),
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    #[test]
    fn a_completion_status_can_be_copied_without_copying_the_output_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { epoch: u64; }
            trusted predicate live(value: &mut Owner);
            trusted func initialize(pos value: &mut Owner) -> u32
                ensures(result != 0 || trusted live(value)) { return 0; }
            func observe(pos value: &mut Owner) requires(trusted live(value)) {}
            func caller() {
                let mut value: Owner = { epoch = 1 };
                let status = initialize(&mut value);
                let copied_status = status;
                if copied_status == 0 { observe(&mut value); }
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn a_vacuous_entry_obligation_does_not_make_the_owner_a_witness() {
        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            func caller(pos value: Owner, flag: bool)
                requires(flag || trusted live(&value)) {
                if flag { let copied = value; }
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn conditional_callable_types_preserve_trusted_guarantees() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { byte: u8; }
            trusted predicate live(value: &Owner);
            callable Producer = trusted func(pos ready: bool) -> Owner
                when(ready) { ensures(trusted live(&result)) };
            trusted func owner(pos ready: bool) -> Owner
                when(ready) { ensures(trusted live(&result)) } { return { byte = 1 }; }
            func observe(pos value: &Owner) requires(trusted live(value)) {}
            func caller() { let produce: Producer = owner; let value = produce(true); observe(&value); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn generic_trait_fulfillments_preserve_exact_trusted_predicates() {
        for (predicate, valid) in [("live", true), ("other", false)] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                trusted predicate live<T>(value: &T);
                trusted predicate other<T>(value: &T);
                trait Observer<T> {{
                    static func observe(pos value: &T) requires(trusted live<T>(value));
                }}
                struct Watcher {{}}
                impl Watcher(Observer<u64>) {{
                    static func observe(pos value: &u64) requires(trusted {predicate}<u64>(value)) {{}}
                }}
            "#
            ));

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn trusted_caller_obligations_require_matching_evidence() {
        for (requirement, invocation, valid) in [
            ("", "use_owner(value);", false),
            ("", "trusted use_owner(value);", false),
            ("requires(trusted live(value))", "use_owner(value);", true),
            ("requires(trusted other(value))", "use_owner(value);", false),
        ] {
            let source = format!(
                r#"
                trusted module app;

                trusted predicate live(value: u64);
                trusted predicate other(value: u64);

                trusted func use_owner(pos value: u64)
                    requires(trusted live(value)) {{}}

                func caller(pos value: u64) {requirement}
                {{
                    {invocation}
                }}
            "#
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert_eq!(
                !diagnostics.has_errors(),
                valid,
                "{source}: {diagnostics:?}"
            );

            if !valid {
                bray_testing::assert_goal_state_diagnostic_kind(
                    diagnostics,
                    DiagnosticKind::CheckingTrustedObligationNotProven,
                );
            }
        }
    }

    #[test]
    fn trusted_producers_establish_live_result_witnesses() {
        let source = r#"
            trusted module app;

            struct Owner { mut epoch: u64; }

            trusted predicate live(owner: &Owner);

            trusted func owner() -> Owner
                ensures(trusted live(&result))
            {
                return { epoch = 1 };
            }

            trusted func observe(pos value: &Owner)
                requires(trusted live(value)) {}

            func caller()
            {
                let value = owner();
                observe(&value);
            }
        "#;

        let compilation = compilation(source);

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn mutation_invalidates_owner_witnesses() {
        let source = r#"
            trusted module app;

            struct Owner { mut epoch: u64; }
            trusted predicate live(owner: &Owner);

            trusted func owner() -> Owner ensures(trusted live(&result))
            {
                return { epoch = 1 };
            }

            trusted func observe(pos value: &Owner) requires(trusted live(value)) {}

            func caller()
            {
                let mut value = owner();
                value.epoch = 2;
                observe(&value);
            }
        "#;

        let compilation = compilation(source);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }
    #[test]
    fn safe_guarantees_cannot_fabricate_trusted_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func fabricated() -> u64 ensures(trusted live(result)) { return 1; }
            trusted func use_owner(pos value: u64) requires(trusted live(value)) {}
            func caller() { use_owner(fabricated()); }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn callable_conversion_cannot_erase_predicate_requirements() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func use_owner(pos value: u64) requires(trusted live(value)) {}
            func caller() { let erased: func(pos value: u64) = use_owner; erased(1); }
        "#,
        );

        assert!(compilation.check_diagnostics().has_errors());
    }

    #[test]
    fn callable_conversion_preserves_trusted_predicate_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            callable Ordinary = func(pos value: u64) requires(live(value));
            func obligated(pos value: u64) requires(trusted live(value)) {}
            func caller() { let erased: Ordinary = obligated; }
        "#,
        );

        assert!(compilation.check_diagnostics().has_errors());
    }

    #[test]
    fn raw_read_uses_declared_storage_obligations() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted func read_byte(pos pointer: RawPointer<u8>) -> u8
                requires(
                    trusted core.memory.valid_read<u8>(pointer = pointer, count = 1),
                    trusted core.memory.aligned_for<u8>(pointer = pointer),
                    trusted core.memory.initialized_as<u8>(pointer = pointer),
                )
                uses(raw_memory)
            { return trusted core.memory.read<u8>(pointer); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn ordinary_field_copy_does_not_outlive_its_witness() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { pointer: u64; }
            trusted predicate live(value: u64);
            trusted func owner() -> Owner ensures(trusted live(result.pointer)) { return { pointer = 1 }; }
            trusted func observe(pos value: u64) requires(trusted live(value)) {}
            func caller() {
                let mut pointer: u64 = 0;
                {
                    let value = owner();
                    pointer = value.pointer;
                };
                observe(pointer);
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn moving_a_witness_preserves_the_new_owner() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
            trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
            func caller() { let value = owner(); let moved = value; observe(&moved); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn named_callable_contracts_preserve_and_enforce_obligations() {
        for (enclosing, valid) in [("", false), ("requires(trusted live(value))", true)] {
            let source = format!(
                r#"
                trusted module app;
                trusted predicate live(value: u64);
                callable Action = func(pos value: u64) requires(trusted live(value));
                func observe(pos value: u64) requires(trusted live(value)) {{}}
                func caller(pos action: Action, pos value: u64) {enclosing} {{ action(value); }}
                func convert() -> Action {{ return observe; }}
            "#
            );

            let compilation = compilation(&source);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn implicit_copy_cannot_duplicate_a_live_witness() {
        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
            func caller() { let value = owner(); let copied = value; }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        );
    }

    #[test]
    fn enclosing_owner_authority_cannot_be_duplicated_by_copying() {
        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            func observe(pos value: &Owner) requires(trusted live(value)) {}
            func caller(pos value: Owner) requires(trusted live(&value)) {
                let copied = value;
                observe(&copied);
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedWitnessTransferNotProven,
        );
    }

    #[test]
    fn a_trust_boundary_does_not_hide_wrapper_requirements() {
        for tail in ["", "observe(&value);"] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ epoch: u64; }}
                trusted predicate live(value: &Owner);
                func observe(pos value: &Owner) requires(trusted live(value)) {{}}
                func caller() {{
                    let value: Owner = {{ epoch = 1 }};
                    trusted {{ observe(&value); observe(&value); }};
                    {tail}
                }}
            "#
            ));

            bray_testing::assert_goal_state_diagnostic_kind(
                compilation.check_diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }

    #[test]
    fn an_acknowledged_producer_establishes_guarantees_when_its_obligation_is_exposed() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { epoch: u64; }
            trusted predicate admitted(epoch: u64);
            trusted predicate live(value: &Owner);
            trusted func owner(pos epoch: u64) -> Owner
                requires(trusted admitted(epoch)) ensures(trusted live(&result))
                { return { epoch = epoch }; }
            func observe(pos value: &Owner) requires(trusted live(value)) {}
            func caller(pos epoch: u64) requires(trusted admitted(epoch)) {
                let value = trusted owner(epoch); observe(&value);
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn unknown_observations_do_not_merge_unrelated_value_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            @copy struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            trusted func establish(pos value: &Owner) executes(pure, total)
                ensures(trusted live(value)) {}
            func caller() {
                let value: Owner = { epoch = 1 };
                let unrelated: Owner = { epoch = 2 };
                establish(&value);
                let copied = unrelated;
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn equal_scalar_values_do_not_share_borrowed_authority() {
        for (observed, valid) in [("value", true), ("unrelated", false)] {
            let compilation = compilation(&format!(
                r#"
            trusted module app;
            trusted predicate live(value: &u8);
            trusted func establish(pos value: &u8) executes(pure, total)
                ensures(trusted live(value)) {{}}
            func observe(pos value: &u8) requires(trusted live(value)) {{}}
            func caller() {{
                let value: u8 = 1;
                let unrelated: u8 = 1;
                establish(&value);
                observe(&{observed});
            }}
        "#
            ));

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn safe_forwarders_preserve_existing_witnesses() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            trusted func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
            func forward(pos value: Owner) -> Owner
                requires(trusted live(&value)) ensures(trusted live(&result)) { return value; }
            trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
            func caller() { let value = forward(owner()); observe(&value); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn ordinary_conditions_do_not_create_trusted_authority() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func ordinary(pos value: u64) requires(live(value)) {}
            trusted func observe(pos value: u64) requires(trusted live(value)) {}
            func caller(pos value: u64) requires(live(value)) { observe(value); }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn ordinary_producer_requirements_guard_trusted_guarantees() {
        for (value, valid) in [(1, true), (0, false)] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ epoch: u64; }}
                trusted predicate live(value: &Owner);
                trusted func owner(pos epoch: u64) -> Owner requires(epoch == 1)
                    ensures(trusted live(&result)) {{ return {{ epoch = epoch }}; }}
                trusted func observe(pos value: &Owner) requires(trusted live(value)) {{}}
                func caller() {{ let value = owner({value}); observe(&value); }}
            "#
            ));

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn ordinary_equalities_match_existing_trusted_conditions() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            trusted func observe(pos value: u64) requires(trusted live(value)) {}
            func caller(pos left: u64, pos right: u64)
                requires(left == right, trusted live(left)) { observe(right); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn deferred_producers_require_live_entry_evidence_when_their_body_begins() {
        for (intervening, valid) in [
            ("", true),
            ("opaque();", false),
            ("if flag { opaque(); }", false),
            ("epoch = 2;", false),
        ] {
            let compilation = compilation(&format!(
                r#"
                trusted module app;
                struct Owner {{ epoch: u64; }}
                trusted predicate admitted(epoch: u64);
                trusted predicate live(value: &Owner);
                trusted async func owner(pos epoch: u64) -> Owner
                    requires(trusted admitted(epoch)) ensures(trusted live(&result))
                    {{ return {{ epoch = epoch }}; }}
                func observe(pos value: &Owner) requires(trusted live(value)) {{}}
                func opaque() {{}}
                async func caller(pos mut epoch: u64, flag: bool) requires(trusted admitted(epoch)) {{
                    let future = owner(epoch);
                    {intervening}
                    let value = await future;
                    observe(&value);
                }}
            "#
            ));

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{intervening}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn deferred_producers_publish_witnesses_after_await() {
        let compilation = compilation(
            r#"
            trusted module app;
            struct Owner { epoch: u64; }
            trusted predicate live(value: &Owner);
            trusted async func owner() -> Owner ensures(trusted live(&result)) { return { epoch = 1 }; }
            trusted func observe(pos value: &Owner) requires(trusted live(value)) {}
            async func caller() { let future = owner(); let value = await future; observe(&value); }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn nested_trust_in_requirements_cannot_hide_obligations() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: u64);
            func observe(pos value: u64) requires((trusted live(value)) && true) {}
            func caller() { observe(1); }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn cleanup_checks_destructor_against_finalizer_completion() {
        for (guarantee, execution, valid) in [
            ("ensures(trusted finished(&self))", "executes(total)", true),
            ("ensures(trusted finished(&self))", "", false),
            ("", "executes(total)", false),
        ] {
            let source = format!(
                r#"
                trusted module app;

                trusted predicate finished(value: &Owner);

                struct Owner {{
                    epoch: u64;

                    trusted finalize() {execution} {guarantee} {{}}

                    trusted destruct() requires(trusted finished(&self)) {{}}
                }}

                func caller() {{
                    let value: Owner = {{ epoch = 1 }};
                }}
            "#
            );

            let compilation = compilation(&source);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn represented_child_cleanup_checks_its_trusted_requirements() {
        let compilation = compilation(
            r#"
            trusted module app;
            trusted predicate live(value: &Child);
            struct Child {
                byte: u8;
                trusted destruct() requires(trusted live(&self)) {}
            }
            struct Parent { child: Child; }
            func caller() { let parent: Parent = { child = { byte = 1 } }; }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn recursive_owned_cleanup_does_not_require_infinite_type_expansion() {
        let compilation = compilation(
            r#"
            module app;
            struct Node { next: box[Heap] Node; }
            func dispose(pos node: Node) {}
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn finalization_revokes_prior_destructor_evidence() {
        let compilation = compilation(
            r#"
            trusted module app;

            trusted predicate live(value: &Owner);

            struct Owner {
                epoch: u64;

                finalize() {}

                trusted destruct() requires(trusted live(&self)) {}
            }

            trusted func owner() -> Owner ensures(trusted live(&result)) {
                return { epoch = 1 };
            }

            func caller() {
                let value = owner();
            }
        "#,
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.check_diagnostics(),
            DiagnosticKind::CheckingTrustedObligationNotProven,
        );
    }

    #[test]
    fn anonymous_calls_preserve_trusted_requirements() {
        for (requirement, valid) in [("", false), ("requires(trusted live(value))", true)] {
            let source = format!(
                r#"
                trusted module app;

                trusted predicate live(value: u64);

                func caller(pos value: u64) {requirement} {{
                    let action = lambda (pos value: u64)
                        requires(trusted live(value))
                    {{}};

                    action(value);
                }}
            "#
            );

            let compilation = compilation(&source);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }

    #[test]
    fn anonymous_bodies_receive_their_declared_trusted_requirements() {
        let compilation = compilation(
            r#"
            trusted module app;

            trusted predicate live(value: u64);

            trusted func observe(pos value: u64) requires(trusted live(value)) {}

            func caller(pos value: u64) requires(trusted live(value)) {
                let action = lambda (pos value: u64) requires(trusted live(value)) {
                    observe(value);
                };

                action(value);
            }
        "#,
        );

        assert!(
            !compilation.check_diagnostics().has_errors(),
            "{:?}",
            compilation.check_diagnostics()
        );
    }

    #[test]
    fn trusted_predicate_guarantees_preserve_execution_property_proofs() {
        for (body, valid) in [("return { epoch = 1 };", true), ("loop {}", false)] {
            let source = format!(
                r#"
                trusted module app;

                struct Owner {{ epoch: u64; }}

                trusted predicate live(value: &Owner);

                trusted func owner() -> Owner
                    executes(pure, total)
                    ensures(trusted live(&result))
                {{
                    {body}
                }}
            "#
            );

            let compilation = compilation(&source);

            assert_eq!(
                !compilation.check_diagnostics().has_errors(),
                valid,
                "{source}: {:?}",
                compilation.check_diagnostics()
            );
        }
    }
}
