use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    GenericConstraintObligationKey, GenericConstraintSatisfactionQuery,
    GenericDeclarationTemplateQuery, ProofOutcome, SymbolQueryRequest,
};

use super::super::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns whether every static constraint holds for one generic declaration instance.
    pub fn generic_constraint_satisfaction(
        &self,
        key: GenericConstraintObligationKey,
    ) -> Result<
        Arc<
            DiagnosticResult<
                <GenericConstraintSatisfactionQuery as bray_symbols::SemanticQueryContract>::Value,
            >,
        >,
        FactQueryError,
    > {
        self.generic_constraint_satisfaction_with_cancellation(key, &self.state.cancellation)
    }

    pub(in crate::compilation) fn generic_constraint_satisfaction_with_cancellation(
        &self,
        key: GenericConstraintObligationKey,
        cancellation: &CancellationToken,
    ) -> Result<
        Arc<
            DiagnosticResult<
                <GenericConstraintSatisfactionQuery as bray_symbols::SemanticQueryContract>::Value,
            >,
        >,
        FactQueryError,
    > {
        let cell = self.state.generic_constraint_satisfaction.cell(key)?;

        let result = cell.get_or_compute(
            &self.state.fact_runtime,
            CompilationFactKey::GenericConstraintSatisfaction(key),
            cancellation,
            || {
                self.compute_generic_constraint_satisfaction(key, cancellation)
                    .map(Arc::new)
            },
        )?;

        Ok(Arc::clone(result))
    }

    fn compute_generic_constraint_satisfaction(
        &self,
        key: GenericConstraintObligationKey,
        cancellation: &CancellationToken,
    ) -> Result<
        DiagnosticResult<
            <GenericConstraintSatisfactionQuery as bray_symbols::SemanticQueryContract>::Value,
        >,
        FactQueryError,
    > {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(key.substitution())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if substitution.owner() != key.owner() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let binding_context = self.binding_context(cancellation)?;

        let template = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                key.owner(),
            ))
            .map_err(super::super::binder::binding_query_error)?;

        let mut outcome = ProofOutcome::Proven;
        let mut diagnostics = template.diagnostics().clone();

        for constraint in template.value().constraints() {
            cancellation.check()?;

            let result = self.evaluate_constraint(
                key.owner(),
                constraint,
                key.substitution(),
                cancellation,
            )?;

            diagnostics = diagnostics.merged(result.diagnostics());
            outcome = outcome.and(*result.value());

            if outcome == ProofOutcome::Disproven {
                break;
            }
        }

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

}
