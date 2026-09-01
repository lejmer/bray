use bray_diagnostics::{
    DiagnosticFactRuntimeFailure, DiagnosticFailureField, DiagnosticFailureValue,
};

use crate::fact::{FactRuntimeError, FactRuntimeFailure};

pub(crate) fn diagnostic_fact_runtime_failure(
    error: &FactRuntimeError,
) -> DiagnosticFactRuntimeFailure {
    use FactRuntimeFailure as Failure;

    let (reason, context) = match error.cause() {
        Failure::SynchronizationPoisoned {
            component,
            fact,
            task,
        } => (
            "synchronization_poisoned",
            optional_task(
                optional_debug(vec![debug_field("component", component)], "fact", fact.as_ref()),
                "task",
                *task,
            ),
        ),
        Failure::SchedulerResultStatePoisoned { item } => (
            "scheduler_result_state_poisoned",
            optional_count(Vec::new(), "item", *item),
        ),
        Failure::UnitIdentityStatePoisoned { unit } => (
            "unit_identity_state_poisoned",
            vec![debug_field("unit", unit)],
        ),
        Failure::CapacityExhausted {
            resource,
            fact,
            task,
        } => (
            "capacity_exhausted",
            optional_task(
                optional_debug(vec![debug_field("resource", resource)], "fact", fact.as_ref()),
                "task",
                *task,
            ),
        ),
        Failure::WorkerPoolCreation {
            pool,
            workers,
            host,
        } => {
            let mut context = vec![
                debug_field("pool", pool),
                count_field("workers", *workers),
            ];

            if let Some(host) = host {
                context.push(text_field("host_error", host.to_string()));
            }

            ("worker_pool_creation", context)
        }
        Failure::WorkerTerminated { worker, item } => (
            "worker_terminated",
            optional_count(optional_count(Vec::new(), "worker", *worker), "item", *item),
        ),
        Failure::InvalidTaskState {
            operation,
            expected,
            actual,
            task,
            fact,
        } => (
            "invalid_task_state",
            vec![
                debug_field("operation", operation),
                debug_field("expected_phase", expected),
                debug_field("actual_phase", actual),
                task_field("task", *task),
                debug_field("fact", fact),
            ],
        ),
        Failure::InvalidSchedulerState {
            counter,
            expected_minimum,
            actual,
        } => (
            "invalid_scheduler_state",
            vec![
                debug_field("counter", counter),
                count_field("expected_minimum", *expected_minimum),
                count_field("actual", *actual),
            ],
        ),
        Failure::InvalidTaskContext {
            expected_runtime,
            actual,
        } => (
            "invalid_task_context",
            vec![
                count_field("expected_runtime", expected_runtime.0),
                count_field("actual_runtime", actual.runtime.0),
                task_field("actual_task", actual.task),
                debug_field("actual_fact", &actual.fact),
            ],
        ),
        Failure::TaskLocalStateUnavailable { operation, cause } => (
            "task_local_state_unavailable",
            vec![
                debug_field("operation", operation),
                debug_field("cause", cause),
            ],
        ),
        Failure::SchedulerLocalStateUnavailable { operation, cause } => (
            "scheduler_local_state_unavailable",
            vec![
                debug_field("operation", operation),
                debug_field("cause", cause),
            ],
        ),
        Failure::MissingInputFingerprint { input, task, fact } => (
            "missing_input_fingerprint",
            vec![
                debug_field("input", input),
                task_field("task", *task),
                debug_field("fact", fact),
            ],
        ),
        Failure::InputFingerprintMismatch {
            input,
            expected,
            actual,
            task,
            fact,
        } => (
            "input_fingerprint_mismatch",
            vec![
                debug_field("input", input),
                debug_field("expected_fingerprint", expected),
                debug_field("actual_fingerprint", actual),
                task_field("task", *task),
                debug_field("fact", fact),
            ],
        ),
        Failure::PublicationMismatch { requested, actual } => (
            "publication_mismatch",
            vec![
                debug_field("requested", requested),
                debug_field("actual", actual),
            ],
        ),
        Failure::AbandonedComputation { task, fact } => (
            "abandoned_computation",
            vec![task_field("task", *task), debug_field("fact", fact)],
        ),
        Failure::MissingDependencyRecord {
            task,
            fact,
            dependency,
        } => (
            "missing_dependency_record",
            vec![
                task_field("task", *task),
                debug_field("fact", fact),
                debug_field("dependency", dependency),
            ],
        ),
        Failure::InvalidWaitGraph {
            requester,
            owner,
            missing_predecessor,
            requested,
        } => (
            "invalid_wait_graph",
            vec![
                task_field("requester", *requester),
                task_field("owner", *owner),
                task_field("missing_predecessor", *missing_predecessor),
                debug_field("requested", requested),
            ],
        ),
        Failure::MissingCycle {
            runtime,
            fact,
            active,
        } => (
            "missing_cycle",
            vec![
                count_field("runtime", runtime.0),
                debug_field("fact", fact),
                text_list_field("active", active.iter().map(debug_text)),
            ],
        ),
        Failure::InvalidCancellationState { expected, actual } => (
            "invalid_cancellation_state",
            vec![
                debug_field("expected", expected),
                debug_field("actual", actual),
            ],
        ),
        Failure::RecursiveCancellationInterest => {
            ("recursive_cancellation_interest", Vec::new())
        }
        Failure::InvalidFrozenFact { fact } => {
            ("invalid_frozen_fact", vec![debug_field("fact", fact)])
        }
        Failure::InvalidUnitQueryKey { fact, unit } => (
            "invalid_unit_query_key",
            vec![debug_field("fact", fact), debug_field("unit", unit)],
        ),
        Failure::UnitSourceCapacityExhausted { source_count } => (
            "unit_source_capacity_exhausted",
            vec![count_field("source_count", *source_count)],
        ),
        Failure::UnknownUnitSource { unit } => {
            ("unknown_unit_source", vec![debug_field("unit", unit)])
        }
        Failure::UnitIdentityCapacityExhausted {
            unit,
            source_ordinal,
        } => (
            "unit_identity_capacity_exhausted",
            vec![
                debug_field("unit", unit),
                count_u64_field("source_ordinal", u64::from(*source_ordinal)),
            ],
        ),
        Failure::UnitIdentityCollision {
            identity,
            expected,
            actual,
        } => (
            "unit_identity_collision",
            vec![
                debug_field("identity", identity),
                debug_field("expected", expected),
                debug_field("actual", actual),
            ],
        ),
    };

    DiagnosticFactRuntimeFailure::new(reason, context)
}

fn debug_field(name: &'static str, value: &impl std::fmt::Debug) -> DiagnosticFailureField {
    text_field(name, debug_text(value))
}

fn debug_text(value: &impl std::fmt::Debug) -> String {
    format!("{value:?}")
}

fn text_field(name: &'static str, value: impl Into<String>) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Text(value.into()))
}

fn text_list_field(
    name: &'static str,
    values: impl IntoIterator<Item = String>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(
        name,
        DiagnosticFailureValue::TextList(values.into_iter().collect()),
    )
}

fn count_field(name: &'static str, value: usize) -> DiagnosticFailureField {
    text_field(name, value.to_string())
}

fn count_u64_field(name: &'static str, value: u64) -> DiagnosticFailureField {
    DiagnosticFailureField::new(name, DiagnosticFailureValue::Count(value))
}

fn task_field(
    name: &'static str,
    value: crate::fact::FactTaskIdentity,
) -> DiagnosticFailureField {
    count_u64_field(name, value.0)
}

fn optional_count(
    mut context: Vec<DiagnosticFailureField>,
    name: &'static str,
    value: Option<usize>,
) -> Vec<DiagnosticFailureField> {
    if let Some(value) = value {
        context.push(count_field(name, value));
    }

    context
}

fn optional_task(
    mut context: Vec<DiagnosticFailureField>,
    name: &'static str,
    value: Option<crate::fact::FactTaskIdentity>,
) -> Vec<DiagnosticFailureField> {
    if let Some(value) = value {
        context.push(task_field(name, value));
    }

    context
}

fn optional_debug(
    mut context: Vec<DiagnosticFailureField>,
    name: &'static str,
    value: Option<&impl std::fmt::Debug>,
) -> Vec<DiagnosticFailureField> {
    if let Some(value) = value {
        context.push(debug_field(name, value));
    }

    context
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;

    use super::diagnostic_fact_runtime_failure;
    use crate::fact::{
        CompilationFactKey, FactRuntimeError, FactRuntimeFailure, FactTaskIdentity,
        PublicationIdentity, PublicationState,
    };

    #[test]
    fn runtime_diagnostics_preserve_leaf_reason_and_context() {
        let error = FactRuntimeError::from(FactRuntimeFailure::PublicationMismatch {
            requested: PublicationIdentity {
                task: Some(FactTaskIdentity(17)),
                fact: CompilationFactKey::SyntaxTree,
            },
            actual: PublicationState::Computing {
                task: FactTaskIdentity(23),
                fact: CompilationFactKey::SymbolGraph,
            },
        });

        let diagnostic = diagnostic_fact_runtime_failure(&error);

        assert_eq!(diagnostic.reason(), "publication_mismatch");
        assert_eq!(diagnostic.context().len(), 2);
        assert_eq!(diagnostic.context()[0].name(), "requested");
        assert_eq!(diagnostic.context()[1].name(), "actual");

        let DiagnosticFailureValue::Text(requested) = diagnostic.context()[0].value() else {
            panic!("publication identity must remain structured diagnostic text")
        };

        let DiagnosticFailureValue::Text(actual) = diagnostic.context()[1].value() else {
            panic!("publication state must remain structured diagnostic text")
        };

        assert!(requested.contains("SyntaxTree"));
        assert!(requested.contains("17"));
        assert!(actual.contains("SymbolGraph"));
        assert!(actual.contains("23"));
    }
}
