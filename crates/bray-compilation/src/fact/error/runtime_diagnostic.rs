use std::hash::Hash;

use bray_diagnostics::{
    DiagnosticFactRuntimeFailure, DiagnosticFailureField, DiagnosticFailureValue,
    DiagnosticIoErrorKind,
};

use super::diagnostic_context::{
    count_field, identity, identity_field, natural_field, signed_field, text_field,
};

use crate::fact::{
    CompilationFactKey, CompilationInputKey, FactRuntimeError, FactRuntimeFailure,
};

pub(crate) fn diagnostic_fact_runtime_failure(
    error: &FactRuntimeError,
) -> DiagnosticFactRuntimeFailure {
    use FactRuntimeFailure as Failure;

    let (reason, context) = match error.cause() {
        Failure::SynchronizationPoisoned {
            component,
            fact,
            task,
        } => {
            let mut context = vec![text_field(
                "component",
                synchronization_component_key(*component),
            )];

            push_optional_fact(&mut context, "fact_kind", "fact_identity", fact.as_ref());
            push_optional_task(&mut context, "task", *task);

            ("synchronization_poisoned", context)
        }
        Failure::SchedulerResultStatePoisoned { item } => {
            let mut context = Vec::new();
            push_optional_natural(&mut context, "item", *item);

            ("scheduler_result_state_poisoned", context)
        }
        Failure::UnitIdentityStatePoisoned { unit } => (
            "unit_identity_state_poisoned",
            vec![identity_field("unit", unit)],
        ),
        Failure::CapacityExhausted {
            resource,
            fact,
            task,
        } => {
            let mut context = vec![text_field("resource", capacity_resource_key(*resource))];
            push_optional_fact(&mut context, "fact_kind", "fact_identity", fact.as_ref());
            push_optional_task(&mut context, "task", *task);

            ("capacity_exhausted", context)
        }
        Failure::WorkerPoolCreation {
            pool,
            workers,
            host,
        } => worker_pool_creation_context(*pool, *workers, host.as_ref()),
        Failure::WorkerTerminated { worker, item } => {
            let mut context = Vec::new();
            push_optional_natural(&mut context, "worker", *worker);
            push_optional_natural(&mut context, "item", *item);

            ("worker_terminated", context)
        }
        Failure::InvalidTaskState {
            operation,
            expected,
            actual,
            task,
            fact,
        } => {
            let mut context = vec![
                text_field("operation", task_operation_key(*operation)),
                text_field("expected_phase", task_phase_key(*expected)),
                text_field("actual_phase", task_phase_key(*actual)),
                task_field("task", *task),
            ];

            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            ("invalid_task_state", context)
        }
        Failure::InvalidSchedulerState {
            counter,
            expected_minimum,
            actual,
        } => (
            "invalid_scheduler_state",
            vec![
                text_field("counter", scheduler_counter_key(*counter)),
                natural_field("expected_minimum", *expected_minimum),
                natural_field("actual", *actual),
            ],
        ),
        Failure::InvalidTaskContext {
            expected_runtime,
            actual,
        } => invalid_task_context(expected_runtime.0, actual),
        Failure::TaskLocalStateUnavailable { operation, cause } => (
            "task_local_state_unavailable",
            vec![
                text_field("operation", task_local_operation_key(*operation)),
                text_field("cause", local_state_failure_key(*cause)),
            ],
        ),
        Failure::SchedulerLocalStateUnavailable { operation, cause } => (
            "scheduler_local_state_unavailable",
            vec![
                text_field("operation", scheduler_local_operation_key(*operation)),
                text_field("cause", local_state_failure_key(*cause)),
            ],
        ),
        Failure::MissingInputFingerprint { input, task, fact } => {
            let mut context = vec![
                text_field("input_kind", compilation_input_kind(input)),
                identity_field("input_identity", input),
                task_field("task", *task),
            ];

            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            ("missing_input_fingerprint", context)
        }
        Failure::InputFingerprintMismatch {
            input,
            expected,
            actual,
            task,
            fact,
        } => {
            let mut context = vec![
                text_field("input_kind", compilation_input_kind(input)),
                identity_field("input_identity", input),
                identity_field("expected_fingerprint", expected),
                identity_field("actual_fingerprint", actual),
                task_field("task", *task),
            ];

            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            ("input_fingerprint_mismatch", context)
        }
        Failure::PublicationMismatch { requested, actual } => {
            publication_mismatch_context(requested, actual)
        }
        Failure::AbandonedComputation { task, fact } => {
            let mut context = vec![task_field("task", *task)];
            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            ("abandoned_computation", context)
        }
        Failure::MissingDependencyRecord {
            task,
            fact,
            dependency,
        } => {
            let mut context = vec![task_field("task", *task)];
            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            push_fact(
                &mut context,
                "dependency_kind",
                "dependency_identity",
                dependency,
            );

            ("missing_dependency_record", context)
        }
        Failure::InvalidWaitGraph {
            requester,
            owner,
            missing_predecessor,
            requested,
        } => {
            let mut context = vec![
                task_field("requester", *requester),
                task_field("owner", *owner),
                task_field("missing_predecessor", *missing_predecessor),
            ];

            push_fact(
                &mut context,
                "requested_fact_kind",
                "requested_fact_identity",
                requested,
            );

            ("invalid_wait_graph", context)
        }
        Failure::MissingCycle {
            runtime,
            fact,
            active,
        } => missing_cycle_context(runtime.0, fact, active),
        Failure::InvalidCancellationState { expected, actual } => (
            "invalid_cancellation_state",
            vec![
                text_field("expected", cancellation_state_key(*expected)),
                text_field("actual", cancellation_state_key(*actual)),
            ],
        ),
        Failure::RecursiveCancellationInterest => {
            ("recursive_cancellation_interest", Vec::new())
        }
        Failure::InvalidFrozenFact { fact } => {
            let mut context = Vec::new();
            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            ("invalid_frozen_fact", context)
        }
        Failure::InvalidUnitQueryKey { fact, unit } => {
            let mut context = vec![identity_field("unit", unit)];
            push_fact(&mut context, "fact_kind", "fact_identity", fact);

            ("invalid_unit_query_key", context)
        }
        Failure::UnitSourceCapacityExhausted { source_count } => (
            "unit_source_capacity_exhausted",
            vec![natural_field("source_count", *source_count)],
        ),
        Failure::UnknownUnitSource { unit } => {
            ("unknown_unit_source", vec![identity_field("unit", unit)])
        }
        Failure::UnitIdentityCapacityExhausted {
            unit,
            source_ordinal,
        } => (
            "unit_identity_capacity_exhausted",
            vec![
                identity_field("unit", unit),
                count_field("source_ordinal", u64::from(*source_ordinal)),
            ],
        ),
        Failure::UnitIdentityCollision {
            identity,
            expected,
            actual,
        } => (
            "unit_identity_collision",
            vec![
                identity_field("identity", identity),
                identity_field("expected", expected),
                identity_field("actual", actual),
            ],
        ),
    };

    DiagnosticFactRuntimeFailure::new(reason, context)
}

fn worker_pool_creation_context(
    pool: crate::fact::WorkerPoolKind,
    workers: usize,
    host: Option<&crate::fact::HostIoFailure>,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    let mut context = vec![
        text_field("pool", worker_pool_key(pool)),
        natural_field("workers", workers),
    ];

    if let Some(host) = host {
        context.push(text_field(
            "host_error_kind",
            DiagnosticIoErrorKind::from(host.kind()).as_str(),
        ));

        context.push(text_field("host_error_message", host.message()));

        if let Some(raw_os_error) = host.raw_os_error() {
            context.push(signed_field("host_raw_os_error", i64::from(raw_os_error)));
        }
    }

    ("worker_pool_creation", context)
}

fn invalid_task_context(
    expected_runtime: usize,
    actual: &crate::fact::TaskContextIdentity,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    let mut context = vec![
        natural_field("expected_runtime", expected_runtime),
        natural_field("actual_runtime", actual.runtime.0),
        task_field("actual_task", actual.task),
    ];

    push_fact(
        &mut context,
        "actual_fact_kind",
        "actual_fact_identity",
        &actual.fact,
    );

    ("invalid_task_context", context)
}

fn publication_mismatch_context(
    requested: &crate::fact::PublicationIdentity,
    actual: &crate::fact::PublicationState,
) -> (&'static str, Vec<DiagnosticFailureField>) {
    let mut context = Vec::new();
    push_optional_task(&mut context, "requested_task", requested.task);

    push_fact(
        &mut context,
        "requested_fact_kind",
        "requested_fact_identity",
        &requested.fact,
    );

    push_publication_state(&mut context, actual);

    ("publication_mismatch", context)
}

fn missing_cycle_context(
    runtime: usize,
    fact: &CompilationFactKey,
    active: &[CompilationFactKey],
) -> (&'static str, Vec<DiagnosticFailureField>) {
    let mut context = vec![natural_field("runtime", runtime)];
    push_fact(&mut context, "fact_kind", "fact_identity", fact);

    context.push(text_list_field(
        "active_fact_kinds",
        active.iter().map(compilation_fact_kind),
    ));

    context.push(identity_list_field(
        "active_fact_identities",
        active.iter(),
    ));

    ("missing_cycle", context)
}

fn push_publication_state(
    context: &mut Vec<DiagnosticFailureField>,
    state: &crate::fact::PublicationState,
) {
    use crate::fact::PublicationState as State;

    match state {
        State::Vacant => context.push(text_field("actual_state", "vacant")),
        State::Computing { task, fact } => {
            context.push(text_field("actual_state", "computing"));
            context.push(task_field("actual_task", *task));

            push_fact(
                context,
                "actual_fact_kind",
                "actual_fact_identity",
                fact,
            );
        }
        State::Ready { fact } => {
            context.push(text_field("actual_state", "ready"));

            push_optional_fact(
                context,
                "actual_fact_kind",
                "actual_fact_identity",
                fact.as_ref(),
            );
        }
        State::PublishedFlagWithoutValue => {
            context.push(text_field("actual_state", "published_flag_without_value"));
        }
    }
}

fn push_fact(
    context: &mut Vec<DiagnosticFailureField>,
    kind_name: &'static str,
    identity_name: &'static str,
    fact: &CompilationFactKey,
) {
    context.push(text_field(kind_name, compilation_fact_kind(fact)));
    context.push(identity_field(identity_name, fact));
}

fn push_optional_fact(
    context: &mut Vec<DiagnosticFailureField>,
    kind_name: &'static str,
    identity_name: &'static str,
    fact: Option<&CompilationFactKey>,
) {
    if let Some(fact) = fact {
        push_fact(context, kind_name, identity_name, fact);
    }
}

fn text_list_field(
    name: &'static str,
    values: impl IntoIterator<Item = &'static str>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(
        name,
        DiagnosticFailureValue::TextList(
            values
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
    )
}

fn identity_list_field<'a, T: Hash + 'a>(
    name: &'static str,
    values: impl IntoIterator<Item = &'a T>,
) -> DiagnosticFailureField {
    DiagnosticFailureField::new(
        name,
        DiagnosticFailureValue::IdentityList(
            values
                .into_iter()
                .map(identity)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        ),
    )
}

fn task_field(
    name: &'static str,
    value: crate::fact::FactTaskIdentity,
) -> DiagnosticFailureField {
    count_field(name, value.0)
}

fn push_optional_task(
    context: &mut Vec<DiagnosticFailureField>,
    name: &'static str,
    value: Option<crate::fact::FactTaskIdentity>,
) {
    if let Some(value) = value {
        context.push(task_field(name, value));
    }
}

fn push_optional_natural(
    context: &mut Vec<DiagnosticFailureField>,
    name: &'static str,
    value: Option<usize>,
) {
    if let Some(value) = value {
        context.push(natural_field(name, value));
    }
}

fn synchronization_component_key(
    value: crate::fact::SynchronizationComponent,
) -> &'static str {
    use crate::fact::SynchronizationComponent as Value;

    match value {
        Value::CancellationInterests => "cancellation_interests",
        Value::CellMap => "cell_map",
        Value::EmbeddedConstantExpectations => "embedded_constant_expectations",
        Value::FactCell => "fact_cell",
        Value::RuntimeDependencies => "runtime_dependencies",
        Value::SchedulerSlots => "scheduler_slots",
        Value::TaskDependencies => "task_dependencies",
    }
}

fn capacity_resource_key(value: crate::fact::CapacityResource) -> &'static str {
    use crate::fact::CapacityResource as Value;

    match value {
        Value::CancellationInterestIdentity => "cancellation_interest_identity",
        Value::CellMapAccessIdentity => "cell_map_access_identity",
        Value::SchedulerInteractiveStreak => "scheduler_interactive_streak",
        Value::SchedulerInteractiveWaiters => "scheduler_interactive_waiters",
        Value::SchedulerOrdinaryWaiters => "scheduler_ordinary_waiters",
        Value::SchedulerWaitRegistrations => "scheduler_wait_registrations",
        Value::SchedulerActiveSlots => "scheduler_active_slots",
        Value::TaskIdentity => "task_identity",
    }
}

fn worker_pool_key(value: crate::fact::WorkerPoolKind) -> &'static str {
    match value {
        crate::fact::WorkerPoolKind::Interactive => "interactive",
        crate::fact::WorkerPoolKind::Ordinary => "ordinary",
    }
}

fn task_phase_key(value: crate::fact::FactTaskPhase) -> &'static str {
    match value {
        crate::fact::FactTaskPhase::Recording => "recording",
        crate::fact::FactTaskPhase::Finished => "finished",
        crate::fact::FactTaskPhase::Discarded => "discarded",
    }
}

fn task_operation_key(value: crate::fact::TaskOperation) -> &'static str {
    use crate::fact::TaskOperation as Value;

    match value {
        Value::Finish => "finish",
        Value::RecordFact => "record_fact",
        Value::RecordFixedInput => "record_fixed_input",
        Value::RecordFrozenFact => "record_frozen_fact",
        Value::RecordInput => "record_input",
    }
}

fn task_local_operation_key(value: crate::fact::TaskLocalOperation) -> &'static str {
    use crate::fact::TaskLocalOperation as Value;

    match value {
        Value::Capture => "capture",
        Value::Current => "current",
        Value::Cycle => "cycle",
        Value::Enter => "enter",
        Value::Replace => "replace",
    }
}

fn scheduler_local_operation_key(value: crate::fact::SchedulerLocalOperation) -> &'static str {
    use crate::fact::SchedulerLocalOperation as Value;

    match value {
        Value::CurrentPriority => "current_priority",
        Value::Enter => "enter",
        Value::Inspect => "inspect",
    }
}

fn scheduler_counter_key(value: crate::fact::SchedulerCounter) -> &'static str {
    match value {
        crate::fact::SchedulerCounter::InteractiveWaiters => "interactive_waiters",
        crate::fact::SchedulerCounter::OrdinaryWaiters => "ordinary_waiters",
    }
}

fn local_state_failure_key(value: crate::fact::LocalStateFailure) -> &'static str {
    match value {
        crate::fact::LocalStateFailure::BorrowConflict => "borrow_conflict",
        crate::fact::LocalStateFailure::Unavailable => "unavailable",
    }
}

fn cancellation_state_key(value: crate::fact::CancellationStateKind) -> &'static str {
    match value {
        crate::fact::CancellationStateKind::Request => "request",
        crate::fact::CancellationStateKind::Shared => "shared",
    }
}

fn compilation_input_kind(value: &CompilationInputKey) -> &'static str {
    use CompilationInputKey as Value;

    match value {
        Value::PackageIdentity => "package_identity",
        Value::PackageSourceAuthority => "package_source_authority",
        Value::SourceSet => "source_set",
        Value::Source(_) => "source",
        Value::SourceDiagnostics => "source_diagnostics",
        Value::ProductKind => "product_kind",
        Value::SelectedTarget => "selected_target",
        Value::NativeLinkInputs => "native_link_inputs",
        Value::SemanticRecursionLimit => "semantic_recursion_limit",
        Value::SemanticPairwiseLimit => "semantic_pairwise_limit",
        Value::DependencySet => "dependency_set",
        Value::DependencyInterface(_) => "dependency_interface",
        Value::DependencyImplementation(_) => "dependency_implementation",
        Value::PlatformServices => "platform_services",
        Value::RuntimeRoles => "runtime_roles",
        Value::PackageInterfaceExport => "package_interface_export",
        Value::CodegenConfiguration => "codegen_configuration",
        Value::StandardLibrary => "standard_library",
        Value::StandardLibraryProviders => "standard_library_providers",
    }
}

pub(super) fn compilation_fact_kind(value: &CompilationFactKey) -> &'static str {
    use CompilationFactKey as Value;

    match value {
        Value::TargetValidity(_) => "target_validity",
        Value::ModuleContributionGate(_) => "module_contribution_gate",
        Value::CallableTypeDirectives(_) => "callable_type_directives",
        Value::BoundUnit(_) => "bound_unit",
        Value::CheckDiagnostics => "check_diagnostics",
        Value::ConstantTemplateKeys => "constant_template_keys",
        Value::CallableBodyKeys => "callable_body_keys",
        Value::PredicateDefinitionKeys => "predicate_definition_keys",
        Value::ConstantInstance(_) => "constant_instance",
        Value::ConstantCall(_) => "constant_call",
        Value::ConstantCallCycle(_) => "constant_call_cycle",
        Value::CheckedControlFlow(_) => "checked_control_flow",
        Value::CheckedPatterns(_) => "checked_patterns",
        Value::StoragePlan(_) => "storage_plan",
        Value::MemoryOperations(_) => "memory_operations",
        Value::BodySemantics(_) => "body_semantics",
        Value::CheckedBodyBehavior(_) => "checked_body_behavior",
        Value::LoweredUnit(_) => "lowered_unit",
        Value::CodegenArtifact(_) => "codegen_artifact",
        Value::NativeProduct(_) => "native_product",
        Value::DeclaredValueTypeTemplates(_) => "declared_value_type_templates",
        Value::ExpressionSemantics(_) => "expression_semantics",
        Value::ProvisionalExpressionSemantics(_) => "provisional_expression_semantics",
        Value::SymbolicConstantTerm(_) => "symbolic_constant_term",
        Value::DeclarationChunk(_) => "declaration_chunk",
        Value::DeclarationTable => "declaration_table",
        Value::ProductSourceGraph => "product_source_graph",
        Value::ProductSemantics => "product_semantics",
        Value::TestDiscovery(_) => "test_discovery",
        Value::DiscoverySymbolGraph => "discovery_symbol_graph",
        Value::DependencyInterface(_) => "dependency_interface",
        Value::DependencyImplementation(_) => "dependency_implementation",
        Value::ImportedDiagnostics => "imported_diagnostics",
        Value::ImplementationHeaderIndex => "implementation_header_index",
        Value::ImplementationCandidateSet(_) => "implementation_candidate_set",
        Value::TraitImplementationConformance(_) => "trait_implementation_conformance",
        Value::GenericConstraintSatisfaction(_) => "generic_constraint_satisfaction",
        Value::ImplementationSelection(_) => "implementation_selection",
        Value::IterationSource(_) => "iteration_source",
        Value::OperationSelection(_) => "operation_selection",
        Value::ImportedSemanticGraph(_) => "imported_semantic_graph",
        Value::ImportedSemanticRecord(_) => "imported_semantic_record",
        Value::ImportedConstantCallableBody(_) => "imported_constant_callable_body",
        Value::ImportedExecutableTemplate(_) => "imported_executable_template",
        Value::ImplementationParticipation(_) => "implementation_participation",
        Value::ImplementationCoherence => "implementation_coherence",
        Value::CallableOverloadValidation => "callable_overload_validation",
        Value::ForeignCallableContract(_) => "foreign_callable_contract",
        Value::ForeignStaticContract(_) => "foreign_static_contract",
        Value::ForeignCallableValidation => "foreign_callable_validation",
        Value::TypeAssociatedSurface(_) => "type_associated_surface",
        Value::DeclaredTypeRepresentation(_) => "declared_type_representation",
        Value::TypeAssociatedImplementationIndex => "type_associated_implementation_index",
        Value::ImportedSymbolSkeleton => "imported_symbol_skeleton",
        Value::PackageInterfaceExportBundle => "package_interface_export_bundle",
        Value::SemanticDiagnostics => "semantic_diagnostics",
        Value::SourceUnitSyntax(_) => "source_unit_syntax",
        Value::SourceReferenceIndex(_) => "source_reference_index",
        Value::SymbolGraph => "symbol_graph",
        Value::Symbol(_) => "symbol",
        Value::SyntaxTree => "syntax_tree",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;

    use super::diagnostic_fact_runtime_failure;
    use crate::fact::{
        CompilationFactKey, FactRuntimeError, FactRuntimeFailure, FactTaskIdentity, HostIoFailure,
        PublicationIdentity, PublicationState, WorkerPoolKind,
    };

    #[test]
    fn runtime_diagnostics_preserve_structured_publication_identities() {
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
        assert_eq!(field_text(&diagnostic, "requested_fact_kind"), "syntax_tree");
        assert_eq!(field_count(&diagnostic, "requested_task"), 17);
        assert_eq!(field_text(&diagnostic, "actual_state"), "computing");
        assert_eq!(field_text(&diagnostic, "actual_fact_kind"), "symbol_graph");
        assert_eq!(field_count(&diagnostic, "actual_task"), 23);

        assert!(matches!(
            field(&diagnostic, "requested_fact_identity"),
            DiagnosticFailureValue::Identity(_)
        ));

        assert!(matches!(
            field(&diagnostic, "actual_fact_identity"),
            DiagnosticFailureValue::Identity(_)
        ));
    }

    #[test]
    fn worker_pool_diagnostics_preserve_exact_host_io_cause() {
        let host_error = std::io::Error::from_raw_os_error(5);

        let error = FactRuntimeError::from(FactRuntimeFailure::WorkerPoolCreation {
            pool: WorkerPoolKind::Ordinary,
            workers: 4,
            host: Some(HostIoFailure::from(&host_error)),
        });

        let diagnostic = diagnostic_fact_runtime_failure(&error);

        assert_eq!(diagnostic.reason(), "worker_pool_creation");

        assert_eq!(
            field_text(&diagnostic, "host_error_kind"),
            bray_diagnostics::DiagnosticIoErrorKind::from(host_error.kind()).as_str()
        );

        assert_eq!(
            field_text(&diagnostic, "host_error_message"),
            host_error.to_string()
        );

        assert_eq!(
            field(&diagnostic, "host_raw_os_error"),
            &DiagnosticFailureValue::Signed(5)
        );
    }

    fn field<'a>(
        diagnostic: &'a bray_diagnostics::DiagnosticFactRuntimeFailure,
        name: &str,
    ) -> &'a DiagnosticFailureValue {
        diagnostic
            .context()
            .iter()
            .find(|field| field.name() == name)
            .unwrap_or_else(|| panic!("missing diagnostic field {name}"))
            .value()
    }

    fn field_text<'a>(
        diagnostic: &'a bray_diagnostics::DiagnosticFactRuntimeFailure,
        name: &str,
    ) -> &'a str {
        let DiagnosticFailureValue::Text(value) = field(diagnostic, name) else {
            panic!("diagnostic field {name} must contain text")
        };

        value
    }

    fn field_count(
        diagnostic: &bray_diagnostics::DiagnosticFactRuntimeFailure,
        name: &str,
    ) -> u64 {
        let DiagnosticFailureValue::Count(value) = field(diagnostic, name) else {
            panic!("diagnostic field {name} must contain a count")
        };

        *value
    }
}
