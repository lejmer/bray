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

        inputs.predicates =
            self.execution_predicate_inputs(semantics, cancellation, &mut diagnostics)?;

        let callable_body = matches!(
            key.kind(),
            bray_bound_tree::BoundUnitKind::CallableBody
                | bray_bound_tree::BoundUnitKind::AnonymousCallable
                | bray_bound_tree::BoundUnitKind::ConstantTemplate
        );

        let trusted = callable_body && self.trusted_predicate_producer(key)?;

        if key.callable_type_header().is_some() {
            inputs.inputs = self.execution_unit_inputs(key, cancellation)?;
        }

        // Predicate units check obligations, without inferring returned witness authority.
        inputs.infer_result_witnesses = callable_body && !trusted;

        if callable_body && key.kind() != bray_bound_tree::BoundUnitKind::ConstantTemplate {
            inputs.inputs = self.execution_unit_inputs(key, cancellation)?;

            let declared = self.execution_declaration(key.source().syntax())?;

            let requirements = self.predicate_condition_inputs(
                key,
                declared.value().requirements(),
                cancellation,
            )?;

            diagnostics.add_range(requirements.diagnostics().iter().cloned());

            for (condition, domain, _) in requirements.into_parts().0 {
                if domain == bray_symbols::PredicateDomain::Trusted {
                    inputs.requirements.push(condition);
                } else {
                    inputs.preconditions.push(condition);
                }
            }

            if !trusted {
                for domain in declared
                    .value()
                    .domains()
                    .iter()
                    .filter(|domain| !domain.postconditions.is_empty())
                {
                    let guards =
                        self.predicate_condition_inputs(key, &domain.guards, cancellation)?;

                    let guarantees =
                        self.predicate_condition_inputs(key, &domain.postconditions, cancellation)?;

                    diagnostics.add_range(guards.diagnostics().iter().cloned());
                    diagnostics.add_range(guarantees.diagnostics().iter().cloned());

                    inputs
                        .guarantees
                        .extend(guarantees.into_parts().0.into_iter().filter_map(
                            |(mut condition, domain, span)| {
                                if domain != bray_symbols::PredicateDomain::Trusted {
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

        if key.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault {
            inputs.inputs = self.execution_unit_inputs(key, cancellation)?;
        }

        for (expression, node) in bound.tree().expressions() {
            if let Some(
                selection @ (SemanticSelection::ScopedUse(_) | SemanticSelection::Iteration(_)),
            ) = semantics.selections().expression(expression)
            {
                self.add_implicit_trusted_contracts(
                    selection,
                    &mut inputs,
                    cancellation,
                    &mut diagnostics,
                )?;

                continue;
            }

            let Some(SemanticSelection::Call(call)) = semantics.selections().expression(expression)
            else {
                if let Some(SemanticSelection::Operation(operation)) =
                    semantics.selections().expression(expression)
                    && let Some(contract) = self.trusted_operation_contract(
                        expression,
                        operation,
                        node,
                        cancellation,
                        &mut diagnostics,
                    )?
                {
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
                && let BoundCallableTarget::Declaration(target)
                | BoundCallableTarget::TraitView { member: target, .. } = call.target()
            {
                contract.preserves_inputs = self.call_preserves_required_inputs(
                    target,
                    behavior,
                    cancellation,
                    &mut diagnostics,
                )?;
            }

            if !contract.requirements.is_empty()
                || !contract.execution_requirements.is_empty()
                || !contract.preconditions.is_empty()
                || !contract.guarantees.is_empty()
                || !contract.postconditions.is_empty()
            {
                // Ordinary observations can supply the extent used by a later trusted obligation.
                inputs.calls.insert(expression.into(), contract);
            }
        }

        let needs_predicate_flow = !inputs.requirements.is_empty()
            || !inputs.preconditions.is_empty()
            || !inputs.guarantees.is_empty()
            || !inputs.calls.is_empty();

        inputs.constants.extend(self.execution_constant_inputs(
            bound,
            semantics,
            cancellation,
            &mut diagnostics,
        )?);

        if needs_predicate_flow {
            inputs
                .statics
                .extend(semantics.selections().entries().iter().filter_map(|entry| {
                    match entry.selection() {
                        SemanticSelection::StaticReference(selection) => Some(selection.clone()),
                        _ => None,
                    }
                }));
        }

        let witnesses = self.trusted_required_witnesses(&inputs.requirements, cancellation)?;

        diagnostics.add_range(witnesses.diagnostics().iter().cloned());
        inputs.required_witnesses = witnesses.into_parts().0;

        Ok(DiagnosticResult::new(inputs, diagnostics))
    }

    fn trusted_call_input_count(
        &self,
        target: BoundCallableTarget,
        node: &bray_bound_tree::BoundExpression,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<Option<usize>, FactQueryError> {
        let count = match target {
            BoundCallableTarget::Declaration(callable)
            | BoundCallableTarget::TraitView {
                member: callable, ..
            } => self
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
