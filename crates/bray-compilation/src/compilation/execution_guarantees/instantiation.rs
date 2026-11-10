use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_checker::{ExecutionCallEvidence, ExecutionCondition, TypeNormalizer};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableInstanceData, CheckedConstraintKind, GenericConstraintsQuery, GenericOwnerId,
    ImplementationInstanceId, ImplementationRequirementKey, ImplementationSelection,
    SymbolQueryRequest,
};

use crate::compilation::{Compilation, binder::binding_query_error};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn instantiate_execution_condition_calls(
        &self,
        condition: &ExecutionCondition,
        available: &[ImplementationInstanceId],
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ExecutionCondition, FactQueryError> {
        bray_checker::map_execution_condition_references(
            condition,
            &Ok,
            &|place| Ok(ExecutionCondition::Input(place.clone())),
            &|value| Ok(Arc::clone(value)),
            &mut |callable, dispatch, witnesses, operands, reference_type| {
                let values = self.semantic_value_store()?;

                let captured = witnesses
                    .iter()
                    .copied()
                    .map(|witness| values.intern_implementation_instance(witness))
                    .collect::<Result<Vec<_>, _>>()?;

                let available = captured
                    .iter()
                    .copied()
                    .chain(available.iter().copied())
                    .collect::<Vec<_>>();

                let context = self
                    .checker_context(cancellation)?
                    .with_implementation_witnesses(available.iter().copied());

                let mut normalizer = TypeNormalizer::new(&context, None);

                let callable = CallableInstanceData::new(
                    callable.definition(),
                    normalizer.normalize_substitution(callable.substitution())?,
                );

                diagnostics.add_range(normalizer.take_diagnostics().iter().cloned());

                let mut witnesses = captured.into_iter().collect::<BTreeSet<_>>();

                self.execution_condition_constraint_witnesses(
                    callable,
                    &available,
                    &mut witnesses,
                    cancellation,
                    diagnostics,
                )?;

                Ok(ExecutionCondition::Call(
                    callable,
                    dispatch,
                    witnesses
                        .into_iter()
                        .map(|witness| *values.implementation_instance_data(witness))
                        .collect::<Vec<_>>()
                        .into(),
                    operands.into(),
                    reference_type.map(|ty| normalizer.normalize_type(ty)).transpose()?,
                ))
            },
        )
    }

    fn execution_condition_constraint_witnesses(
        &self,
        callable: CallableInstanceData,
        available: &[ImplementationInstanceId],
        witnesses: &mut BTreeSet<ImplementationInstanceId>,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        let Some(owner) = GenericOwnerId::try_new(callable.definition().symbol()) else {
            return Ok(());
        };

        let binding = self.binding_context(cancellation)?;
        let values = self.semantic_value_store()?;

        let constraints = binding
            .resolve_symbol_query(SymbolQueryRequest::<GenericConstraintsQuery>::new(owner))
            .map_err(binding_query_error)?;

        diagnostics.add_range(constraints.diagnostics().iter().cloned());

        let context = self
            .checker_context(cancellation)?
            .with_implementation_witnesses(available.iter().copied());

        let mut normalizer = TypeNormalizer::new(&context, Some(callable.substitution()));

        for constraint in constraints.value().constraints() {
            let CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } = constraint.kind()
            else {
                continue;
            };

            let requirement = ImplementationRequirementKey::new(
                normalizer
                    .normalize_type(values.substitute_type(subject, callable.substitution())?)?,
                normalizer.normalize_trait_application(
                    values.substitute_trait_application(application, callable.substitution())?,
                )?,
            );

            if bray_checker::built_in_trait_constraint_outcome(
                &context,
                requirement.subject(),
                requirement.trait_application(),
            ) == Some(bray_symbols::ProofOutcome::Proven)
            {
                continue;
            }

            let saved = crate::compilation::implementation::matching_implementation_witness(
                &binding,
                requirement,
                available,
            )?;

            if let Some(witness) = saved {
                witnesses.insert(witness);
                continue;
            }

            let selected =
                self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

            diagnostics.add_range(selected.diagnostics().iter().cloned());

            if let ImplementationSelection::Selected(witness) = selected.value() {
                witnesses.insert(*witness);
            }
        }

        diagnostics.add_range(normalizer.take_diagnostics().iter().cloned());

        Ok(())
    }

    pub(super) fn instantiate_execution_evidence(
        &self,
        evidence: &ExecutionCallEvidence,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ExecutionCallEvidence, FactQueryError> {
        evidence.try_map_conditions(|condition| {
            self.instantiate_execution_condition_calls(
                condition,
                evidence.implementation_witnesses(),
                cancellation,
                diagnostics,
            )
        })
    }
}
