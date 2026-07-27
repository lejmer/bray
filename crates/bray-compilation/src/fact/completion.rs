use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, SymbolCompletionLevel, SymbolCompletionPlanError, SymbolFactCompletionRequest,
    SymbolFactForcer, SymbolGraph,
};

use super::{CancellationToken, FactRuntime};

/// An outer force-completion outcome that is not a source diagnostic.
#[derive(Debug, Eq, PartialEq)]
pub enum SymbolCompletionError<E> {
    /// Completion was cancelled and no partial diagnostics were returned.
    Cancelled,
    /// The requested root does not belong to the symbol graph.
    UnknownSymbol(AnySymbolId),
    /// The fact provider could not be prepared for completion.
    Provider(E),
    /// One exact fact request failed at the query boundary.
    Fact {
        /// The canonical request whose provider failed.
        request: SymbolFactCompletionRequest,
        /// The provider-specific query error.
        error: E,
    },
    /// A scoped completion worker panicked before returning its result.
    WorkerFailure,
}

impl<E: std::fmt::Display> std::fmt::Display for SymbolCompletionError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("symbol completion was cancelled"),
            Self::UnknownSymbol(symbol) => {
                write!(formatter, "symbol completion root {symbol:?} is unknown")
            }
            Self::Provider(error) => {
                write!(formatter, "symbol completion provider failed: {error}")
            }
            Self::Fact { request, error } => write!(
                formatter,
                "symbol fact {:?} for {:?} failed: {error}",
                request.kind(),
                request.symbol()
            ),
            Self::WorkerFailure => formatter.write_str("a symbol completion worker failed"),
        }
    }
}

impl<E> std::error::Error for SymbolCompletionError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Provider(error) | Self::Fact { error, .. } => Some(error),
            Self::Cancelled | Self::UnknownSymbol(_) | Self::WorkerFailure => None,
        }
    }
}

/// Forces one symbol-owned subtree and deterministically aggregates fact diagnostics.
///
/// Diagnostics are returned only after every required fact completes successfully.
pub(crate) fn force_complete_symbol<F>(
    graph: &SymbolGraph,
    root: AnySymbolId,
    level: SymbolCompletionLevel,
    runtime: &FactRuntime,
    cancellation: &CancellationToken,
    forcer: &F,
) -> Result<DiagnosticBag, SymbolCompletionError<F::Error>>
where
    F: SymbolFactForcer + ?Sized,
    F::Error: Send,
{
    let plan = match graph.completion_plan(root, level, cancellation) {
        Ok(plan) => plan,
        Err(SymbolCompletionPlanError::Cancelled) => {
            return Err(SymbolCompletionError::Cancelled);
        }
        Err(SymbolCompletionPlanError::UnknownSymbol(symbol)) => {
            return Err(SymbolCompletionError::UnknownSymbol(symbol));
        }
    };

    let diagnostics = force_scheduled(plan.requests(), runtime, cancellation, forcer)?;

    Ok(DiagnosticBag::merged_all(diagnostics.iter()))
}

fn force_scheduled<F>(
    requests: &[SymbolFactCompletionRequest],
    runtime: &FactRuntime,
    cancellation: &CancellationToken,
    forcer: &F,
) -> Result<Vec<DiagnosticBag>, SymbolCompletionError<F::Error>>
where
    F: SymbolFactForcer + ?Sized,
    F::Error: Send,
{
    let scheduled = catch_unwind(AssertUnwindSafe(|| {
        runtime.map_indexed(requests.len(), |index| {
            if cancellation.is_cancelled() {
                None
            } else {
                Some(forcer.force(requests[index]))
            }
        })
    }));

    let indexed = match scheduled {
        Ok(Ok(indexed)) => indexed,
        Ok(Err(_)) | Err(_) => return Err(SymbolCompletionError::WorkerFailure),
    };

    let mut diagnostics = Vec::with_capacity(requests.len());

    for (request, result) in requests.iter().copied().zip(indexed) {
        let Some(result) = result else {
            return Err(SymbolCompletionError::Cancelled);
        };

        match result {
            Ok(fact_diagnostics) => diagnostics.push(fact_diagnostics),
            Err(error) => return Err(SymbolCompletionError::Fact { request, error }),
        }
    }

    if cancellation.is_cancelled() {
        return Err(SymbolCompletionError::Cancelled);
    }

    Ok(diagnostics)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::{Condvar, Mutex};

    use bray_diagnostics::{
        Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
    };
    use bray_source::{SourceIdentity, SourceInput, SourceVersion, TextSize};
    use bray_symbols::{
        AnySymbolId, SymbolCompletionLevel, SymbolFactCompletionRequest, SymbolFactKind,
        SymbolGraph,
    };

    use super::{SymbolCompletionError, force_complete_symbol};
    use crate::fact::{CompilationFactKey, FactRuntime, SymbolFactKey};
    use crate::{CancellationToken, Compilation, FactCycle, FactQueryError, WorkerBudget};

    #[test]
    fn serial_and_parallel_completion_merge_diagnostics_in_plan_order() {
        let graph =
            graph("module app; struct Config<T> { value: T = 1; func read() {} } func main() {}");

        let package = AnySymbolId::from(graph.packages()[0].id());

        let plan = match graph.completion_plan(
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &bray_symbols::NeverCancelSymbolCompletion,
        ) {
            Ok(plan) => plan,
            Err(error) => panic!("completion plan should build: {error:?}"),
        };

        let ordinals = plan
            .requests()
            .iter()
            .copied()
            .enumerate()
            .map(|(index, request)| (request, index))
            .collect::<BTreeMap<_, _>>();

        let forcer = |request: SymbolFactCompletionRequest| -> Result<DiagnosticBag, ()> {
            let Some(index) = ordinals.get(&request).copied() else {
                return Err(());
            };

            let diagnostic_id = match u32::try_from(index) {
                Ok(index) => DiagnosticId::new(index),
                Err(_) => return Err(()),
            };

            let offset = match u32::try_from(index) {
                Ok(index) => TextSize::new(index),
                Err(_) => return Err(()),
            };

            Ok(DiagnosticBag::single(
                Diagnostic::new(
                    diagnostic_id,
                    DiagnosticKind::DeclarationDuplicateName,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::text_offset(offset)),
            ))
        };

        let serial = force(&graph, package, WorkerBudget::serial(), &forcer);
        let parallel = force(&graph, package, worker_budget(4), &forcer);

        assert_eq!(serial, parallel);

        assert_eq!(
            serial
                .iter()
                .map(|diagnostic| diagnostic.id())
                .collect::<Vec<_>>(),
            (0..plan.requests().len())
                .filter_map(|index| u32::try_from(index).ok())
                .map(DiagnosticId::new)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn parallel_completion_selects_the_first_plan_error_after_later_error_finishes() {
        let graph = graph("module app; func main() {}");
        let package = AnySymbolId::from(graph.packages()[0].id());

        let plan = match graph.completion_plan(
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &bray_symbols::NeverCancelSymbolCompletion,
        ) {
            Ok(plan) => plan,
            Err(error) => panic!("completion plan should build: {error:?}"),
        };

        let [first_request, second_request, ..] = plan.requests() else {
            panic!("test completion plan should contain multiple fact requests");
        };

        let first_request = *first_request;
        let second_request = *second_request;

        let serial_forcer = |request| -> Result<DiagnosticBag, ForcedFactError> {
            if request == first_request || request == second_request {
                return Err(ForcedFactError::Request(request));
            }

            Ok(DiagnosticBag::new())
        };

        let serial = force_complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(WorkerBudget::serial()),
            &CancellationToken::new(),
            &serial_forcer,
        );

        let parallel_forcer = LaterErrorFirstForcer::new(first_request, second_request);

        let parallel = force_complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(worker_budget(2)),
            &CancellationToken::new(),
            &parallel_forcer,
        );

        let expected = Err(SymbolCompletionError::Fact {
            request: first_request,
            error: ForcedFactError::Request(first_request),
        });

        assert_eq!(serial, expected);
        assert_eq!(parallel, expected);

        assert!(parallel_forcer.later_error_completed());
    }

    #[test]
    fn cancellation_discards_partial_completion_diagnostics() {
        let graph = graph("module app; func main(value: Int) {}");
        let package = AnySymbolId::from(graph.packages()[0].id());
        let cancellation = CancellationToken::new();

        let forcer = |_request: SymbolFactCompletionRequest| -> Result<DiagnosticBag, ()> {
            cancellation.cancel();

            Ok(DiagnosticBag::single(Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::DeclarationDuplicateName,
                SeverityKind::Error,
            )))
        };

        let result = force_complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(WorkerBudget::serial()),
            &cancellation,
            &forcer,
        );

        assert!(matches!(result, Err(SymbolCompletionError::Cancelled)));
    }

    #[test]
    fn parallel_completion_observes_cancellation_without_publishing() {
        let graph = graph("module app; func first() {} func second() {}");
        let package = AnySymbolId::from(graph.packages()[0].id());
        let cancellation = CancellationToken::new();

        let forcer = |_request: SymbolFactCompletionRequest| -> Result<DiagnosticBag, ()> {
            cancellation.cancel();

            Ok(DiagnosticBag::single(Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::DeclarationDuplicateName,
                SeverityKind::Error,
            )))
        };

        let result = force_complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(worker_budget(4)),
            &cancellation,
            &forcer,
        );

        assert!(matches!(result, Err(SymbolCompletionError::Cancelled)));
    }

    #[test]
    fn fact_cycles_are_outer_errors_unless_the_provider_recovers() {
        let graph = graph("module app; const First: Int = 1;");
        let constant = AnySymbolId::from(graph.constants()[0].id());

        let failing = |request: SymbolFactCompletionRequest| {
            if request.kind() == SymbolFactKind::GenericDeclarationTemplate {
                let key =
                    CompilationFactKey::from(SymbolFactKey::new(request.symbol(), request.kind()));

                return Err(FactQueryError::Cycle(FactCycle::new([key.clone(), key])));
            }

            Ok(DiagnosticBag::new())
        };

        let failure = force_complete_symbol(
            &graph,
            constant,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(WorkerBudget::serial()),
            &CancellationToken::new(),
            &failing,
        );

        assert!(matches!(
            failure,
            Err(SymbolCompletionError::Fact {
                request,
                error: FactQueryError::Cycle(_),
            }) if request.kind() == SymbolFactKind::GenericDeclarationTemplate
        ));

        let recovering =
            |request: SymbolFactCompletionRequest| -> Result<DiagnosticBag, FactQueryError> {
                if request.kind() == SymbolFactKind::GenericDeclarationTemplate {
                    return Ok(DiagnosticBag::single(Diagnostic::new(
                        DiagnosticId::new(7),
                        DiagnosticKind::DeclarationDuplicateName,
                        SeverityKind::Error,
                    )));
                }

                Ok(DiagnosticBag::new())
            };

        let recovered = force(&graph, constant, WorkerBudget::serial(), &recovering);

        assert_eq!(recovered.len(), 1);
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum ForcedFactError {
        Coordination,
        Request(SymbolFactCompletionRequest),
    }

    struct LaterErrorFirstForcer {
        first_request: SymbolFactCompletionRequest,
        later_request: SymbolFactCompletionRequest,
        later_completed: Mutex<bool>,
        later_changed: Condvar,
    }

    impl LaterErrorFirstForcer {
        fn new(
            first_request: SymbolFactCompletionRequest,
            later_request: SymbolFactCompletionRequest,
        ) -> Self {
            Self {
                first_request,
                later_request,
                later_completed: Mutex::new(false),
                later_changed: Condvar::new(),
            }
        }

        fn later_error_completed(&self) -> bool {
            match self.later_completed.lock() {
                Ok(completed) => *completed,
                Err(_) => false,
            }
        }
    }

    impl bray_symbols::SymbolFactForcer for LaterErrorFirstForcer {
        type Error = ForcedFactError;

        fn force(
            &self,
            request: SymbolFactCompletionRequest,
        ) -> Result<DiagnosticBag, Self::Error> {
            if request == self.first_request {
                let mut later_completed = self
                    .later_completed
                    .lock()
                    .map_err(|_| ForcedFactError::Coordination)?;

                while !*later_completed {
                    later_completed = self
                        .later_changed
                        .wait(later_completed)
                        .map_err(|_| ForcedFactError::Coordination)?;
                }

                return Err(ForcedFactError::Request(request));
            }

            if request == self.later_request {
                let mut later_completed = self
                    .later_completed
                    .lock()
                    .map_err(|_| ForcedFactError::Coordination)?;

                *later_completed = true;

                self.later_changed.notify_all();

                return Err(ForcedFactError::Request(request));
            }

            Ok(DiagnosticBag::new())
        }
    }

    fn force<F>(
        graph: &SymbolGraph,
        root: AnySymbolId,
        workers: WorkerBudget,
        forcer: &F,
    ) -> DiagnosticBag
    where
        F: bray_symbols::SymbolFactForcer,
        F::Error: std::fmt::Debug + Send,
    {
        let runtime = FactRuntime::new(workers);

        match force_complete_symbol(
            graph,
            root,
            SymbolCompletionLevel::DeclarationSurface,
            &runtime,
            &CancellationToken::new(),
            forcer,
        ) {
            Ok(diagnostics) => diagnostics,
            Err(error) => panic!("completion should succeed: {error:?}"),
        }
    }

    fn worker_budget(workers: usize) -> WorkerBudget {
        match WorkerBudget::new(workers) {
            Ok(budget) => budget,
            Err(error) => panic!("test worker budget should be valid: {error:?}"),
        }
    }

    fn graph(text: &str) -> SymbolGraph {
        let package = crate::test_support::package_identity();

        let compilation = match Compilation::load_sources(
            package.clone(),
            vec![SourceInput::virtual_text(
                SourceIdentity::new(0),
                "completion.bray",
                SourceVersion::new(0),
                text,
            )],
        ) {
            Ok(compilation) => compilation,
            Err(error) => panic!("test compilation should load: {error:?}"),
        };

        match SymbolGraph::build_source(
            package,
            compilation.declaration_table(),
            compilation.syntax_tree(),
        ) {
            Ok(graph) => graph,
            Err(error) => panic!("test symbol graph should build: {error:?}"),
        }
    }
}
