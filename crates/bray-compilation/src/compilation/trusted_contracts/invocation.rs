use bray_checker::{ExecutionCondition, TrustedCallContract, TrustedContractInputs};
use bray_diagnostics::DiagnosticBag;

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn trusted_operation_contract(
        &self,
        owner: bray_bound_tree::BoundExpressionId,
        operation: &bray_bound_tree::SelectedOperation,
        expression: &bray_bound_tree::BoundExpression,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TrustedCallContract>, FactQueryError> {
        use bray_bound_tree::SelectedOperation;

        if let SelectedOperation::Construction(construction) = operation
            && let Some(contract) = self.trusted_storage_formation(owner, construction, cancellation)?
        {
            return Ok(Some(contract));
        }

        let Some(target) = operation.invoked_callable() else {
            return Ok(None);
        };

        let context = self.binding_context(cancellation)?;

        let Some(signature) =
            self.resolve_callable_instance_signature(&context, target, diagnostics)?
        else {
            return Ok(None);
        };

        let inputs = self.execution_callable_inputs(target.definition().symbol(), cancellation)?;

        let operands = match operation {
            SelectedOperation::Construction(construction) => {
                let staged = construction.inputs().iter().any(|input| matches!(input,
                    bray_bound_tree::SelectedConstructionInput::Default { .. }));
                construction.inputs().iter().map(|input| {
                    let (bray_bound_tree::SelectedConstructionInput::Explicit { ordinal, .. }
                        | bray_bound_tree::SelectedConstructionInput::Default { ordinal, .. }) = input;
                    let site = if staged {
                        bray_bound_tree::BoundExecutionSite::CallInput { expression: owner, ordinal: bray_symbols::SymbolOrdinal::new(*ordinal) }
                    } else {
                        let bray_bound_tree::SelectedConstructionInput::Explicit { expression, .. } = input else {
                            unreachable!("unstaged constructor has explicit inputs");
                        };
                        (*expression).into()
                    };
                    (usize::try_from(*ordinal).expect("input ordinal fits host"), site)
                }).collect::<Vec<_>>()
            }
            _ => expression.child_expressions().enumerate().map(|(ordinal, expression)| (ordinal, expression.into())).collect(),
        };

        let arguments = operands
            .into_iter()
            .filter_map(|(ordinal, expression)| {
                inputs.get(ordinal).map(|input| (*input, expression))
            })
            .collect();

        self.trusted_invocation_contract(target, &signature, arguments, cancellation, diagnostics)
    }

    pub(super) fn add_implicit_trusted_contracts(
        &self,
        selection: &bray_bound_tree::SemanticSelection,
        inputs: &mut TrustedContractInputs,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        use bray_bound_tree::{BoundExecutionSite as Site, SemanticSelection};
        match selection {
            SemanticSelection::ScopedUse(scoped) => {
                for (site, selected, source) in [
                    (Site::ScopedEnter(scoped.expression()), scoped.enter(), scoped.initializer().into()),
                    (Site::ScopedExit(scoped.expression()), scoped.exit(), Site::ScopedEnter(scoped.expression())),
                ] {
                    self.add_implicit_invocation(site, selected.0, &selected.1, source,
                        inputs, cancellation, diagnostics)?;
                }
            }
            SemanticSelection::Iteration(iteration) => {
                let context = self.binding_context(cancellation)?;
                for (site, target, source) in [
                    (Site::Iterate(iteration.expression()), iteration.iterate(), iteration.source().into()),
                    (Site::Next(iteration.expression()), iteration.next(), Site::Iterate(iteration.expression())),
                ] {
                    let Some(signature) = self.resolve_callable_instance_signature(&context, target, diagnostics)? else {
                        continue;
                    };
                    self.add_implicit_invocation(site, target, &signature, source,
                        inputs, cancellation, diagnostics)?;
                }
            }
            _ => unreachable!("implicit contracts require a selected protocol"),
        }
        Ok(())
    }

    fn add_implicit_invocation(
        &self,
        site: bray_bound_tree::BoundExecutionSite,
        target: bray_symbols::CallableInstanceData,
        signature: &bray_symbols::CallableSignature,
        source: bray_bound_tree::BoundExecutionSite,
        inputs: &mut TrustedContractInputs,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        let formal = if let Some(receiver) = signature.receiver() {
            assert!(signature.parameters().is_empty(), "implicit receiver invocation has one input");
            receiver.parameter().into()
        } else {
            assert_eq!(signature.parameters().len(), 1, "implicit invocation has one input");
            signature.parameters()[0].parameter().into()
        };
        if let Some(contract) = self.trusted_invocation_contract(target, signature,
            std::collections::BTreeMap::from([(bray_bound_tree::BoundReferenceTarget::Surface(formal), source)]),
            cancellation, diagnostics)?
        {
            inputs.calls.insert(site, contract);
        }
        Ok(())
    }

    fn trusted_invocation_contract(
        &self,
        target: bray_symbols::CallableInstanceData,
        signature: &bray_symbols::CallableSignature,
        arguments: std::collections::BTreeMap<
            bray_bound_tree::BoundReferenceTarget,
            bray_bound_tree::BoundExecutionSite,
        >,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TrustedCallContract>, FactQueryError> {
        let values = self.semantic_value_store()?;
        let data = values.type_data(signature.callable_type());

        let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
            panic!("selected invocation signature retains its callable type");
        };

        let behavior = callable.phase_behaviors().invocation();

        let completion = callable
            .phase_behaviors()
            .deferred_execution()
            .unwrap_or(behavior);

        let inputs = self.execution_callable_inputs(target.definition().symbol(), cancellation)?;

        let decode =
            |predicate: &bray_symbols::PredicateSemanticSummary| -> Result<_, FactQueryError> {
                Ok(predicate
                    .condition()
                    .map(|term| bray_checker::execution_condition_from_term(values, term, &inputs))
                    .transpose()?
                    .unwrap_or(ExecutionCondition::Unknown))
            };

        let mut contract = self.trusted_phase_contract(
            behavior,
            completion,
            Some(signature.result()),
            decode,
            cancellation,
            diagnostics,
        )?;

        contract.preserves_inputs =
            self.call_preserves_required_inputs(target, behavior, cancellation, diagnostics)?;

        contract.arguments = arguments;

        Ok(Some(contract))
    }

    pub(super) fn call_preserves_required_inputs(
        &self,
        target: bray_symbols::CallableInstanceData,
        behavior: &bray_symbols::CallablePhaseBehavior,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<bool, FactQueryError> {
        if let Some(owner) = self.execution_contract_owner(target.definition().symbol())? {
            let declaration = self.execution_declaration(owner.source().syntax())?;

            return Ok(declaration.value().domains().iter().any(|domain| {
                domain.guards.is_empty()
                    && domain
                        .properties
                        .iter()
                        .any(|property| property.property == bray_checker::ExecutionProperty::Pure)
            }));
        }

        let contract = self.imported_execution_contract(target, diagnostics, cancellation)?;

        let requirements = behavior
            .predicate_requirements()
            .iter()
            .filter_map(|predicate| predicate.condition())
            .collect::<std::collections::BTreeSet<_>>();

        Ok(contract.domains.iter().any(|domain| {
            domain
                .properties
                .contains(&bray_checker::ExecutionProperty::Pure)
                && domain.requires_only(&requirements)
        }))
    }

    pub(super) fn trusted_phase_contract(
        &self,
        behavior: &bray_symbols::CallablePhaseBehavior,
        completion: &bray_symbols::CallablePhaseBehavior,
        result: Option<bray_symbols::TypeId>,
        decode: impl Fn(
            &bray_symbols::PredicateSemanticSummary,
        ) -> Result<ExecutionCondition, FactQueryError>,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<TrustedCallContract, FactQueryError> {
        let mut requirements = Vec::new();
        let mut preconditions = Vec::new();
        let mut execution_requirements = Vec::new();

        for predicate in behavior.predicate_requirements() {
            let condition = decode(predicate)?;

            if predicate.domain() == bray_symbols::PredicateDomain::Execution
                && predicate
                    .execution_requirement(
                        self.semantic_value_store()?,
                        self.available_compiler_known_symbols(),
                    )
                    .is_none()
            {
                // Compound lane conditions are not a runtime Boolean or an unconditional lane requirement.
                execution_requirements.push(condition.clone());
            }

            let conditions = if predicate.is_trusted() {
                &mut requirements
            } else {
                &mut preconditions
            };

            conditions.push(condition);
        }

        let mut guarantees = Vec::new();
        let mut postconditions = Vec::new();

        for predicate in completion.predicate_guarantees() {
            let conditions = if predicate.is_trusted() {
                &mut guarantees
            } else {
                &mut postconditions
            };

            conditions.push(decode(predicate)?);
        }

        Ok(TrustedCallContract {
            arguments: Default::default(),
            transferred_inputs: Vec::new(),
            completes: result.is_some(),
            checks_preconditions: behavior.has_runtime_requirements(),
            preserves_inputs: false,
            witness_subjects: self.trusted_predicate_subjects(
                &guarantees,
                result,
                cancellation,
                diagnostics,
            )?,
            requirements,
            execution_requirements,
            preconditions,
            guarantees,
            postconditions,
        })
    }
}
