use std::sync::Arc;

use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    ImplementationInstanceData, ImplementationRequirementKey, ImplementationSelection,
    ImplementationSelectionCandidate, ImplementationSelectionFact, ProofOutcome,
    SemanticFactResult,
};

use super::super::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns the selected implementation witness for one exact requirement.
    pub fn implementation_selection_result(
        &self,
        key: ImplementationRequirementKey,
    ) -> Result<Arc<SemanticFactResult<ImplementationSelectionFact>>, FactQueryError> {
        self.implementation_selection_result_with_cancellation(key, &self.state.cancellation)
    }

    pub(in crate::compilation) fn implementation_selection_result_with_cancellation(
        &self,
        key: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<SemanticFactResult<ImplementationSelectionFact>>, FactQueryError> {
        let cell = self.state.implementation_selections.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::ImplementationSelection(key),
            cancellation,
            || {
                self.compute_implementation_selection(key, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_implementation_selection(
        &self,
        key: ImplementationRequirementKey,
        cancellation: &CancellationToken,
    ) -> Result<SemanticFactResult<ImplementationSelectionFact>, FactQueryError> {
        let candidates =
            self.implementation_candidate_set_result_with_cancellation(key, cancellation)?;

        let values = self.semantic_value_store()?;

        let mut applicable = Vec::new();
        let mut has_unknown = false;
        let mut diagnostics = candidates.diagnostics().clone();

        for candidate in candidates.value().candidates() {
            cancellation.check()?;

            let outcome = self.implementation_candidate_constraint_outcome(
                candidate,
                cancellation,
                &mut diagnostics,
            )?;

            match outcome {
                ProofOutcome::Proven => {}
                ProofOutcome::Unknown => {
                    has_unknown = true;

                    continue;
                }
                ProofOutcome::Disproven | ProofOutcome::Recovered => continue,
            }

            let instance = values
                .intern_implementation_instance(ImplementationInstanceData::new(
                    candidate.implementation(),
                    candidate.substitution(),
                ))
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            applicable.push(ImplementationSelectionCandidate::new(
                candidate.key().clone(),
                instance,
            ));
        }

        let selection = match (has_unknown, applicable.as_slice()) {
            (true, _) => ImplementationSelection::Deferred,
            (false, []) => ImplementationSelection::Unavailable,
            (false, [candidate]) => ImplementationSelection::Selected(candidate.instance()),
            (false, _) => ImplementationSelection::ambiguous(applicable)
                .map_err(|_| FactQueryError::InfrastructureFailure)?,
        };

        Ok(DiagnosticResult::new(selection, diagnostics))
    }
}
