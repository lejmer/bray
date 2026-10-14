use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundCallableTarget, BoundUnit, BoundUnitKey, CheckedExpressionSemantics, SemanticSelection,
};
use bray_checker::{ExecutionCondition, TrustedContractInputs};
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
                | bray_bound_tree::BoundUnitKind::ConstantTemplate
        ) {
            return Ok(DiagnosticResult::new(inputs, diagnostics));
        }

        let trusted = self.trusted_predicate_producer(key)?;

        inputs.infer_result_witnesses = !trusted;

        if key.kind() != bray_bound_tree::BoundUnitKind::ConstantTemplate {
            let context = self.binding_context(cancellation)?;

            let owner = context.symbols().symbol_for_key(key.declared_owner())
                .expect("checked callable body retains its owner");

            inputs.inputs = self.execution_callable_inputs(owner, cancellation)?;

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

        let mut needs_trusted_flow =
            !inputs.requirements.is_empty() || !inputs.guarantees.is_empty();

        for (expression, node) in bound.tree().expressions() {
            if let Some(SemanticSelection::ScopedUse(scoped)) =
                semantics.selections().expression(expression)
            {
                self.add_scoped_trusted_contracts(
                    scoped,
                    &mut inputs,
                    cancellation,
                    &mut diagnostics,
                )?;

                needs_trusted_flow |= [
                    bray_bound_tree::BoundExecutionSite::ScopedEnter(expression),
                    bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                ]
                .into_iter()
                .filter_map(|site| inputs.calls.get(&site))
                .any(|contract| {
                    !contract.requirements.is_empty() || !contract.guarantees.is_empty()
                });

                continue;
            }

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
                    needs_trusted_flow |=
                        !contract.requirements.is_empty() || !contract.guarantees.is_empty();

                    inputs.calls.insert(expression.into(), contract);
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

            let Some(count) =
                self.trusted_call_input_count(call.target(), node, semantics, cancellation)?
            else {
                continue;
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

            let result = match call.resolution().result() {
                bray_bound_tree::BoundCallResult::Immediate(ty) => Some(ty),
                _ => None,
            };

            let mut contract = self.trusted_phase_contract(
                behavior,
                completion,
                result,
                decode,
                cancellation,
                &mut diagnostics,
            )?;

            if (!contract.preconditions.is_empty() || !contract.requirements.is_empty())
                && let BoundCallableTarget::Declaration(target) = call.target()
            {
                contract.preserves_inputs = self.call_preserves_required_inputs(
                    target,
                    behavior,
                    cancellation,
                    &mut diagnostics,
                )?;
            }

            needs_trusted_flow |=
                !contract.requirements.is_empty() || !contract.guarantees.is_empty();

            if !contract.requirements.is_empty()
                || !contract.guarantees.is_empty()
                || !contract.postconditions.is_empty()
            {
                // Ordinary observations can supply the extent used by a later trusted obligation.
                inputs.calls.insert(expression.into(), contract);
            }
        }

        if !needs_trusted_flow {
            // Ordinary-only bodies use their existing completion analysis.
            inputs.calls.clear();
        } else {
            inputs.constants =
                self.execution_constant_inputs(bound, semantics, cancellation, &mut diagnostics)?;

            self.trusted_static_inputs(semantics, &mut inputs, &mut diagnostics)?;
        }

        let witnesses = self.trusted_required_witnesses(&inputs.requirements, cancellation)?;

        diagnostics.add_range(witnesses.diagnostics().iter().cloned());
        inputs.required_witnesses = witnesses.into_parts().0;

        Ok(DiagnosticResult::new(inputs, diagnostics))
    }

    fn trusted_static_inputs(
        &self,
        semantics: &CheckedExpressionSemantics,
        inputs: &mut TrustedContractInputs,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        // Static initialization is demanded only by bodies that reason about authority.
        // Ordinary dependency discovery must not recursively check initializer bodies.
        for entry in semantics.selections().entries() {
            if let SemanticSelection::StaticReference(selection) = entry.selection() {
                let place = bray_checker::ExecutionPlace::static_instance(selection.clone());

                if !inputs.statics.contains_key(&place) {
                    let template = self.static_instance_template(selection.template().declaration())?;

                    diagnostics.add_range(template.diagnostics().iter().cloned());

                    let values = self.semantic_value_store()?;

                    let contract = values.substitute_dependency_contract(template.value().dependency_contract(), selection.substitution())
                        .expect("selected static substitution belongs to the same semantic store");

                    let contract = values.dependency_contract_template_data(contract);

                    inputs.statics.insert(place, contract.carriers().to_vec());
                }
            }
        }

        Ok(())
    }

    fn trusted_call_input_count(
        &self,
        target: BoundCallableTarget,
        node: &bray_bound_tree::BoundExpression,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<Option<usize>, FactQueryError> {
        let count = match target {
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
            BoundCallableTarget::Predicate(_) => return Ok(None),
        };

        Ok(Some(count))
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
}
