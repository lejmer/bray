use crate::BinderFactContext;
use crate::request::{AbandonedDependencyRelevance, BinderRequestCheckpoint, BinderRequestContext};

use super::{BindingError, BindingResult};

/// The caller's decision after binding one semantic candidate.
pub(crate) enum CandidateAction<Committed> {
    Commit(Committed),
    Abandon {
        dependency_relevance: AbandonedDependencyRelevance,
    },
}

/// The observable result of one candidate transaction.
pub(crate) enum CandidateResult<Committed> {
    Committed(Committed),
    Abandoned,
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
            Ok(value) if self.candidate_context_is_balanced(&checkpoint) => Ok(value),
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
    pub(crate) fn bind_candidate<Committed>(
        &mut self,
        bind: impl FnOnce(&mut Self) -> BindingResult<CandidateAction<Committed>>,
    ) -> BindingResult<CandidateResult<Committed>> {
        let checkpoint = self.checkpoint();

        match bind(self) {
            Ok(CandidateAction::Commit(value))
                if self.candidate_context_is_balanced(&checkpoint) =>
            {
                Ok(CandidateResult::Committed(value))
            }
            Ok(CandidateAction::Commit(_)) => {
                self.rollback_or_error(checkpoint, AbandonedDependencyRelevance::Relevant)?;

                Err(BindingError::CandidateContextMismatch)
            }
            Ok(CandidateAction::Abandon {
                dependency_relevance,
            }) => {
                self.rollback_or_error(checkpoint, dependency_relevance)?;

                Ok(CandidateResult::Abandoned)
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
        ControlTarget, ControlTargetKind, ExpectedContext, ExpectedSemanticKind,
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
            push_binding(request.unit_mut(), root, unit_fixture.first, false);

            request.add_diagnostic(diagnostic(1));
            request.record_dependency(dependency.clone());

            Ok::<_, BindingError>(CandidateAction::<()>::Abandon {
                dependency_relevance: AbandonedDependencyRelevance::Relevant,
            })
        });

        assert!(matches!(abandoned, Ok(CandidateResult::Abandoned)));

        let irrelevant = request.bind_candidate(|request| {
            request.add_diagnostic(diagnostic(2));
            request.record_dependency(irrelevant_dependency);

            Ok::<_, BindingError>(CandidateAction::<()>::Abandon {
                dependency_relevance: AbandonedDependencyRelevance::ProvenIrrelevant,
            })
        });

        assert!(matches!(irrelevant, Ok(CandidateResult::Abandoned)));

        let committed = request.bind_candidate(|request| {
            let binding = push_binding(request.unit_mut(), root, unit_fixture.first, false);

            Ok::<_, BindingError>(CandidateAction::Commit(binding))
        });

        assert!(matches!(committed, Ok(CandidateResult::Committed(_))));

        let result = match request.finish() {
            Ok(result) => result,
            Err(error) => panic!("request must freeze: {error:?}"),
        };

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.dependencies(), &[dependency]);
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

            Ok::<_, BindingError>(CandidateAction::Commit(binding))
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
    fn candidate_context_mismatches_restore_exact_outer_stacks() {
        let fact_fixture = TestFixture::new();
        let facts = fact_fixture.context();
        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(26));
        let mut request = BinderRequestContext::new(&facts, BindingContext::Expression, unit);
        let expected = ExpectedContext::Semantic(ExpectedSemanticKind::Value);
        let target = ControlTarget::new(ControlTargetKind::Loop, unit_fixture.first, None);

        request.push_expected(expected);
        request.push_control_target(target);

        let popped = request.bind_candidate(|request| {
            request.pop_expected();
            request.pop_control_target();

            Ok::<_, BindingError>(CandidateAction::Commit(()))
        });

        assert!(matches!(
            popped,
            Err(BindingError::CandidateContextMismatch)
        ));
        assert_eq!(request.expected(), Some(expected));
        assert_eq!(request.control_target(), Some(target));

        let replaced = request.bind_candidate(|request| {
            request.pop_expected();
            request.push_expected(ExpectedContext::Type(fact_fixture.declared_type));

            request.pop_control_target();
            request.push_control_target(ControlTarget::new(
                ControlTargetKind::Block,
                unit_fixture.first,
                None,
            ));

            Ok::<_, BindingError>(CandidateAction::Commit(()))
        });

        assert!(matches!(
            replaced,
            Err(BindingError::CandidateContextMismatch)
        ));
        assert_eq!(request.expected(), Some(expected));
        assert_eq!(request.control_target(), Some(target));
    }

    fn diagnostic(id: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
    }
}
