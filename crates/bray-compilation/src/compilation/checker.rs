use std::sync::Arc;

use bray_binder::{BinderFactContext, BinderFactError, SymbolFactProvider};
use bray_bound_tree::{BoundSourceAnchor, BoundUnit, BoundUnitKey};
use bray_checker::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerOutcome,
    CheckerRequestContext, CheckerSemanticFactProvider, CheckerSource,
    DefaultTargetValidityChecker, TargetValidity, TargetValidityChecker, TargetValidityContext,
    TargetValidityRequest,
};
use bray_diagnostics::DiagnosticResult;
use bray_source::{SourceSnapshot, SourceSpan};
use bray_symbols::{
    AvailableCompilerKnownSymbols, SemanticValueStore, SymbolFactContract, SymbolFactRequest,
    SymbolFactResult,
};
use bray_target::TargetProfile;

use super::Compilation;
use super::binder::CompilationBinderFacts;
use crate::fact::{CancellationToken, FactQueryError};

pub(super) struct CompilationCheckerContext<'compilation> {
    facts: CompilationBinderFacts<'compilation>,
}

impl<'compilation> CompilationCheckerContext<'compilation> {
    pub(super) fn new(facts: CompilationBinderFacts<'compilation>) -> Self {
        Self { facts }
    }

    pub(super) fn symbols(&self) -> &bray_symbols::SymbolGraph {
        self.facts.symbols()
    }
}

impl CheckerRequestContext for CompilationCheckerContext<'_> {
    fn semantic_context_matches(
        &self,
        unit: &BoundUnit,
        context: &bray_checker::SemanticUnitContext,
    ) -> bool {
        bray_binder::semantic_unit_context(self.symbols(), unit)
            .is_ok_and(|expected| expected == *context)
    }

    fn semantic_values(&self) -> &SemanticValueStore {
        self.facts.semantic_values()
    }

    fn symbols(&self) -> &bray_symbols::SymbolGraph {
        CompilationCheckerContext::symbols(self)
    }

    fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.facts
            .compilation()
            .selected_target()
            .available_compiler_known_symbols()
    }

    fn selected_target(&self) -> &TargetProfile {
        self.facts
            .compilation()
            .selected_target()
            .target()
            .profile()
    }

    fn checked_constant_expression(
        &self,
        occurrence: bray_symbols::ConstantExpressionOccurrence,
    ) -> CheckerFactResult<DiagnosticResult<bray_symbols::ConstantTermId>> {
        // The checker request contract returns an owned result across the crate boundary.
        self.facts
            .compilation()
            .embedded_constant_term_with_cancellation(occurrence, self.facts.cancellation())
            .map(|result| (*result).clone())
            .map_err(|error| match error {
                FactQueryError::Cancelled => CheckerFactError::Cancelled,
                FactQueryError::CheckerInfrastructure(error) => {
                    CheckerFactError::Infrastructure(error)
                }
                FactQueryError::Cycle(_)
                | FactQueryError::InfrastructureFailure
                | FactQueryError::SemanticUnitContext(_) => CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                ),
            })
    }

    fn generic_constraints(
        &self,
        obligation: bray_symbols::GenericConstraintObligationKey,
    ) -> CheckerFactResult<DiagnosticResult<bray_symbols::ProofOutcome>> {
        match self
            .facts
            .compilation()
            .generic_constraint_satisfaction_with_cancellation(
                obligation,
                self.facts.cancellation(),
            ) {
            Ok(result) => Ok((*result).clone()),
            Err(FactQueryError::Cycle(_)) => Ok(DiagnosticResult::without_diagnostics(
                bray_symbols::ProofOutcome::Unknown,
            )),
            Err(error) => Err(match error {
                FactQueryError::Cancelled => CheckerFactError::Cancelled,
                FactQueryError::CheckerInfrastructure(error) => {
                    CheckerFactError::Infrastructure(error)
                }
                FactQueryError::Cycle(_) => unreachable!("cycles are handled above"),
                FactQueryError::InfrastructureFailure | FactQueryError::SemanticUnitContext(_) => {
                    CheckerFactError::Infrastructure(
                        CheckerInfrastructureError::SemanticValueUnavailable,
                    )
                }
            }),
        }
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        checker_source(self.facts.compilation(), anchor)
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self.facts.cancellation()
    }
}

struct CompilationTargetValidityContext<'compilation> {
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
}

impl TargetValidityContext for CompilationTargetValidityContext<'_> {
    fn selected_target(&self) -> &TargetProfile {
        self.compilation.selected_target().target().profile()
    }

    fn source(
        &self,
        anchor: BoundSourceAnchor,
    ) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
        checker_source(self.compilation, anchor)
    }

    fn cancellation(&self) -> &dyn bray_base::Cancellation {
        self.cancellation
    }
}

fn checker_source_snapshot(
    compilation: &Compilation,
    anchor: BoundSourceAnchor,
) -> Result<&SourceSnapshot, CheckerInfrastructureError> {
    let source_id = anchor.syntax().source_id();

    let Some(source) = compilation.source(source_id) else {
        return Err(CheckerInfrastructureError::MissingSource { source_id });
    };

    if source.version() != anchor.source_version() {
        return Err(CheckerInfrastructureError::SourceVersionMismatch {
            source_id,
            expected: anchor.source_version(),
            actual: source.version(),
        });
    }

    Ok(source)
}

fn checker_source(
    compilation: &Compilation,
    anchor: BoundSourceAnchor,
) -> Result<CheckerSource<'_>, CheckerInfrastructureError> {
    let source = checker_source_snapshot(compilation, anchor)?;
    let range = anchor.syntax().full_range();
    let span = SourceSpan::new(source.source_id(), range);

    let Some(text) = source.text_slice(range) else {
        return Err(CheckerInfrastructureError::InvalidSourceRange { span });
    };

    Ok(CheckerSource::new(span, text))
}

impl<'compilation, C> CheckerSemanticFactProvider<C> for CompilationCheckerContext<'compilation>
where
    C: SymbolFactContract,
    CompilationBinderFacts<'compilation>: SymbolFactProvider<C>,
{
    fn symbol_fact(
        &self,
        request: SymbolFactRequest<C>,
    ) -> CheckerFactResult<Arc<SymbolFactResult<C>>> {
        self.facts
            .symbol_fact(request)
            .map_err(|error| match error {
                BinderFactError::Cancelled => CheckerFactError::Cancelled,
                BinderFactError::DependencyUnavailable => CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticFactUnavailable {
                        symbol: request.symbol(),
                        kind: request.kind(),
                    },
                ),
            })
    }
}

impl Compilation {
    /// Returns post-selection validity and diagnostics for one exact target requirement.
    pub fn target_validity(
        &self,
        request: TargetValidityRequest,
    ) -> Result<Arc<DiagnosticResult<TargetValidity>>, FactQueryError> {
        let cell = self.state.target_validity.cell(request.clone())?;

        let published = self.query_fact_with_cancellation(
            crate::fact::CompilationFactKey::TargetValidity(request.clone()),
            &cell,
            &self.state.cancellation,
            |cancellation| {
                let context = CompilationTargetValidityContext {
                    compilation: self,
                    cancellation,
                };

                checker_result(
                    DefaultTargetValidityChecker.check_target_validity(&context, &request),
                )
                .map(Arc::new)
            },
        )?;

        // The caller owns the immutable publication independently of the map cell guard.
        Ok(Arc::clone(published))
    }

    pub(super) fn checker_context_for<'compilation>(
        &'compilation self,
        key: &BoundUnitKey,
        cancellation: &'compilation CancellationToken,
    ) -> Result<CompilationCheckerContext<'compilation>, FactQueryError> {
        let facts = self.binder_facts_for(key, cancellation)?;

        Ok(CompilationCheckerContext::new(facts))
    }
}

pub(super) fn checker_result<T>(
    outcome: CheckerOutcome<T>,
) -> Result<DiagnosticResult<T>, FactQueryError> {
    match outcome {
        CheckerOutcome::Complete(result) => Ok(result),
        CheckerOutcome::Cancelled => Err(FactQueryError::Cancelled),
        CheckerOutcome::InfrastructureFailure(error) => {
            Err(FactQueryError::CheckerInfrastructure(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_binder::semantic_unit_context;
    use bray_bound_tree::BoundSourceAnchor;
    use bray_checker::{
        CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView, TargetValidity,
        TargetValidityRequest, TargetValidityRequirement,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::DiagnosticKind;
    use bray_source::SourceVersion;
    use bray_symbols::{CallableSignatureFact, CallableSymbolId, SymbolFactRequest, SymbolOrigin};

    use super::Compilation;
    use crate::fact::CompilationFactKey;
    use crate::test_support::{compilation, source_callable_body_key};

    #[test]
    fn contexts_resolve_only_the_source_range_named_by_a_bound_anchor() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let anchor = key.source();

        let source = match context.source(anchor) {
            Ok(source) => source,
            Err(error) => panic!("bound source must resolve: {error:?}"),
        };

        let span = source.span();

        assert_eq!(span.source_id(), anchor.syntax().source_id());
        assert_eq!(span.range(), anchor.syntax().full_range());

        let Some(snapshot) = compilation.source(span.source_id()) else {
            panic!("test compilation must retain its source");
        };

        assert_eq!(snapshot.text_slice(span.range()), Some(source.text()));
        assert_ne!(source.text(), snapshot.text());
    }

    #[test]
    fn contexts_reject_bound_anchors_from_another_source_revision() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let source = key.source();

        let stale = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        assert!(matches!(
            context.source(stale),
            Err(CheckerInfrastructureError::SourceVersionMismatch { .. })
        ));
    }

    #[test]
    fn contexts_supply_typed_symbol_facts_without_origin_specific_apis() {
        let compilation = callable_compilation();
        let key = source_callable_body_key(&compilation);

        let context = match compilation.checker_context_for(&key, &compilation.state.cancellation) {
            Ok(context) => context,
            Err(error) => panic!("checker context must be available: {error:?}"),
        };

        let bound = match compilation.bound_unit(key.clone()) {
            Ok(bound) => bound,
            Err(error) => panic!("bound unit must be available: {error:?}"),
        };

        let entry = match semantic_unit_context(context.symbols(), bound.value()) {
            Ok(entry) => entry,
            Err(error) => panic!("semantic unit context must be available: {error:?}"),
        };

        let request = match CheckerUnitView::new(bound.value(), &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("checker unit view must be valid: {error:?}"),
        };

        let graph = match compilation.symbol_graph() {
            Ok(graph) => graph,
            Err(error) => panic!("symbol graph must be available: {error:?}"),
        };

        let source_functions = graph
            .functions()
            .iter()
            .filter(|function| function.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [function] = source_functions.as_slice() else {
            panic!("test source must contain one function");
        };

        let signature_request =
            SymbolFactRequest::<CallableSignatureFact>::new(CallableSymbolId::from(function.id()));

        let signature = request.symbol_fact(signature_request);

        assert!(signature.is_ok());
    }

    #[test]
    fn target_validity_reuses_one_exact_published_fact() {
        let compilation = callable_compilation();
        let source = source_callable_body_key(&compilation).source();

        let request = TargetValidityRequest::new(
            source,
            TargetValidityRequirement::Representation(RepresentationRole::ScalarR16),
        );

        let first = match compilation.target_validity(request.clone()) {
            Ok(result) => result,
            Err(error) => panic!("target validity must be available: {error:?}"),
        };

        let second = match compilation.target_validity(request) {
            Ok(result) => result,
            Err(error) => panic!("target validity must be available: {error:?}"),
        };

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn unconditional_target_validity_does_not_demand_target_or_source() {
        let compilation = callable_compilation();
        let source = source_callable_body_key(&compilation).source();

        let stale_source = BoundSourceAnchor::new(
            source.syntax(),
            SourceVersion::new(source.source_version().raw() + 1),
        );

        let request = TargetValidityRequest::new(
            stale_source,
            TargetValidityRequirement::Representation(RepresentationRole::ScalarI32),
        );

        let key = CompilationFactKey::TargetValidity(request.clone());

        let validity = match compilation.target_validity(request) {
            Ok(result) => result,
            Err(error) => panic!("unconditional target validity must be available: {error:?}"),
        };

        let dependencies = match compilation.state.fact_runtime.dependencies(&key) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("target-validity dependencies must be published"),
            Err(error) => panic!("target-validity dependencies must be readable: {error:?}"),
        };

        assert_eq!(*validity.value(), TargetValidity::Valid);
        assert!(validity.diagnostics().is_empty());
        assert!(dependencies.is_empty());
    }

    #[test]
    fn target_dependent_invalidity_demands_target_and_reports_source() {
        let compilation = callable_compilation();
        let source = source_callable_body_key(&compilation).source();

        let request = TargetValidityRequest::new(
            source,
            TargetValidityRequirement::Representation(RepresentationRole::ScalarR16),
        );

        let key = CompilationFactKey::TargetValidity(request.clone());

        let validity = match compilation.target_validity(request) {
            Ok(result) => result,
            Err(error) => panic!("target validity must be available: {error:?}"),
        };

        let dependencies = match compilation.state.fact_runtime.dependencies(&key) {
            Ok(Some(dependencies)) => dependencies,
            Ok(None) => panic!("target-validity dependencies must be published"),
            Err(error) => panic!("target-validity dependencies must be readable: {error:?}"),
        };

        let [diagnostic] = validity.diagnostics().diagnostics() else {
            panic!("invalid target requirement must report one diagnostic");
        };

        assert_eq!(*validity.value(), TargetValidity::Invalid);

        assert_eq!(
            diagnostic.kind(),
            DiagnosticKind::CheckingTargetRepresentationUnavailable
        );

        assert_eq!(
            diagnostic.primary_span().map(|span| span.range()),
            Some(source.syntax().full_range())
        );

        assert_eq!(dependencies.as_ref(), [CompilationFactKey::SelectedTarget]);
    }

    fn callable_compilation() -> Compilation {
        compilation(
            r#"module app;
func main()
{
}
"#,
        )
    }
}
