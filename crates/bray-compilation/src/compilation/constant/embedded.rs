use std::collections::BTreeMap;
use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
use bray_checker::{
    CheckedConstantTerms, ConstantEvaluationInput, ConstantEvaluator, DefaultConstantEvaluator,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_source::SourceSpan;
use bray_symbols::{
    ConstantExpressionExpectedType, ConstantExpressionOccurrence, ConstantExpressionOccurrenceKey,
    ConstantTermId, ConstantValueId, TypeExpressionTemplate,
};

use super::super::Compilation;
use super::super::checker::checker_result;

use super::CompilationConstantCallResolver;
use crate::compilation::{
    SemanticDataKind, SemanticQueryContext, SemanticQueryFailure, SemanticQueryViolation,
};
use crate::fact::{
    CancellationToken, FactQueryError, FactRuntimeFailure, SynchronizationComponent,
};

impl Compilation {
    pub(in crate::compilation) fn embedded_constant_key(
        &self,
        occurrence: ConstantExpressionOccurrence,
    ) -> Result<BoundUnitKey, FactQueryError> {
        let mut expectations = self
            .state
            .embedded_constant_expectations
            .lock()
            .map_err(|_| embedded_constant_expectations_poisoned())?;

        match expectations.entry(occurrence.key()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(occurrence.expected_type());
            }
            std::collections::btree_map::Entry::Occupied(entry)
                if *entry.get() != occurrence.expected_type() =>
            {
                let syntax = occurrence.key().syntax();

                return Err(SemanticQueryFailure::located_contract(
                    SemanticQueryContext::Symbol(occurrence.key().owner()),
                    SemanticQueryViolation::ConstantExpectationMismatch {
                        expected: *entry.get(),
                        actual: occurrence.expected_type(),
                    },
                    SourceSpan::new(syntax.source_id(), syntax.full_range()),
                )
                .into());
            }
            std::collections::btree_map::Entry::Occupied(_) => {}
        }

        drop(expectations);

        let owner = self
            .symbol_graph()?
            .symbol_key(occurrence.key().owner())
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(occurrence.key().owner()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let syntax = occurrence.key().syntax();

        let source = self.source(syntax.source_id()).ok_or_else(|| {
            SemanticQueryFailure::located_contract(
                SemanticQueryContext::Symbol(occurrence.key().owner()),
                SemanticQueryViolation::Missing(SemanticDataKind::SourceAnchor),
                SourceSpan::new(syntax.source_id(), syntax.full_range()),
            )
        })?;

        // Stable symbol keys are Arc-backed identities retained by the bound unit key.
        BoundUnitKey::embedded_constant(
            owner.clone(),
            BoundSourceAnchor::new(syntax, source.version()),
        )
        .ok_or_else(|| {
            SemanticQueryFailure::located_contract(
                SemanticQueryContext::Symbol(occurrence.key().owner()),
                SemanticQueryViolation::Missing(SemanticDataKind::BoundUnit),
                SourceSpan::new(syntax.source_id(), syntax.full_range()),
            )
            .into()
        })
    }

    pub(in crate::compilation) fn embedded_constant_expected_type(
        &self,
        occurrence: ConstantExpressionOccurrenceKey,
    ) -> Result<ConstantExpressionExpectedType, FactQueryError> {
        self.state
            .embedded_constant_expectations
            .lock()
            .map_err(|_| embedded_constant_expectations_poisoned())?
            .get(&occurrence)
            .copied()
            .ok_or_else(|| {
                SemanticQueryFailure::located_contract(
                    SemanticQueryContext::Symbol(occurrence.owner()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Type),
                    SourceSpan::new(
                        occurrence.syntax().source_id(),
                        occurrence.syntax().full_range(),
                    ),
                )
                .into()
            })
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

        let semantic_context = semantic_unit_context(context.symbols(), bound.result().value());

        let (references, dependency_diagnostics) = self.concrete_embedded_references(
            bound.result().value(),
            semantics.result().value().selections(),
            bray_checker::ConstantEvaluationLimits::default(),
            cancellation,
        )?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(
            semantics.result().value().types(),
            semantics.result().value().selections(),
        )
        .with_references(references)
        .with_call_resolver(&resolver);

        let unit =
            bray_checker::CheckerUnitView::new(bound.result().value(), &semantic_context, &context);

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
            .map_err(|cause| SemanticQueryFailure::CheckedConstantTerms { unit: None, cause })?;

        Ok(DiagnosticResult::new(checked, diagnostics))
    }
}

fn embedded_constant_expectations_poisoned() -> FactRuntimeFailure {
    FactRuntimeFailure::SynchronizationPoisoned {
        component: SynchronizationComponent::EmbeddedConstantExpectations,
        fact: None,
        task: None,
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use bray_declarations::SyntaxAnchor;
    use bray_parser::parse_source_unit;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
    use bray_symbols::{
        AnySymbolId, ConstantExpressionExpectedType, ConstantExpressionOccurrence,
        ConstantExpressionOccurrenceKey, ConstantSymbolId, GenericConstParameterSymbolId, SymbolId,
    };
    use bray_syntax::LiteralExpressionSyntax;

    use super::Compilation;
    use crate::fact::{
        CancellationToken, FactQueryError, FactRuntimeFailure, SynchronizationComponent,
    };
    use crate::request::CompilationRequest;
    use crate::test_support::package_identity;

    #[test]
    fn poisoned_embedded_constant_expectation_write_reports_runtime_component() {
        let compilation = compilation();
        let occurrence = occurrence();
        poison_embedded_constant_expectations(&compilation);

        assert_eq!(
            compilation.embedded_constant_key(occurrence),
            Err(poisoned_expectations_error())
        );
    }

    #[test]
    fn poisoned_embedded_constant_expectation_read_stays_distinct_from_cancellation() {
        let compilation = compilation();
        let occurrence = occurrence();
        poison_embedded_constant_expectations(&compilation);

        assert_eq!(
            compilation.embedded_constant_expected_type(occurrence.key()),
            Err(poisoned_expectations_error())
        );

        let cancellation = CancellationToken::new();
        cancellation.cancel();

        assert_eq!(cancellation.check(), Err(FactQueryError::Cancelled));
    }

    #[test]
    fn conflicting_embedded_constant_expectations_retain_both_types_and_source() {
        let compilation = compilation();
        let expected = occurrence();

        let actual = ConstantExpressionOccurrence::new(
            expected.key(),
            ConstantExpressionExpectedType::GenericParameter(
                GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(3)),
            ),
        );

        let _ = compilation.embedded_constant_key(expected);

        let syntax = actual.key().syntax();

        let expected_error: FactQueryError =
            crate::compilation::SemanticQueryFailure::located_contract(
                crate::compilation::SemanticQueryContext::Symbol(actual.key().owner()),
                crate::compilation::SemanticQueryViolation::ConstantExpectationMismatch {
                    expected: expected.expected_type(),
                    actual: actual.expected_type(),
                },
                bray_source::SourceSpan::new(syntax.source_id(), syntax.full_range()),
            )
            .into();

        assert_eq!(
            compilation.embedded_constant_key(actual),
            Err(expected_error)
        );
    }

    fn compilation() -> Compilation {
        match Compilation::load(CompilationRequest::new(package_identity(), Vec::new())) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation must load: {error:?}"),
        }
    }

    fn occurrence() -> ConstantExpressionOccurrence {
        let source = SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("embedded-constant-poison-test"),
            SourceVersion::new(1),
            "module example;\nconst value: u8 = 4;\n",
        );

        let source = match source {
            Ok(source) => source,
            Err(error) => panic!("test source must fit: {error:?}"),
        };

        let parsed = parse_source_unit(&source);

        let literals =
            bray_testing::syntax_descendants::<LiteralExpressionSyntax>(parsed.source_unit());

        let [literal] = literals.as_slice() else {
            panic!("test source must contain one literal");
        };

        ConstantExpressionOccurrence::new(
            ConstantExpressionOccurrenceKey::new(
                AnySymbolId::from(ConstantSymbolId::from_symbol_id(SymbolId::new(1))),
                SyntaxAnchor::from_node(literal),
            ),
            ConstantExpressionExpectedType::GenericParameter(
                GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(2)),
            ),
        )
    }

    fn poison_embedded_constant_expectations(compilation: &Compilation) {
        let _ = catch_unwind(AssertUnwindSafe(|| {
            let _expectations = compilation
                .state
                .embedded_constant_expectations
                .lock()
                .unwrap_or_else(|_| panic!("expectation state must begin available"));

            panic!("poison embedded constant expectations");
        }));
    }

    fn poisoned_expectations_error() -> FactQueryError {
        FactRuntimeFailure::SynchronizationPoisoned {
            component: SynchronizationComponent::EmbeddedConstantExpectations,
            fact: None,
            task: None,
        }
        .into()
    }
}
