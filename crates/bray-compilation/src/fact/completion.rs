use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, SymbolCompletionEvaluator, SymbolCompletionLevel, SymbolCompletionPlanError,
    SymbolCompletionQuery, SymbolGraph,
};

use super::{
    BatchCompletionError, BatchWork, CancellationToken, DiagnosticPublicationOrder, FactQueryError,
    FactRuntime, OrderedDiagnosticCollection, publish_diagnostics,
};

/// An outer symbol-completion outcome that is not a source diagnostic.
#[derive(Debug, Eq, PartialEq)]
pub enum SymbolCompletionError<E> {
    /// Completion was cancelled and no partial diagnostics were returned.
    Cancelled,
    /// The requested root does not belong to the symbol graph.
    UnknownSymbol(AnySymbolId),
    /// The query evaluator could not be prepared for completion.
    Evaluator(E),
    /// One exact query failed at the completion boundary.
    Query {
        /// The stable query whose evaluator failed.
        request: SymbolCompletionQuery,
        /// The evaluator-specific query error.
        error: E,
    },
    /// Compiler query scheduling or worker publication failed.
    Scheduler(FactQueryError),
    /// The compiler-owned evaluator panicked while processing a completion request.
    EvaluatorPanic {
        /// The exact request whose evaluator panicked.
        request: SymbolCompletionQuery,
    },
}

impl<E: std::fmt::Display> std::fmt::Display for SymbolCompletionError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("symbol completion was cancelled"),
            Self::UnknownSymbol(symbol) => {
                write!(formatter, "symbol completion root {symbol:?} is unknown")
            }
            Self::Evaluator(error) => {
                write!(formatter, "symbol completion evaluator failed: {error}")
            }
            Self::Query { request, error } => write!(
                formatter,
                "symbol query {:?} for {:?} failed: {error}",
                request.kind(),
                request.symbol()
            ),
            Self::Scheduler(error) => {
                write!(formatter, "symbol completion scheduling failed: {error}")
            }
            Self::EvaluatorPanic { request } => write!(
                formatter,
                "the symbol completion evaluator panicked for {:?} on {:?}",
                request.kind(),
                request.symbol()
            ),
        }
    }
}

impl<E> std::error::Error for SymbolCompletionError<E>
where
    E: std::error::Error + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Evaluator(error) | Self::Query { error, .. } => Some(error),
            Self::Scheduler(error) => Some(error),
            Self::Cancelled | Self::UnknownSymbol(_) | Self::EvaluatorPanic { .. } => None,
        }
    }
}

/// Evaluates one symbol-owned subtree and deterministically aggregates query diagnostics.
///
/// Diagnostics are returned only after every required query completes successfully.
pub(crate) fn complete_symbol<F>(
    graph: &SymbolGraph,
    root: AnySymbolId,
    level: SymbolCompletionLevel,
    runtime: &FactRuntime,
    cancellation: &CancellationToken,
    evaluator: &F,
) -> Result<DiagnosticBag, SymbolCompletionError<F::Error>>
where
    F: SymbolCompletionEvaluator + ?Sized,
    F::Error: Send,
{
    let plan = match graph.completion_plan(root, level, cancellation) {
        Ok(plan) => plan,
        Err(SymbolCompletionPlanError::Cancelled) => {
            return match cancellation.check() {
                Ok(()) | Err(FactQueryError::Cancelled) => Err(SymbolCompletionError::Cancelled),
                Err(error) => Err(SymbolCompletionError::Scheduler(error)),
            };
        }
        Err(SymbolCompletionPlanError::UnknownSymbol(symbol)) => {
            return Err(SymbolCompletionError::UnknownSymbol(symbol));
        }
    };

    let diagnostics = runtime
        .complete_batch(plan.requests().iter().copied(), cancellation, |request| {
            match catch_unwind(AssertUnwindSafe(|| evaluator.evaluate(*request))) {
                Ok(Ok(diagnostics)) => Ok(BatchWork::leaf(diagnostics)),
                Ok(Err(error)) => Err(CompletionEvaluatorOutcome::Error(error)),
                Err(_) => Err(CompletionEvaluatorOutcome::Panic),
            }
        })
        .map_err(symbol_completion_error)?;

    let diagnostics = diagnostics
        .iter()
        .enumerate()
        .map(|(ordinal, (_, diagnostics))| {
            OrderedDiagnosticCollection::new(DiagnosticPublicationOrder::new(ordinal), diagnostics)
        })
        .collect();

    Ok(publish_diagnostics(runtime.profile(), diagnostics))
}

fn symbol_completion_error<E>(
    error: BatchCompletionError<SymbolCompletionQuery, CompletionEvaluatorOutcome<E>>,
) -> SymbolCompletionError<E> {
    match error {
        BatchCompletionError::Cancelled => SymbolCompletionError::Cancelled,
        BatchCompletionError::Evaluation {
            key,
            error: CompletionEvaluatorOutcome::Error(error),
        } => SymbolCompletionError::Query {
            request: key,
            error,
        },
        BatchCompletionError::Evaluation {
            key,
            error: CompletionEvaluatorOutcome::Panic,
        } => SymbolCompletionError::EvaluatorPanic { request: key },
        BatchCompletionError::Scheduler(error) => SymbolCompletionError::Scheduler(error),
    }
}

#[derive(Debug)]
enum CompletionEvaluatorOutcome<E> {
    Error(E),
    Panic,
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
        AnySymbolId, SymbolCompletionLevel, SymbolCompletionQuery, SymbolGraph, SymbolQueryKind,
    };

    use super::{SymbolCompletionError, complete_symbol, symbol_completion_error};
    use crate::fact::{
        BatchCompletionError, CompilationFactKey, FactRuntime, FactRuntimeFailure,
        SharedCancellation, SymbolQueryKey,
    };
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

        let evaluator = |request: SymbolCompletionQuery| -> Result<DiagnosticBag, ()> {
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

        let serial = complete(&graph, package, WorkerBudget::serial(), &evaluator);
        let parallel = complete(&graph, package, worker_budget(4), &evaluator);

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
            panic!("test completion plan should contain multiple query requests");
        };

        let first_request = *first_request;
        let second_request = *second_request;

        let serial_evaluator = |request| -> Result<DiagnosticBag, CompletionEvaluationError> {
            if request == first_request || request == second_request {
                return Err(CompletionEvaluationError::Request(request));
            }

            Ok(DiagnosticBag::new())
        };

        let serial = complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(WorkerBudget::serial()),
            &CancellationToken::new(),
            &serial_evaluator,
        );

        let parallel_evaluator = LaterErrorFirstEvaluator::new(first_request, second_request);

        let parallel = complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(worker_budget(2)),
            &CancellationToken::new(),
            &parallel_evaluator,
        );

        let expected = Err(SymbolCompletionError::Query {
            request: first_request,
            error: CompletionEvaluationError::Request(first_request),
        });

        assert_eq!(serial, expected);
        assert_eq!(parallel, expected);

        assert!(parallel_evaluator.later_error_completed());
    }

    #[test]
    fn cancellation_discards_partial_completion_diagnostics() {
        let graph = graph("module app; func main(value: Int) {}");
        let package = AnySymbolId::from(graph.packages()[0].id());
        let cancellation = CancellationToken::new();

        let evaluator = |_request: SymbolCompletionQuery| -> Result<DiagnosticBag, ()> {
            cancellation.cancel();

            Ok(DiagnosticBag::single(Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::DeclarationDuplicateName,
                SeverityKind::Error,
            )))
        };

        let result = complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(WorkerBudget::serial()),
            &cancellation,
            &evaluator,
        );

        assert!(matches!(result, Err(SymbolCompletionError::Cancelled)));
    }

    #[test]
    fn parallel_completion_observes_cancellation_without_publishing() {
        let graph = graph("module app; func first() {} func second() {}");
        let package = AnySymbolId::from(graph.packages()[0].id());
        let cancellation = CancellationToken::new();

        let evaluator = |_request: SymbolCompletionQuery| -> Result<DiagnosticBag, ()> {
            cancellation.cancel();

            Ok(DiagnosticBag::single(Diagnostic::new(
                DiagnosticId::new(0),
                DiagnosticKind::DeclarationDuplicateName,
                SeverityKind::Error,
            )))
        };

        let result = complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(worker_budget(4)),
            &cancellation,
            &evaluator,
        );

        assert!(matches!(result, Err(SymbolCompletionError::Cancelled)));
    }

    #[test]
    fn query_cycles_are_outer_errors_unless_the_evaluator_recovers() {
        let graph = graph("module app; const First: Int = 1;");
        let constant = AnySymbolId::from(graph.constants()[0].id());

        let failing = |request: SymbolCompletionQuery| {
            if request.kind() == SymbolQueryKind::GenericDeclarationTemplate {
                let key =
                    CompilationFactKey::from(SymbolQueryKey::new(request.symbol(), request.kind()));

                return Err(FactQueryError::Cycle(FactCycle::new([key.clone(), key])));
            }

            Ok(DiagnosticBag::new())
        };

        let failure = complete_symbol(
            &graph,
            constant,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(WorkerBudget::serial()),
            &CancellationToken::new(),
            &failing,
        );

        assert!(matches!(
            failure,
            Err(SymbolCompletionError::Query {
                request,
                error: FactQueryError::Cycle(_),
            }) if request.kind() == SymbolQueryKind::GenericDeclarationTemplate
        ));

        let recovering = |request: SymbolCompletionQuery| -> Result<DiagnosticBag, FactQueryError> {
            if request.kind() == SymbolQueryKind::GenericDeclarationTemplate {
                return Ok(DiagnosticBag::single(Diagnostic::new(
                    DiagnosticId::new(7),
                    DiagnosticKind::DeclarationDuplicateName,
                    SeverityKind::Error,
                )));
            }

            Ok(DiagnosticBag::new())
        };

        let recovered = complete(&graph, constant, WorkerBudget::serial(), &recovering);

        assert_eq!(recovered.len(), 1);
    }

    #[test]
    fn scheduler_runtime_failures_survive_completion_unchanged() {
        let failure = FactQueryError::from(FactRuntimeFailure::WorkerTerminated {
            worker: Some(3),
            item: Some(5),
        });

        let completion = symbol_completion_error::<()>(BatchCompletionError::Scheduler(failure));

        assert!(matches!(
            completion,
            SymbolCompletionError::Scheduler(FactQueryError::Runtime(error))
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::WorkerTerminated {
                        worker: Some(3),
                        item: Some(5),
                    }
                )
        ));
    }

    #[test]
    fn compiler_domain_infrastructure_failure_survives_completion_unchanged() {
        let completion = symbol_completion_error::<()>(BatchCompletionError::Scheduler(
            FactQueryError::InfrastructureFailure,
        ));

        assert_eq!(
            completion,
            SymbolCompletionError::Scheduler(FactQueryError::InfrastructureFailure)
        );
    }

    #[test]
    fn evaluator_panics_remain_distinct_from_runtime_worker_termination() {
        let graph = graph("module app; func main() {}");
        let package = AnySymbolId::from(graph.packages()[0].id());
        let cancellation = CancellationToken::new();

        let expected_request = graph
            .completion_plan(
                package,
                SymbolCompletionLevel::DeclarationSurface,
                &cancellation,
            )
            .unwrap_or_else(|error| panic!("test completion plan must be available: {error:?}"))
            .requests()[0];

        let evaluator = |_request: SymbolCompletionQuery| -> Result<DiagnosticBag, ()> {
            panic!("test evaluator panic");
        };

        let result = complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(worker_budget(2)),
            &cancellation,
            &evaluator,
        );

        assert!(matches!(
            result,
            Err(SymbolCompletionError::EvaluatorPanic { request })
                if request == expected_request
        ));
    }

    #[test]
    fn completion_planning_preserves_shared_cancellation_failures() {
        let graph = graph("module app; func main() {}");
        let package = AnySymbolId::from(graph.packages()[0].id());
        let first = SharedCancellation::new();
        let second = SharedCancellation::new();

        let _first_interest = first
            .register(second.token())
            .unwrap_or_else(|error| panic!("first shared interest must register: {error:?}"));

        let _second_interest = second
            .register(first.token())
            .unwrap_or_else(|error| panic!("second shared interest must register: {error:?}"));

        let evaluator = |_request: SymbolCompletionQuery| Ok::<_, ()>(DiagnosticBag::new());

        let result = complete_symbol(
            &graph,
            package,
            SymbolCompletionLevel::DeclarationSurface,
            &FactRuntime::new(worker_budget(2)),
            first.token(),
            &evaluator,
        );

        assert!(matches!(
            result,
            Err(SymbolCompletionError::Scheduler(FactQueryError::Runtime(error)))
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::RecursiveCancellationInterest
                )
        ));
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum CompletionEvaluationError {
        Coordination,
        Request(SymbolCompletionQuery),
    }

    struct LaterErrorFirstEvaluator {
        first_request: SymbolCompletionQuery,
        later_request: SymbolCompletionQuery,
        later_completed: Mutex<bool>,
        later_changed: Condvar,
    }

    impl LaterErrorFirstEvaluator {
        fn new(first_request: SymbolCompletionQuery, later_request: SymbolCompletionQuery) -> Self {
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

    impl bray_symbols::SymbolCompletionEvaluator for LaterErrorFirstEvaluator {
        type Error = CompletionEvaluationError;

        fn evaluate(&self, request: SymbolCompletionQuery) -> Result<DiagnosticBag, Self::Error> {
            if request == self.first_request {
                let mut later_completed = self
                    .later_completed
                    .lock()
                    .map_err(|_| CompletionEvaluationError::Coordination)?;

                while !*later_completed {
                    later_completed = self
                        .later_changed
                        .wait(later_completed)
                        .map_err(|_| CompletionEvaluationError::Coordination)?;
                }

                return Err(CompletionEvaluationError::Request(request));
            }

            if request == self.later_request {
                let mut later_completed = self
                    .later_completed
                    .lock()
                    .map_err(|_| CompletionEvaluationError::Coordination)?;

                *later_completed = true;

                self.later_changed.notify_all();

                return Err(CompletionEvaluationError::Request(request));
            }

            Ok(DiagnosticBag::new())
        }
    }

    fn complete<F>(
        graph: &SymbolGraph,
        root: AnySymbolId,
        workers: WorkerBudget,
        evaluator: &F,
    ) -> DiagnosticBag
    where
        F: bray_symbols::SymbolCompletionEvaluator,
        F::Error: std::fmt::Debug + Send,
    {
        let runtime = FactRuntime::new(workers);

        match complete_symbol(
            graph,
            root,
            SymbolCompletionLevel::DeclarationSurface,
            &runtime,
            &CancellationToken::new(),
            evaluator,
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
