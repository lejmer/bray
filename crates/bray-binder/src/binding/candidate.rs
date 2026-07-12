use crate::BinderFactContext;
use crate::request::{AbandonedDependencyRelevance, BinderRequestCheckpoint, BinderRequestContext};

use super::{BindingError, BindingResult};

/// The caller's decision after binding one semantic candidate.
pub(crate) enum CandidateAction<Committed, Abandoned> {
    Commit(Committed),
    Abandon {
        summary: Abandoned,
        dependency_relevance: AbandonedDependencyRelevance,
    },
}

/// The observable result of one candidate transaction.
pub(crate) enum CandidateResult<Committed, Abandoned> {
    Committed(Committed),
    Abandoned(Abandoned),
}

impl<C> BinderRequestContext<'_, C>
where
    C: BinderFactContext + ?Sized,
{
    /// Runs an atomic binding operation whose failed state is never published.
    pub(crate) fn bind_transaction<T>(
        &mut self,
        bind: impl FnOnce(&mut Self) -> BindingResult<T>,
    ) -> BindingResult<T> {
        let checkpoint = self.checkpoint();

        match bind(self) {
            Ok(value) if self.candidate_context_is_balanced(checkpoint) => Ok(value),
            Ok(_) => {
                self.rollback_or_error(checkpoint, AbandonedDependencyRelevance::ProvenIrrelevant)?;

                Err(BindingError::CandidateContextMismatch)
            }
            Err(error) => {
                self.rollback_or_error(checkpoint, AbandonedDependencyRelevance::ProvenIrrelevant)?;

                Err(error)
            }
        }
    }

    /// Binds one semantic candidate and commits or abandons it deterministically.
    pub(crate) fn bind_candidate<Committed, Abandoned>(
        &mut self,
        bind: impl FnOnce(&mut Self) -> BindingResult<CandidateAction<Committed, Abandoned>>,
    ) -> BindingResult<CandidateResult<Committed, Abandoned>> {
        let checkpoint = self.checkpoint();

        match bind(self) {
            Ok(CandidateAction::Commit(value))
                if self.candidate_context_is_balanced(checkpoint) =>
            {
                Ok(CandidateResult::Committed(value))
            }
            Ok(CandidateAction::Commit(_)) => {
                self.rollback_or_error(checkpoint, AbandonedDependencyRelevance::Relevant)?;

                Err(BindingError::CandidateContextMismatch)
            }
            Ok(CandidateAction::Abandon {
                summary,
                dependency_relevance,
            }) => {
                self.rollback_or_error(checkpoint, dependency_relevance)?;

                Ok(CandidateResult::Abandoned(summary))
            }
            Err(error) => {
                self.rollback_or_error(checkpoint, AbandonedDependencyRelevance::Relevant)?;

                Err(error)
            }
        }
    }

    fn rollback_or_error(
        &mut self,
        checkpoint: BinderRequestCheckpoint,
        dependency_relevance: AbandonedDependencyRelevance,
    ) -> BindingResult<()> {
        if self.rollback(checkpoint, dependency_relevance) {
            Ok(())
        } else {
            Err(BindingError::RollbackFailed)
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{Diagnostic, DiagnosticId, DiagnosticKind, SeverityKind};
    use bray_symbols::{LocalSymbolRegionId, SymbolFactKind};

    use super::{CandidateAction, CandidateResult};
    use crate::binding::BindingError;
    use crate::fact::test_support::TestFixture;
    use crate::request::{
        AbandonedDependencyRelevance, BinderDependency, BinderRequestContext, BindingContext,
        ExpectedContext, ExpectedSemanticKind,
    };
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn abandoned_candidates_restore_state_and_classify_dependencies() {
        let fact_fixture = TestFixture::new();
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(24));
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);
        let root = request.unit().root_scope();
        let dependency = BinderDependency::Symbol {
            symbol: fact_fixture.constant.into(),
            kind: SymbolFactKind::ConstantDefinition,
        };
        let irrelevant_dependency = BinderDependency::Target(fact_fixture.constant);

        let abandoned = request.bind_candidate(|request| {
            let binding = push_binding(request.unit_mut(), root, unit_fixture.first, false);

            request.add_diagnostic(diagnostic(1));
            request.record_dependency(dependency.clone());

            Ok::<_, BindingError>(
                CandidateAction::<bray_symbols::LocalBindingSymbolId, _>::Abandon {
                    summary: binding,
                    dependency_relevance: AbandonedDependencyRelevance::Relevant,
                },
            )
        });

        let abandoned = match abandoned {
            Ok(CandidateResult::Abandoned(binding)) => binding,
            Ok(CandidateResult::Committed(_)) => panic!("candidate must be abandoned"),
            Err(error) => panic!("candidate abandonment must succeed: {error:?}"),
        };

        let irrelevant = request.bind_candidate(|request| {
            request.add_diagnostic(diagnostic(2));
            request.record_dependency(irrelevant_dependency);

            Ok::<_, BindingError>(CandidateAction::<(), _>::Abandon {
                summary: "irrelevant",
                dependency_relevance: AbandonedDependencyRelevance::ProvenIrrelevant,
            })
        });

        assert!(matches!(
            irrelevant,
            Ok(CandidateResult::Abandoned("irrelevant"))
        ));

        let committed = request.bind_candidate(|request| {
            let binding = push_binding(request.unit_mut(), root, unit_fixture.first, false);

            Ok::<_, BindingError>(CandidateAction::<_, ()>::Commit(binding))
        });

        let reused = match committed {
            Ok(CandidateResult::Committed(binding)) => binding,
            Ok(CandidateResult::Abandoned(())) => panic!("candidate must commit"),
            Err(error) => panic!("candidate commit must succeed: {error:?}"),
        };

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("request must freeze: {error:?}"),
        };

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.dependencies(), &[dependency]);
        assert_eq!(reused, abandoned);
        assert_eq!(result.unit().local_symbols().bindings().len(), 1);
    }

    #[test]
    fn committed_candidates_publish_through_the_ordinary_request() {
        let fact_fixture = TestFixture::new();
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(25));
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);
        let root = request.unit().root_scope();

        let committed = request.bind_candidate(|request| {
            let binding = push_binding(request.unit_mut(), root, unit_fixture.first, false);

            request.add_diagnostic(diagnostic(2));

            Ok::<_, BindingError>(CandidateAction::<_, ()>::Commit(binding))
        });

        assert!(matches!(committed, Ok(CandidateResult::Committed(_))));

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("request must freeze: {error:?}"),
        };

        assert_eq!(result.diagnostics().len(), 1);
        assert_eq!(result.unit().local_symbols().bindings().len(), 1);
    }

    #[test]
    fn malformed_candidate_contexts_rollback_without_panicking() {
        let fact_fixture = TestFixture::new();
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(26));
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);

        let result = request.bind_candidate(|request| {
            request.push_expected(ExpectedContext::Semantic(ExpectedSemanticKind::Value));

            Ok::<_, BindingError>(CandidateAction::<(), ()>::Commit(()))
        });

        assert!(matches!(
            result,
            Err(BindingError::CandidateContextMismatch)
        ));
        assert_eq!(request.expected(), None);
    }

    fn diagnostic(id: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
    }
}
