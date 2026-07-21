use crate::BinderFactContext;
use crate::binder::{AbandonedDependencyRelevance, Binder, BinderCheckpoint};

use super::super::{BindingError, BindingResult};

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

impl<C> Binder<'_, C>
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
        checkpoint: BinderCheckpoint,
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
    use crate::binder::{
        AbandonedDependencyRelevance, Binder, BinderDependency, BindingContext, ControlTarget,
        ControlTargetKind, ExpectedContext, ExpectedSemanticKind,
    };
    use crate::binding::BindingError;
    use crate::fact::test_support::TestFixture;
    use crate::unit::test_support::{builder, fixture, push_binding};

    #[test]
    fn abandoned_candidates_restore_state_and_classify_dependencies() {
        let fact_fixture = TestFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(24));

        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        let root = binder.unit().root_scope();

        let dependency = BinderDependency::Symbol {
            symbol: fact_fixture.constant.into(),
            kind: SymbolFactKind::ConstantDefinition,
        };

        let irrelevant_dependency = BinderDependency::Target(fact_fixture.constant);

        let abandoned = binder.bind_candidate(|binder| {
            push_binding(binder.unit_mut(), root, unit_fixture.first, false);

            binder.add_diagnostic(diagnostic(1));
            binder.record_dependency(dependency.clone());

            Ok::<_, BindingError>(CandidateAction::<()>::Abandon {
                dependency_relevance: AbandonedDependencyRelevance::Relevant,
            })
        });

        assert!(matches!(abandoned, Ok(CandidateResult::Abandoned)));

        let irrelevant = binder.bind_candidate(|binder| {
            binder.add_diagnostic(diagnostic(2));
            binder.record_dependency(irrelevant_dependency);

            Ok::<_, BindingError>(CandidateAction::<()>::Abandon {
                dependency_relevance: AbandonedDependencyRelevance::ProvenIrrelevant,
            })
        });

        assert!(matches!(irrelevant, Ok(CandidateResult::Abandoned)));

        let committed = binder.bind_candidate(|binder| {
            let binding = push_binding(binder.unit_mut(), root, unit_fixture.first, false);

            Ok::<_, BindingError>(CandidateAction::Commit(binding))
        });

        assert!(matches!(committed, Ok(CandidateResult::Committed(_))));

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("binder must freeze: {error:?}"),
        };

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.dependencies(), &[dependency]);
        assert_eq!(result.unit().local_symbols().bindings().len(), 1);
    }

    #[test]
    fn committed_candidates_publish_through_the_ordinary_binder() {
        let fact_fixture = TestFixture::new();
        let facts = fact_fixture.context();

        let unit_fixture = fixture();
        let unit = builder(&unit_fixture, LocalSymbolRegionId::new(25));

        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        let root = binder.unit().root_scope();

        let committed = binder.bind_candidate(|binder| {
            let binding = push_binding(binder.unit_mut(), root, unit_fixture.first, false);

            binder.add_diagnostic(diagnostic(2));

            Ok::<_, BindingError>(CandidateAction::Commit(binding))
        });

        assert!(matches!(committed, Ok(CandidateResult::Committed(_))));

        let result = match binder.finish() {
            Ok(result) => result,
            Err(error) => panic!("binder must freeze: {error:?}"),
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

        let mut binder = Binder::new(&facts, BindingContext::Expression, unit);

        let expected = ExpectedContext::Semantic(ExpectedSemanticKind::Value);
        let target = ControlTarget::new(ControlTargetKind::Loop, unit_fixture.first, None);

        binder.push_expected(expected);
        binder.push_control_target(target);

        let popped = binder.bind_candidate(|binder| {
            binder.pop_expected();
            binder.pop_control_target();

            Ok::<_, BindingError>(CandidateAction::Commit(()))
        });

        assert!(matches!(
            popped,
            Err(BindingError::CandidateContextMismatch)
        ));

        assert_eq!(binder.expected(), Some(expected));
        assert_eq!(binder.control_target(), Some(target));

        let replaced = binder.bind_candidate(|binder| {
            binder.pop_expected();
            binder.push_expected(ExpectedContext::Type(fact_fixture.declared_type));

            binder.pop_control_target();
            binder.push_control_target(ControlTarget::new(
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

        assert_eq!(binder.expected(), Some(expected));
        assert_eq!(binder.control_target(), Some(target));
    }

    fn diagnostic(id: u32) -> Diagnostic {
        Diagnostic::new(
            DiagnosticId::new(id),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        )
    }
}
