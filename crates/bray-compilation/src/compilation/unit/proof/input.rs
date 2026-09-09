use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, BoundUnitKey, CallableProofObligation, CallableProofResult};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{CallableConditions, CallableSymbolId, ResolvedCallableEvidenceTarget};

use crate::compilation::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Copy)]
pub(super) enum ProofDependency {
    Selected(ResolvedCallableEvidenceTarget, CallableProofObligation),
    Unverified(AnyBoundNodeId),
}

pub(super) type ProofInputs = BTreeMap<CallableProofObligation, Vec<ProofDependency>>;

impl Compilation {
    pub(super) fn source_proof_inputs(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofInputs>, FactQueryError> {
        let semantics = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
        let expressions = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let diagnostics = semantics
            .result()
            .diagnostics()
            .merged(expressions.result().diagnostics());

        let mut inputs = BTreeMap::new();

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(inputs, diagnostics));
        }

        let selections = expressions.result().value().selections();

        for checked in semantics.result().value().execution_proofs() {
            let CallableProofResult::Candidate(candidate) = checked else {
                continue;
            };

            let dependencies = inputs
                .entry(candidate.obligation())
                .or_insert_with(Vec::new);

            for dependency in candidate.dependencies() {
                if dependency.uses_indirect_contract(candidate.obligation(), selections) {
                    continue;
                }

                dependencies.push(
                    match self.source_callable_evidence_target(
                        dependency.target(),
                        selections,
                        cancellation,
                    )? {
                        Some(target) => ProofDependency::Selected(target, dependency.obligation()),
                        None => ProofDependency::Unverified(dependency.target().site()),
                    },
                );
            }
        }

        Ok(DiagnosticResult::new(inputs, diagnostics))
    }

    pub(super) fn declared_proof_inputs(
        &self,
        callable: CallableSymbolId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofInputs>, FactQueryError> {
        let binding = self.binding_context(cancellation)?;

        let context = crate::compilation::checker::CompilationCheckerContext::new(
            self.binding_context(cancellation)?,
        );

        let provided = bray_checker::compiler_projection_guarantees(&context, callable)?;

        let mut inputs = provided
            .into_iter()
            .map(|guarantee| (CallableProofObligation::Execution(guarantee), Vec::new()))
            .collect::<BTreeMap<_, _>>();

        let Some(address) = binding
            .imported_semantic_address(callable.into_any())
            .map_err(crate::compilation::binder::binding_query_error)?
        else {
            let assertions = self.foreign_callable_assertions(callable, cancellation)?;

            if !assertions.diagnostics().has_errors() {
                inputs.extend(
                    assertions
                        .value()
                        .iter()
                        .map(|obligation| ((*obligation).into(), Vec::new())),
                );
            }

            return Ok(DiagnosticResult::new(
                inputs,
                assertions.diagnostics().clone(),
            ));
        };

        let records = self.imported_semantics_with_cancellation(
            crate::fact::ImportedSemanticRecordKey::new(
                address.interface(),
                address.symbol(),
                bray_package_interface::InterfaceSemanticRecordKind::CallableContracts,
            ),
            cancellation,
        )?;

        if records.diagnostics().has_errors() {
            return Ok(DiagnosticResult::new(inputs, records.diagnostics().clone()));
        }

        let [bray_package_interface::ImportedSemanticRecord::CallableContracts(contract)] =
            records.value().as_ref()
        else {
            return Ok(DiagnosticResult::new(inputs, records.diagnostics().clone()));
        };

        if contract.evidence().first().is_some_and(|evidence| {
            evidence.origin() == bray_symbols::CallableEvidenceOrigin::ForeignAssertion
        }) {
            let boundary =
                self.imported_native_boundary_with_cancellation(callable.into_any(), cancellation)?;

            if !boundary.is_some_and(|boundary| {
                boundary.direction() == bray_symbols::ForeignCallableDirection::Import
                    && matches!(
                        boundary.kind(),
                        bray_package_interface::InterfaceNativeBoundaryKind::Callable
                    )
            }) {
                return Ok(DiagnosticResult::new(inputs, records.diagnostics().clone()));
            }
        }

        for evidence in contract.evidence() {
            inputs.insert(
                evidence.obligation().into(),
                evidence
                    .dependencies()
                    .iter()
                    .map(|(target, obligation)| {
                        ProofDependency::Selected(*target, (*obligation).into())
                    })
                    .collect(),
            );
        }

        Ok(DiagnosticResult::new(inputs, records.diagnostics().clone()))
    }

    pub(super) fn fulfillment_proof_obligations(
        &self,
        callable: bray_symbols::CallableInstanceData,
        required: CallableProofObligation,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CallableProofObligation>, FactQueryError> {
        let provided = self.execution_callable_conditions(callable, cancellation)?;

        if provided.diagnostics().has_errors() {
            return Ok(Vec::new());
        }

        // Conformance checks implication across guards and predicate fulfillments. Certify each
        // provided promise used by that surface rather than equating unrelated clause ordinals.
        Ok(match required {
            CallableProofObligation::Execution(required) => provided
                .value()
                .execution_guarantees()
                .iter()
                .filter(|provided| provided.property() == required.property())
                .map(|provided| CallableProofObligation::Execution(*provided))
                .collect(),
            CallableProofObligation::Postcondition(_) => provided
                .value()
                .normal_completion_postconditions()
                .iter()
                .chain(provided.value().guarded_postconditions())
                .map(|clause| CallableProofObligation::Postcondition(clause.ordinal()))
                .collect(),
            _ => Vec::new(),
        })
    }
}
