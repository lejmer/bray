use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_checker::{
    ExecutionCallEvidence, ExecutionClauseId, ExecutionCondition, ExecutionObligation,
    execution_condition_from_term,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableContractsQuery, CallableExecutionObligation, CallableExecutionOrigin,
    CallableExecutionTarget, CallableInstanceData, ResolvedCallableExecutionContract,
    SymbolOrdinal, SymbolQueryRequest,
};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

/// Proof identity retains source ownership or an exact separately compiled instantiation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum ExecutionProofOwner {
    Source(SyntaxAnchor, Option<CallableInstanceData>),
    Premise(
        bray_symbols::CallableInstanceId,
        bray_symbols::ImplementationRequirementKey,
    ),
    Indirect(bray_symbols::TypeId),
    Imported(CallableInstanceData),
}

pub(super) type ExecutionProofGraph = BTreeMap<
    (ExecutionProofOwner, ExecutionObligation),
    BTreeSet<(ExecutionProofOwner, ExecutionObligation)>,
>;

impl Compilation {
    pub(super) fn imported_default_proves_execution(
        &self,
        provider: bray_symbols::DefaultValueProvider,
        required: ExecutionObligation,
        predicate: bool,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let imported = self.checked_default_template(provider, cancellation)?;

        diagnostics.add_range(imported.diagnostics().iter().cloned());

        Ok(!imported.diagnostics().has_errors()
            && imported.value().as_ref().is_some_and(|template| {
                (!predicate || template.behavior().execution().constant_eligible())
                    && required.property().is_some_and(|property| {
                        template
                            .behavior()
                            .execution()
                            .properties()
                            .contains(&property)
                    })
            }))
    }

    pub(in crate::compilation) fn imported_execution_contract(
        &self,
        callable: CallableInstanceData,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<ResolvedCallableExecutionContract, FactQueryError> {
        let context = self.binding_context(cancellation)?;

        let contract = context
            .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(crate::compilation::binder::binding_query_error)?;

        diagnostics.add_range(contract.diagnostics().iter().cloned());

        let values = self.semantic_value_store()?;

        contract
            .value()
            .execution_contract()
            .try_map(
                &mut (),
                |_, term| values.substitute_constant_term(term, callable.substitution()),
                |_, target| match target {
                    CallableExecutionTarget::Callable(target) => values
                        .substitute_callable_instance(target, callable.substitution())
                        .map(CallableExecutionTarget::Callable),
                    CallableExecutionTarget::Requirement {
                        callable: target,
                        requirement,
                    } => Ok(CallableExecutionTarget::Requirement {
                        callable: values
                            .substitute_callable_instance(target, callable.substitution())?,
                        requirement: bray_symbols::ImplementationRequirementKey::new(
                            values
                                .substitute_type(requirement.subject(), callable.substitution())?,
                            values.substitute_trait_application(
                                requirement.trait_application(),
                                callable.substitution(),
                            )?,
                        ),
                    }),
                    CallableExecutionTarget::Indirect(ty) => values
                        .substitute_type(ty, callable.substitution())
                        .map(CallableExecutionTarget::Indirect),
                },
            )
            .map_err(FactQueryError::SemanticValueStore)
    }

    pub(super) fn imported_execution_conditions(
        &self,
        callable: CallableInstanceData,
        terms: impl IntoIterator<Item = bray_symbols::ConstantTermId>,
        cancellation: &CancellationToken,
    ) -> Result<Vec<ExecutionCondition>, FactQueryError> {
        let inputs =
            self.execution_callable_inputs(callable.definition().symbol(), cancellation)?;

        let values = self.semantic_value_store()?;

        terms
            .into_iter()
            .map(|term| {
                execution_condition_from_term(values, term, &inputs)
                    .map_err(FactQueryError::SemanticValueStore)
            })
            .collect()
    }

    pub(super) fn select_imported_execution_obligation(
        &self,
        callable: CallableInstanceData,
        required: ExecutionObligation,
        evidence: Option<&ExecutionCallEvidence>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<Option<ExecutionObligation>, FactQueryError> {
        if self.intrinsic_projection_obligation(callable, required, cancellation)? {
            return Ok(Some(required));
        }

        let contract = self.imported_execution_contract(callable, diagnostics, cancellation)?;

        let inputs = self.execution_callable_inputs(callable.definition().symbol(), cancellation)?;

        match required {
            ExecutionObligation::FiniteIteration => Ok(contract
                .evidence
                .iter()
                .any(|proof| {
                    Some(proof.obligation) == portable_imported_obligation(required)
                        && proof.origin == CallableExecutionOrigin::CheckedBody
                })
                .then_some(required)),
            ExecutionObligation::Observation
            | ExecutionObligation::Termination
            | ExecutionObligation::Property(_, _) => {
                // A checked total entry domain also proves termination. Finite cursor
                // progress remains a separate obligation with its own evidence.
                let (property, selected) = match required {
                    ExecutionObligation::Observation => {
                        (bray_checker::ExecutionProperty::Pure, None)
                    }
                    ExecutionObligation::Termination => {
                        (bray_checker::ExecutionProperty::Total, None)
                    }
                    ExecutionObligation::Property(property, selected) => (property, selected),
                    _ => unreachable!("property selection retains its requested property"),
                };

                for domain in &*contract.domains {
                    if !domain.properties.contains(&property)
                        || selected.is_some_and(|selected| {
                            selected != ExecutionClauseId::Imported(domain.ordinal)
                        })
                    {
                        continue;
                    }

                    let entry = self.imported_execution_conditions(
                        callable,
                        domain.entry.iter().copied(),
                        cancellation,
                    )?;

                    if evidence
                        .unwrap_or(&ExecutionCallEvidence::default())
                        .proves_trusted_with_inputs(&entry, &inputs)
                    {
                        return Ok(Some(ExecutionObligation::Property(
                            property,
                            Some(ExecutionClauseId::Imported(domain.ordinal)),
                        )));
                    }
                }

                Ok((matches!(
                    required,
                    ExecutionObligation::Observation | ExecutionObligation::Termination
                ) && contract.evidence.iter().any(|proof| {
                    Some(proof.obligation) == portable_imported_obligation(required)
                        && proof.origin == CallableExecutionOrigin::CheckedBody
                }))
                .then_some(required))
            }
            ExecutionObligation::Postcondition(ExecutionClauseId::Imported(_)) => {
                Ok(Some(required))
            }
            ExecutionObligation::Postcondition(ExecutionClauseId::Source(_)) => Ok(None),
        }
    }
}

pub(super) fn imported_obligation(
    obligation: CallableExecutionObligation<SymbolOrdinal>,
) -> ExecutionObligation {
    match obligation {
        CallableExecutionObligation::Observation => ExecutionObligation::Observation,
        CallableExecutionObligation::Termination => ExecutionObligation::Termination,
        CallableExecutionObligation::FiniteIteration => ExecutionObligation::FiniteIteration,
        CallableExecutionObligation::Property(property, guard) => {
            ExecutionObligation::Property(property, guard.map(ExecutionClauseId::Imported))
        }
        CallableExecutionObligation::Postcondition(post) => {
            ExecutionObligation::Postcondition(ExecutionClauseId::Imported(post))
        }
    }
}

pub(super) fn portable_imported_obligation(
    obligation: ExecutionObligation,
) -> Option<CallableExecutionObligation<SymbolOrdinal>> {
    match obligation {
        ExecutionObligation::Observation => Some(CallableExecutionObligation::Observation),
        ExecutionObligation::Termination => Some(CallableExecutionObligation::Termination),
        ExecutionObligation::FiniteIteration => Some(CallableExecutionObligation::FiniteIteration),
        ExecutionObligation::Property(property, Some(ExecutionClauseId::Imported(guard))) => {
            Some(CallableExecutionObligation::Property(property, Some(guard)))
        }
        ExecutionObligation::Property(property, None) => {
            Some(CallableExecutionObligation::Property(property, None))
        }
        ExecutionObligation::Postcondition(ExecutionClauseId::Imported(post)) => {
            Some(CallableExecutionObligation::Postcondition(post))
        }
        _ => None,
    }
}
