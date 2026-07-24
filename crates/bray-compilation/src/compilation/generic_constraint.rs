use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_bound_tree::{
    BoundBlockItem, BoundExpressionId, BoundSourceAnchor, BoundUnit, BoundUnitKey, BoundUnitRoot,
};
use bray_checker::{
    CheckerUnitView, ConstantEvaluationInput, ConstantEvaluator, DefaultConstantEvaluator,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    ConstantValueKind, GenericConstraintObligationKey, GenericConstraintSatisfactionFact,
    GenericConstraintTemplate, GenericDeclarationTemplateFact, GenericSubstitutionId,
    ImplementationCandidate, ProofOutcome, SemanticFactResult, SymbolFactRequest,
};

use super::Compilation;
use super::checker::{CompilationCheckerContext, checker_result};
use super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

impl Compilation {
    /// Returns whether every static constraint holds for one generic declaration instance.
    pub fn generic_constraint_satisfaction(
        &self,
        key: GenericConstraintObligationKey,
    ) -> Result<Arc<SemanticFactResult<GenericConstraintSatisfactionFact>>, FactQueryError> {
        self.generic_constraint_satisfaction_with_cancellation(key, &self.state.cancellation)
    }

    pub(in crate::compilation) fn generic_constraint_satisfaction_with_cancellation(
        &self,
        key: GenericConstraintObligationKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<SemanticFactResult<GenericConstraintSatisfactionFact>>, FactQueryError> {
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
    ) -> Result<SemanticFactResult<GenericConstraintSatisfactionFact>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(key.substitution())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if substitution.owner() != key.owner() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let facts = self.binder_facts(cancellation)?;

        let template = facts
            .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                key.owner(),
            ))
            .map_err(super::binder::binder_fact_error)?;

        let mut outcome = ProofOutcome::Proven;
        let mut diagnostics = template.diagnostics().clone();

        for constraint in template.value().constraints() {
            cancellation.check()?;

            let result = self.evaluate_constraint(*constraint, key.substitution(), cancellation)?;

            diagnostics = diagnostics.merged(result.diagnostics());
            outcome = outcome.and(*result.value());

            if outcome == ProofOutcome::Disproven {
                break;
            }
        }

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

    pub(in crate::compilation) fn implementation_candidate_constraint_outcome(
        &self,
        candidate: &ImplementationCandidate,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<ProofOutcome, FactQueryError> {
        let values = self.semantic_value_store()?;

        let substitution = values
            .generic_substitution_data(candidate.substitution())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

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

    fn evaluate_constraint(
        &self,
        constraint: GenericConstraintTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<ProofOutcome>, FactQueryError> {
        let (Some(unit), Some(expression)) = (constraint.unit_syntax(), constraint.expression())
        else {
            return Ok(DiagnosticResult::without_diagnostics(ProofOutcome::Unknown));
        };

        let key = self.constraint_unit_key(expression.owner(), unit)?;
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let semantics = self.expression_semantics_with_cancellation(key, cancellation)?;

        let diagnostics = DiagnosticBag::merged_all([
            bound.result().diagnostics(),
            semantics.result().diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        }

        let root = constraint_expression(bound.result().value(), expression.syntax())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let references = self.concrete_call_references(
            bound.result().value(),
            &semantics.result().value().1,
            substitution,
            None,
            &BTreeMap::new(),
            cancellation,
        )?;

        let (references, reference_diagnostics) = references;

        let context = CompilationCheckerContext::new(
            self.binder_facts_for(bound.result().value().key(), cancellation)?,
        );

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let request = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(
                    bray_checker::CheckerInfrastructureError::InvalidUnitView(error),
                )
            })?;

        let resolver = super::constant::CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(
            &semantics.result().value().0,
            &semantics.result().value().1,
        )
        .with_root(root)
        .with_references(references)
        .with_call_resolver(&resolver);

        let evaluated =
            checker_result(DefaultConstantEvaluator.evaluate_constant(request, &input))?;

        let diagnostics = DiagnosticBag::merged_all([
            &diagnostics,
            &reference_diagnostics,
            evaluated.diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok(DiagnosticResult::new(ProofOutcome::Recovered, diagnostics));
        }

        let values = self.semantic_value_store()?;

        let value = values
            .constant_value_data(*evaluated.value())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let outcome = match value.kind() {
            ConstantValueKind::Boolean(true) => ProofOutcome::Proven,
            ConstantValueKind::Boolean(false) => ProofOutcome::Disproven,
            ConstantValueKind::Error => ProofOutcome::Recovered,
            _ => ProofOutcome::Unknown,
        };

        Ok(DiagnosticResult::new(outcome, diagnostics))
    }

    fn constraint_unit_key(
        &self,
        owner: bray_symbols::AnySymbolId,
        syntax: bray_declarations::SyntaxAnchor,
    ) -> Result<BoundUnitKey, FactQueryError> {
        let symbols = self.symbol_graph()?;

        let owner = symbols
            .symbol_key(owner)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let source = self
            .source(syntax.source_id())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let source = BoundSourceAnchor::new(syntax, source.version());

        BoundUnitKey::constraint(owner.clone(), source).ok_or(FactQueryError::InfrastructureFailure)
    }
}

fn constraint_expression(
    unit: &BoundUnit,
    syntax: bray_declarations::SyntaxAnchor,
) -> Option<BoundExpressionId> {
    let BoundUnitRoot::ExpressionSequence(root) = unit.root() else {
        return None;
    };

    let block = unit.view().block(root)?;

    block.items().iter().find_map(|item| {
        let BoundBlockItem::Expression(expression) = item else {
            return None;
        };

        unit.view()
            .expression(*expression)
            .filter(|bound| {
                let bound = bound.origin().source_anchor().syntax();

                bound.source_id() == syntax.source_id() && bound.full_range() == syntax.full_range()
            })
            .map(|_| *expression)
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{GenericConstraintObligationKey, GenericOwnerId};

    use crate::test_support::{compilation, diagnostic_kinds};

    #[test]
    fn concrete_generic_constraints_participate_in_callable_selection() {
        let accepted = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        assert!(
            accepted.check_diagnostics().is_empty(),
            "{:?}",
            accepted.check_diagnostics()
        );

        let rejected = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(false)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        assert!(
            diagnostic_kinds(rejected.check_diagnostics())
                .contains(&DiagnosticKind::CheckingNoApplicableCandidate)
        );
    }

    #[test]
    fn generic_constraint_facts_are_cached_by_exact_substitution() {
        let compilation = compilation(concat!(
            "module app;\n",
            "\n",
            "func constrained() with(true)\n",
            "{\n",
            "}\n",
            "\n",
            "func main()\n",
            "{\n",
            "    constrained();\n",
            "}\n",
        ));

        let graph = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must be available: {error:?}"));

        let function = graph
            .functions()
            .iter()
            .find(|function| {
                graph
                    .member_name(function.id().into())
                    .is_some_and(|name| name.as_str() == "constrained")
            })
            .unwrap_or_else(|| panic!("constrained function must be declared"));

        let symbol = function.id().into();

        let owner = GenericOwnerId::try_new(symbol)
            .unwrap_or_else(|| panic!("function must be a generic owner"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must be available: {error:?}"));

        let substitution = super::super::substitution::empty_substitution(values, symbol)
            .unwrap_or_else(|error| panic!("empty substitution must be available: {error:?}"));

        let obligation = GenericConstraintObligationKey::new(owner, substitution);

        let first = compilation
            .generic_constraint_satisfaction(obligation)
            .unwrap_or_else(|error| panic!("constraint fact must be available: {error:?}"));

        let second = compilation
            .generic_constraint_satisfaction(obligation)
            .unwrap_or_else(|error| panic!("constraint fact must be reusable: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));
    }
}
