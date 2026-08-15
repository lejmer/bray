use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
use bray_checker::{
    CheckedConstantTerms, ConstantEvaluationInput, ConstantEvaluator, DefaultConstantEvaluator,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    ConstantExpressionExpectedType, ConstantExpressionOccurrence, ConstantExpressionOccurrenceKey,
    ConstantTermId, ConstantValueId, TypeExpressionTemplate,
};

use super::super::Compilation;
use super::super::checker::checker_result;
use super::super::unit::semantic_unit_context_for;
use super::CompilationConstantCallResolver;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(in crate::compilation) fn embedded_constant_key(
        &self,
        occurrence: ConstantExpressionOccurrence,
    ) -> Result<BoundUnitKey, FactQueryError> {
        let mut expectations = self
            .state
            .embedded_constant_expectations
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match expectations.entry(occurrence.key()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(occurrence.expected_type());
            }
            std::collections::btree_map::Entry::Occupied(entry)
                if *entry.get() != occurrence.expected_type() =>
            {
                return Err(FactQueryError::InfrastructureFailure);
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }

        drop(expectations);

        let owner = self
            .symbol_graph()?
            .symbol_key(occurrence.key().owner())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let syntax = occurrence.key().syntax();

        let source = self
            .source(syntax.source_id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        // Stable symbol keys are Arc-backed identities retained by the bound unit key.
        BoundUnitKey::embedded_constant(
            owner.clone(),
            BoundSourceAnchor::new(syntax, source.version()),
        )
        .ok_or(FactQueryError::InfrastructureFailure)
    }

    pub(in crate::compilation) fn embedded_constant_expected_type(
        &self,
        occurrence: ConstantExpressionOccurrenceKey,
    ) -> Result<ConstantExpressionExpectedType, FactQueryError> {
        self.state
            .embedded_constant_expectations
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?
            .get(&occurrence)
            .copied()
            .ok_or(FactQueryError::InfrastructureFailure)
    }

    /// Returns the checked symbolic term for one source constant expression embedded in a type.
    pub fn embedded_constant_term(
        &self,
        occurrence: ConstantExpressionOccurrence,
    ) -> Result<Arc<DiagnosticResult<ConstantTermId>>, FactQueryError> {
        self.embedded_constant_term_with_cancellation(occurrence, &self.state.cancellation)
    }

    pub(in crate::compilation) fn embedded_constant_term_with_cancellation(
        &self,
        occurrence: ConstantExpressionOccurrence,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<ConstantTermId>>, FactQueryError> {
        let key = self.embedded_constant_key(occurrence)?;
        let published = self.symbolic_constant_term_with_cancellation(key, cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    pub(in crate::compilation) fn embedded_constant_value_with_cancellation(
        &self,
        occurrence: ConstantExpressionOccurrence,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ConstantValueId>, FactQueryError> {
        let key = self.embedded_constant_key(occurrence)?;

        // Independently cached results retain the same Arc-backed unit identity.
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let context = self.checker_context_for(&key, cancellation)?;

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let (references, dependency_diagnostics) = self.concrete_embedded_references(
            bound.result().value(),
            &semantics.result().value().1,
            bray_checker::ConstantEvaluationLimits::default(),
            cancellation,
        )?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(
            &semantics.result().value().0,
            &semantics.result().value().1,
        )
        .with_references(references)
        .with_call_resolver(&resolver);

        let unit =
            bray_checker::CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                .map_err(|error| {
                    FactQueryError::CheckerInfrastructure(
                        bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                    )
                })?;

        let evaluated = checker_result(DefaultConstantEvaluator.evaluate_constant(unit, &input))?;

        let diagnostics = DiagnosticBag::merged_all([
            semantics.result().diagnostics(),
            &dependency_diagnostics,
            evaluated.diagnostics(),
        ]);

        Ok(DiagnosticResult::new(*evaluated.value(), diagnostics))
    }

    /// Returns checked symbolic terms for all constants embedded in one type template.
    pub fn checked_constant_terms(
        &self,
        template: &TypeExpressionTemplate,
    ) -> Result<DiagnosticResult<CheckedConstantTerms>, FactQueryError> {
        self.checked_constant_terms_for_templates_with_cancellation(
            [template],
            &self.state.cancellation,
        )
    }

    pub(in crate::compilation) fn checked_constant_terms_for_templates_with_cancellation<
        'template,
    >(
        &self,
        templates: impl IntoIterator<Item = &'template TypeExpressionTemplate>,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<CheckedConstantTerms>, FactQueryError> {
        let mut terms = BTreeMap::new();
        let mut diagnostics = DiagnosticBag::new();

        for template in templates {
            for occurrence in template.constant_expressions() {
                let result =
                    self.embedded_constant_term_with_cancellation(occurrence, cancellation)?;

                diagnostics.extend(result.diagnostics().iter().cloned());
                terms.insert(occurrence.key(), *result.value());
            }
        }

        let checked = CheckedConstantTerms::try_from_terms(terms)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(DiagnosticResult::new(checked, diagnostics))
    }
}
