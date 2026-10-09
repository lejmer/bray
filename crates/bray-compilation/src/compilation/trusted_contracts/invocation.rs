use bray_checker::{ExecutionCondition, TrustedCallContract, TrustedContractInputs};
use bray_diagnostics::DiagnosticBag;

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn trusted_operation_contract(
        &self,
        operation: &bray_bound_tree::SelectedOperation,
        expression: &bray_bound_tree::BoundExpression,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<TrustedCallContract>, FactQueryError> {
        use bray_bound_tree::{
            ConstructionTarget, ConversionTarget, IndexTarget, OperatorTarget, SelectedOperation,
        };

        if let SelectedOperation::Construction(construction) = operation
            && let Some(contract) = self.trusted_storage_formation(construction, cancellation)?
        {
            return Ok(Some(contract));
        }

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
                inputs.get(ordinal).map(|input| (*input, expression.into()))
            })
            .collect();

        self.trusted_invocation_contract(target, &signature, arguments, cancellation, diagnostics)
    }

    pub(super) fn add_scoped_trusted_contracts(
        &self,
        scoped: &bray_bound_tree::SelectedScopedUse,
        inputs: &mut TrustedContractInputs,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        let expression = scoped.expression();

        for (invocation, selected) in [
            (
                bray_bound_tree::BoundExecutionSite::ScopedEnter(expression),
                scoped.enter(),
            ),
            (
                bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                scoped.exit(),
            ),
        ] {
            if let Some(contract) = self.trusted_invocation_contract(
                selected.0,
                &selected.1,
                std::collections::BTreeMap::new(),
                cancellation,
                diagnostics,
            )? {
                inputs.calls.insert(invocation, contract);
            }
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

        if behavior.predicate_requirements().is_empty()
            && completion.predicate_guarantees().is_empty()
        {
            return Ok(None);
        }

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
                && domain
                    .entry
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>()
                    == requirements
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

        for predicate in behavior.predicate_requirements() {
            let conditions = if predicate.is_trusted() {
                &mut requirements
            } else {
                &mut preconditions
            };

            conditions.push(decode(predicate)?);
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

        let result_is_witness = match result {
            Some(ty) => {
                self.trusted_result_is_witness(ty, &guarantees, cancellation, diagnostics)?
            }
            None => true,
        };

        Ok(TrustedCallContract {
            arguments: Default::default(),
            transferred_inputs: Vec::new(),
            completes: result.is_some(),
            preserves_inputs: false,
            result_is_witness,
            witness_subjects: self.trusted_predicate_subjects(
                &guarantees,
                cancellation,
                diagnostics,
            )?,
            requirements,
            preconditions,
            guarantees,
            postconditions,
        })
    }
}
