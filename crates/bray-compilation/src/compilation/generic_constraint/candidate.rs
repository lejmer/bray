use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    GenericConstraintObligationKey, GenericConstraintTemplate, ImplementationCandidate,
    ProofOutcome,
};

use super::super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn implementation_candidate_constraint_outcome(
        &self,
        candidate: &ImplementationCandidate,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ProofOutcome, FactQueryError> {
        self.implementation_candidate_constraint_outcome_with_evidence(
            candidate,
            &[],
            cancellation,
            diagnostics,
        )
    }

    pub(in crate::compilation) fn implementation_candidate_constraint_outcome_with_evidence(
        &self,
        candidate: &ImplementationCandidate,
        evidence: &[(bray_symbols::TypeId, bray_symbols::TraitApplicationId)],
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ProofOutcome, FactQueryError> {
        if self.implementation_candidate_constraints_are_established(
            candidate,
            evidence,
            cancellation,
        )? {
            return Ok(ProofOutcome::Proven);
        }

        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(candidate.substitution())
            .map_err(FactQueryError::SemanticValueStore)?;

        let obligation =
            GenericConstraintObligationKey::new(substitution.owner(), candidate.substitution());

        match self.generic_constraint_satisfaction_with_cancellation(obligation, cancellation) {
            Ok(result) => {
                diagnostics.add_range(result.diagnostics().clone());

                Ok(*result.value())
            }
            Err(FactQueryError::Cycle(_)) => Ok(ProofOutcome::Unknown),
            Err(error) => Err(error),
        }
    }

    fn implementation_candidate_constraints_are_established(
        &self,
        candidate: &ImplementationCandidate,
        evidence: &[(bray_symbols::TypeId, bray_symbols::TraitApplicationId)],
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        if candidate.constraints().is_empty() {
            return Ok(true);
        }

        let values = self.semantic_value_store()?;

        for constraint in candidate.constraints() {
            let (subject, application) = match constraint {
                GenericConstraintTemplate::Resolved(constraint) => {
                    let bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                        subject,
                        application,
                    } = constraint.kind()
                    else {
                        return Ok(false);
                    };

                    let subject = values
                        .substitute_type(subject, candidate.substitution())
                        .map_err(FactQueryError::SemanticValueStore)?;

                    let application = values
                        .substitute_trait_application(application, candidate.substitution())
                        .map_err(FactQueryError::SemanticValueStore)?;

                    (subject, application)
                }
                GenericConstraintTemplate::TraitSatisfaction {
                    subject,
                    application,
                    ..
                } => {
                    let resolved = self.resolve_trait_satisfaction_constraint(
                        subject,
                        application,
                        candidate.substitution(),
                        cancellation,
                    )?;

                    let Some(resolved) = resolved.value() else {
                        return Ok(false);
                    };

                    *resolved
                }
                GenericConstraintTemplate::Source { .. }
                | GenericConstraintTemplate::TypeEquality { .. } => return Ok(false),
            };

            if !evidence.contains(&(subject, application)) {
                return Ok(false);
            }
        }

        Ok(true)
    }
}
