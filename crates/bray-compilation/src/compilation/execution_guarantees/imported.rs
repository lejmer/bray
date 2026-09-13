use std::collections::{BTreeMap, BTreeSet};

use bray_binder::SymbolQueryProvider;
use bray_checker::{
    ExecutionCallEvidence, ExecutionClauseId, ExecutionCondition, ExecutionObligation,
    ExecutionProperty, execution_condition_from_term,
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
    Source(SyntaxAnchor),
    Imported(CallableInstanceData),
}

pub(super) type ExecutionProofGraph = BTreeMap<
    (ExecutionProofOwner, ExecutionObligation),
    BTreeSet<(ExecutionProofOwner, ExecutionObligation)>,
>;

impl Compilation {
    pub(super) fn imported_execution_contract(
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
        if self.synthetic_heap_projection_obligation(callable, required) {
            return Ok(Some(required));
        }

        let contract = self.imported_execution_contract(callable, diagnostics, cancellation)?;

        match required {
            ExecutionObligation::Property(property, _) => {
                for domain in &*contract.domains {
                    if !domain.properties.contains(&property) {
                        continue;
                    }

                    let entry = self.imported_execution_conditions(
                        callable,
                        domain.entry.iter().copied(),
                        cancellation,
                    )?;

                    if evidence
                        .unwrap_or(&ExecutionCallEvidence::default())
                        .proves(&entry)
                    {
                        return Ok(Some(ExecutionObligation::Property(
                            property,
                            Some(ExecutionClauseId::Imported(domain.ordinal)),
                        )));
                    }
                }

                Ok(None)
            }
            ExecutionObligation::Postcondition(ExecutionClauseId::Imported(_)) => {
                Ok(Some(required))
            }
            ExecutionObligation::Postcondition(ExecutionClauseId::Source(_)) => Ok(None),
        }
    }

    pub(super) fn append_imported_execution_proof(
        &self,
        callable: CallableInstanceData,
        obligation: ExecutionObligation,
        graph: &mut ExecutionProofGraph,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let mut pending = vec![(callable, obligation)];
        let mut visited = BTreeSet::new();
        let values = self.semantic_value_store()?;

        while let Some((callable, obligation)) = pending.pop() {
            cancellation.check()?;

            if !visited.insert((callable, obligation)) {
                continue;
            }

            if visited.len()
                > usize::try_from(bray_checker::ConstantEvaluationLimits::default().call_depth())
                    .unwrap_or(1024)
            {
                return Ok(false);
            }

            if self.synthetic_heap_projection_obligation(callable, obligation) {
                graph.insert(
                    (ExecutionProofOwner::Imported(callable), obligation),
                    BTreeSet::new(),
                );

                continue;
            }

            let Some(portable) = portable_imported_obligation(obligation) else {
                return Ok(false);
            };

            let contract = self.imported_execution_contract(callable, diagnostics, cancellation)?;

            let Some(proof) = contract
                .evidence
                .iter()
                .find(|proof| proof.obligation == portable)
            else {
                return Ok(false);
            };

            if proof.origin == CallableExecutionOrigin::Requirement {
                return Ok(false);
            }

            if proof.origin == CallableExecutionOrigin::CompilerIntrinsic
                && (!matches!(
                    obligation,
                    ExecutionObligation::Property(
                        ExecutionProperty::Pure | ExecutionProperty::Total,
                        _
                    )
                ) || !self
                    .intrinsic_projection_symbol(callable.definition().symbol(), cancellation)?)
            {
                return Ok(false);
            }

            let mut dependencies = BTreeSet::new();

            for (target, required) in &*proof.dependencies {
                match target {
                    CallableExecutionTarget::Callable(target) => {
                        let target = *values
                            .callable_instance_data(*target)
                            .map_err(FactQueryError::SemanticValueStore)?;

                        let required = imported_obligation(*required);
                        dependencies.insert((ExecutionProofOwner::Imported(target), required));
                        pending.push((target, required));
                    }
                    CallableExecutionTarget::Indirect(ty) => {
                        let ty = values
                            .type_data(*ty)
                            .map_err(FactQueryError::SemanticValueStore)?;

                        if *required
                            != CallableExecutionObligation::Property(ExecutionProperty::Pure, None)
                            || !matches!(ty.as_ref(), bray_symbols::TypeData::Callable(callable) if callable.phase_behaviors().invocation().execution_properties().contains(&ExecutionProperty::Pure))
                        {
                            return Ok(false);
                        }
                    }
                }
            }

            graph.insert(
                (ExecutionProofOwner::Imported(callable), obligation),
                dependencies,
            );
        }

        Ok(!diagnostics.has_errors())
    }
}

pub(super) fn imported_obligation(
    obligation: CallableExecutionObligation<SymbolOrdinal>,
) -> ExecutionObligation {
    match obligation {
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
