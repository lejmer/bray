use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundDependencyRequirementKind, BoundDependencySubject, BoundExecutionSite, BoundExpressionId,
    StorageAccessId, StorageAccessPurpose,
};
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind};
use bray_symbols::TypeId;

use crate::analysis::id::AnalysisBlockId;
use crate::{CheckerQueryError, CheckerQueryResult, CheckerRequestContext};

use super::core::StorageFlowCollector;

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum DiagnosticSubject {
    Storage(DiagnosticKind, StorageAccessId),
    Memory(DiagnosticKind, BoundExecutionSite),
    Terminal(BoundExecutionSite),
}

pub(super) enum DiagnosticArgument {
    StorageAccess(StorageAccessId, StorageAccessPurpose),
    ActualType(TypeId),
}

enum BlockCheck<Upstream> {
    Diagnostic {
        subject: DiagnosticSubject,
        diagnostic: Diagnostic,
        argument: Option<DiagnosticArgument>,
    },
    IndependentDiagnostic(Diagnostic),
    CallableCleanup(BoundExpressionId),
    TaskStart(BoundExpressionId, Vec<(BoundDependencySubject, BoundDependencyRequirementKind)>),
    QueryFailure(CheckerQueryError<Upstream>),
}

pub(super) struct BlockChecks<Upstream> {
    pub(super) block: Option<AnalysisBlockId>,
    entries: BTreeMap<Option<AnalysisBlockId>, Vec<BlockCheck<Upstream>>>,
}

impl<Upstream> Default for BlockChecks<Upstream> {
    fn default() -> Self {
        Self { block: None, entries: BTreeMap::new() }
    }
}

impl<Upstream> BlockChecks<Upstream> {
    pub(super) fn record(&mut self, subject: DiagnosticSubject, diagnostic: Diagnostic) {
        self.record_with_argument(subject, diagnostic, None);
    }

    pub(super) fn record_with_argument(
        &mut self,
        subject: DiagnosticSubject,
        diagnostic: Diagnostic,
        argument: Option<DiagnosticArgument>,
    ) {
        self.push(BlockCheck::Diagnostic {
            subject,
            diagnostic,
            argument,
        });
    }

    pub(super) fn record_callable_cleanup(&mut self, expression: BoundExpressionId) {
        self.push(BlockCheck::CallableCleanup(expression));
    }

    pub(super) fn record_task_start(
        &mut self,
        expression: BoundExpressionId,
        unavailable: Vec<(BoundDependencySubject, BoundDependencyRequirementKind)>,
    ) {
        self.push(BlockCheck::TaskStart(expression, unavailable));
    }

    pub(super) fn record_query_failure(&mut self, error: CheckerQueryError<Upstream>) {
        self.push(BlockCheck::QueryFailure(error));
    }

    fn push(&mut self, check: BlockCheck<Upstream>) {
        self.entries.entry(self.block).or_default().push(check);
    }

    pub(super) fn add_range(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        let mut diagnostics = diagnostics.into_iter().peekable();

        if diagnostics.peek().is_some() {
            self.entries.entry(self.block).or_default().extend(diagnostics.map(BlockCheck::IndependentDiagnostic));
        }
    }

    pub(super) fn finish<C: CheckerRequestContext<UpstreamError = Upstream> + ?Sized>(
        self,
        collector: &mut StorageFlowCollector<'_, C>,
    ) -> CheckerQueryResult<DiagnosticBag, Upstream> {
        let mut published = DiagnosticBag::new();
        let mut seen = BTreeSet::new();
        let mut failed_starts = BTreeMap::<_, BTreeSet<_>>::new();

        for check in self.entries.into_values().flatten() {
            let (subject, diagnostic, argument) = match check {
                BlockCheck::Diagnostic { subject, diagnostic, argument } => (subject, diagnostic, argument),
                BlockCheck::IndependentDiagnostic(diagnostic) => {
                    published.add(diagnostic);
                    continue;
                }
                BlockCheck::CallableCleanup(expression) => {
                    collector.collect_callable_cleanup(expression)?;
                    continue;
                }
                BlockCheck::TaskStart(expression, unavailable) => {
                    failed_starts.entry(expression).or_default().extend(unavailable);
                    continue;
                }
                BlockCheck::QueryFailure(error) => return Err(error),
            };

            if !seen.insert(subject) { continue; }

            let id = DiagnosticId::from_index(published.len());
            let diagnostic = diagnostic.with_id(id);

            let diagnostic = match argument {
                Some(DiagnosticArgument::StorageAccess(access, purpose)) => diagnostic.with_arg(
                    DiagnosticArg::storage_access(collector.diagnostic_storage_access(access, purpose)?),
                ),
                Some(DiagnosticArgument::ActualType(ty)) => diagnostic.with_arg(DiagnosticArg::actual_type(
                    crate::diagnostic::diagnostic_type(collector.request.context(), ty)?,
                )),
                None => diagnostic,
            };

            published.add(diagnostic);
        }

        for (expression, unavailable) in failed_starts {
            let unavailable = unavailable.into_iter().collect::<Vec<_>>();

            let diagnostic = crate::asynchronous::task_start_dependency_diagnostic(
                collector.request, collector.storage, expression, &unavailable,
            ).with_id(DiagnosticId::from_index(published.len()));

            published.add(diagnostic);
        }

        Ok(published)
    }
}
