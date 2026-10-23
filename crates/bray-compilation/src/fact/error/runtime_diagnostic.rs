use bray_diagnostics::{
    DiagnosticFactRuntimeFailure, DiagnosticFailureField, DiagnosticIoErrorKind,
};

use super::diagnostic_context::{
    count_field, identity_field, natural_field, signed_field, text_field,
};

use crate::fact::{CompilationFactKey, FactRuntimeError, FactRuntimeFailure};

pub(crate) fn diagnostic_fact_runtime_failure(
    error: &FactRuntimeError,
) -> DiagnosticFactRuntimeFailure {
    use FactRuntimeFailure as Failure;

    let (reason, context) = match error.cause() {
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
        Failure::UnitSourceCapacityExhausted { source_count } => (
            "unit_source_capacity_exhausted",
            vec![natural_field("source_count", *source_count)],
        ),
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

fn task_field(name: &'static str, value: crate::fact::FactTaskIdentity) -> DiagnosticFailureField {
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

pub(super) fn compilation_fact_kind(value: &CompilationFactKey) -> &'static str {
    use CompilationFactKey as Value;

    match value {
        Value::TargetValidity(_) => "target_validity",
        Value::ModuleContributionGate(_) => "module_contribution_gate",
        Value::CallableTypeDirectives(_) => "callable_type_directives",
        Value::BoundUnit(_) => "bound_unit",
        Value::CheckDiagnostics => "check_diagnostics",
        Value::DeclaredUnits => "declared_units",
        Value::ConstantInstance(_) => "constant_instance",
        Value::ConstantCall(_) => "constant_call",
        Value::ConstantCallCycle(_) => "constant_call_cycle",
        Value::CheckedControlFlow(_) => "checked_control_flow",
        Value::CheckedPatterns(_) => "checked_patterns",
        Value::StoragePlan(_) => "storage_plan",
        Value::MemoryOperations(_) => "memory_operations",
        Value::UnwindCandidate(_) => "unwind_candidate",
        Value::UnwindBehavior(_) => "unwind_behavior",
        Value::ExecutionCandidates(_) => "execution_candidates",
        Value::ConstantBodyTemplate(_) => "constant_body_template",
        Value::CertifiedExecution(_) => "certified_execution",
        Value::CleanupCompletion(_) => "cleanup_completion",
        Value::SelectedBodyBehavior(_) => "selected_body_behavior",
        Value::PreparedBodyAnalysis(_) => "prepared_body_analysis",
        Value::ResultDependencyComponent(_) => "result_dependency_component",
        Value::BodySemantics(_) => "body_semantics",
        Value::CheckedBodyBehavior(_) => "checked_body_behavior",
        Value::SourceOutputUnit(_) => "source_output_unit",
        Value::ExecutionGuaranteeDiagnostics => "execution_guarantee_diagnostics",
        Value::ImportedStandardLibraryImplementations => {
            "imported_standard_library_implementations"
        }
        Value::LoweredUnit(_) => "lowered_unit",
        Value::CodegenArtifact(_) => "codegen_artifact",
        Value::OptimizedMir(_) => "optimized_mir",
        Value::GeneratedLifecycle(..) => "generated_lifecycle",
        Value::DestructionBody(..) => "destruction_body",
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
    use crate::fact::{FactRuntimeError, FactRuntimeFailure, HostIoFailure, WorkerPoolKind};

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
}
