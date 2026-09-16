use bray_diagnostics::{
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticProductQueryFailure,
};

use super::context::product_query_context;
use super::mir::{push_mir_call_target, push_mir_helper};
use crate::compilation::product::ProductSynchronizationComponent;
use crate::compilation::{
    ProductDataKind, ProductQueryContext, ProductQueryError, ProductQueryFailure, ProductValueKind,
};
use crate::fact::diagnostic_context::{
    constant_value_kind, count_field, identity, identity_field, natural_field, product_kind,
    push_semantic_type_data, push_symbol, text_field,
};

// rust-style: allow(function-too-large, reason = "product-query variants form one exhaustive flat conversion into typed diagnostic fields")
pub(in crate::compilation::product_emission::diagnostics) fn diagnostic_product_query_failure(
    error: &ProductQueryError,
) -> DiagnosticProductQueryFailure {
    use ProductQueryFailure as Failure;

    let (reason, context) = match error.cause() {
        Failure::Missing { context, data } => {
            ("product_query_missing", context_with_data(context, *data))
        }
        Failure::Conflict { context, data } => {
            ("product_query_conflict", context_with_data(context, *data))
        }
        Failure::UnexpectedKind {
            context,
            expected,
            actual,
        } => {
            let mut fields = product_query_context(context);

            fields.extend([
                text_field("expected_value_kind", product_value_kind(*expected)),
                text_field("actual_value_kind", product_value_kind(*actual)),
            ]);

            ("product_query_unexpected_kind", fields)
        }
        Failure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => {
            let mut fields = context_with_data(context, *data);

            fields.extend([
                natural_field("expected_count", *expected),
                natural_field("actual_count", *actual),
            ]);

            ("product_query_count_mismatch", fields)
        }
        Failure::UnsupportedConstantValue { value, kind } => (
            "product_query_unsupported_constant_value",
            vec![
                identity_field("constant_value", value),
                text_field("constant_value_kind", constant_value_kind(kind)),
            ],
        ),
        Failure::GenericSubstitution {
            substitution,
            cause,
        } => {
            let mut fields = vec![text_field(
                "substitution_cause",
                crate::fact::generic_substitution_reason(cause),
            )];

            crate::fact::push_generic_substitution_failure(&mut fields, cause);

            push_optional_identity(&mut fields, "substitution", substitution.as_ref());

            ("product_query_generic_substitution", fields)
        }
        Failure::TraitApplicationMismatch {
            witness,
            expected,
            actual,
        } => (
            "product_query_trait_application_mismatch",
            vec![
                identity_field("witness", witness),
                identity_field("expected_trait_application", expected),
                identity_field("actual_trait_application", actual),
            ],
        ),
        Failure::ConflictingImplementationWitness {
            identity,
            existing,
            actual,
        } => (
            "product_query_conflicting_implementation_witness",
            vec![
                identity_field("witness_identity", identity),
                identity_field("existing_implementation", existing),
                identity_field("actual_implementation", actual),
            ],
        ),
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => {
            let mut fields = vec![
                identity_field("semantic_type", ty),
                text_field("expected_value_kind", product_value_kind(*expected)),
            ];

            push_semantic_type_data(&mut fields, actual);

            ("product_query_unexpected_semantic_type", fields)
        }
        Failure::SynchronizationPoisoned { component } => (
            "product_query_synchronization_poisoned",
            vec![text_field(
                "synchronization_component",
                product_synchronization_component(*component),
            )],
        ),
        Failure::OutgoingCapacityOverflow { value } => (
            "product_query_outgoing_capacity_overflow",
            vec![identity_field("constant_value", value)],
        ),
        Failure::StaticDependencyOverflow { static_instance } => (
            "product_query_static_dependency_overflow",
            vec![identity_field("static_instance", static_instance)],
        ),
        Failure::StaticDependencyUnderflow { static_instance } => (
            "product_query_static_dependency_underflow",
            vec![identity_field("static_instance", static_instance)],
        ),
        Failure::StaticLifecycleCycle { instances } => (
            "product_query_static_lifecycle_cycle",
            vec![DiagnosticFailureField::new(
                "static_instances",
                DiagnosticFailureValue::IdentityList(
                    instances
                        .iter()
                        .map(crate::fact::diagnostic_context::identity)
                        .collect::<Vec<_>>()
                        .into_boxed_slice(),
                ),
            )],
        ),
        Failure::ConflictingConcreteInstance { key } => (
            "product_query_conflicting_concrete_instance",
            vec![identity_field("instance", key)],
        ),
        Failure::ConflictingStaticRelocation { value } => (
            "product_query_conflicting_static_relocation",
            vec![identity_field("constant_value", value)],
        ),
        Failure::CallableDefinitionMismatch {
            instance,
            expected,
            actual,
        } => (
            "product_query_callable_definition_mismatch",
            vec![
                identity_field("instance", instance),
                identity_field("expected_callable_definition", expected),
                identity_field("actual_callable_definition", actual),
            ],
        ),
        Failure::TraitDefinitionMismatch {
            context,
            expected,
            actual,
        } => {
            let mut fields = product_query_context(context);

            fields.extend([
                identity_field("expected_trait_definition", expected),
                identity_field("actual_trait_definition", actual),
            ]);

            ("product_query_trait_definition_mismatch", fields)
        }
        Failure::InvalidCodegenSourceFile { source, path } => {
            let mut fields = vec![text_field("path", path)];

            if let Some(source) = source {
                fields.push(count_field("source", u64::from(source.raw())));
            }

            ("product_query_invalid_codegen_source_file", fields)
        }
        Failure::SourceIndex { source, cause } => (
            "product_query_source_index",
            vec![
                count_field("source", u64::from(source.raw())),
                natural_field("source_byte_count", cause.bytes()),
            ],
        ),
        Failure::ExternalSymbolIdentity { symbol, cause } => {
            let mut fields = Vec::new();
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);
            super::export::push_package_interface_export_failure(&mut fields, cause);

            ("product_query_external_symbol_identity", fields)
        }
        Failure::InvalidCompilerKnownDeclarationKey { key } => (
            "product_query_invalid_compiler_known_declaration_key",
            vec![text_field("declaration_key", key)],
        ),
        Failure::InvalidRecognizedStandardLibraryDeclarationKey { key } => (
            "product_query_invalid_recognized_standard_library_declaration_key",
            vec![text_field("declaration_key", key)],
        ),
        Failure::InvalidPackageIdentity { identity } => (
            "product_query_invalid_package_identity",
            vec![text_field("package_identity", identity)],
        ),
        Failure::UnexpectedEntryResult { actual } => (
            "product_query_unexpected_entry_result",
            vec![text_field(
                "actual_entry_result",
                executable_entry_result(actual),
            )],
        ),
        Failure::CompilerKnownRepresentationMismatch {
            ty,
            expected,
            actual,
        } => {
            let mut fields = vec![
                identity_field("semantic_type", ty),
                text_field("expected_representation", expected.as_str()),
            ];

            if let Some(actual) = actual {
                fields.push(text_field("actual_representation", actual.as_str()));
            }

            (
                "product_query_compiler_known_representation_mismatch",
                fields,
            )
        }
        Failure::NativeBoundaryKindMismatch { reference, actual } => (
            "product_query_native_boundary_kind_mismatch",
            vec![
                identity_field("static_reference", reference),
                text_field("actual_boundary_kind", native_boundary_kind(actual)),
            ],
        ),
        Failure::UnexpectedSymbolKind {
            symbol,
            expected,
            actual,
        } => {
            let mut fields = Vec::new();
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);

            fields.extend([
                text_field("expected_symbol_kind", expected.as_str()),
                text_field("actual_symbol_kind", actual.as_str()),
            ]);

            ("product_query_unexpected_symbol_kind", fields)
        }
        Failure::ImplementationSymbolKeyExpected { key, actual } => (
            "product_query_implementation_symbol_key_expected",
            vec![
                identity_field("symbol_key", key),
                text_field("actual_symbol_kind", actual.as_str()),
            ],
        ),
        Failure::InvalidHelperOperation {
            context,
            helper,
            operation,
        } => {
            let mut fields = product_query_context(context);

            push_mir_helper(&mut fields, helper);
            fields.push(text_field("operation", mir_operation_kind(operation)));

            ("product_query_invalid_helper_operation", fields)
        }
        Failure::InvalidCleanupCallTarget { context, target } => {
            let mut fields = product_query_context(context);
            push_mir_call_target(&mut fields, target);

            ("product_query_invalid_cleanup_call_target", fields)
        }
        Failure::LifecycleRoleMismatch {
            instance,
            expected,
            actual,
        } => {
            let fields = vec![
                identity_field("instance", instance),
                text_field(
                    "expected_lifecycle_role",
                    expected.map_or("none", mir_lifecycle_role),
                ),
                text_field(
                    "actual_lifecycle_role",
                    actual.map_or("none", mir_lifecycle_role),
                ),
            ];

            ("product_query_lifecycle_role_mismatch", fields)
        }
        Failure::BuiltInProofMismatch {
            requirement,
            actual,
        } => {
            let fields = vec![
                identity_field("requirement", requirement),
                text_field("actual_proof", actual.map_or("none", proof_outcome)),
            ];

            ("product_query_built_in_proof_mismatch", fields)
        }
        Failure::ImplementationSelectionMismatch {
            requirement,
            actual,
        } => {
            let mut fields = vec![identity_field("requirement", requirement)];
            push_implementation_selection(&mut fields, actual);

            ("product_query_implementation_selection_mismatch", fields)
        }
        Failure::UnsupportedRuntimeDefaultSubject { provider, subject } => {
            let mut fields = Vec::new();
            push_symbol(&mut fields, "provider_kind", "provider", *provider);
            push_symbol(&mut fields, "subject_kind", "subject", *subject);

            ("product_query_unsupported_runtime_default_subject", fields)
        }
        Failure::InvalidCallableDefinitionSymbol { symbol, actual } => {
            let mut fields = Vec::new();
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);
            fields.push(text_field("actual_symbol_kind", actual.as_str()));

            ("product_query_invalid_callable_definition_symbol", fields)
        }
        Failure::SourceSnapshotMismatch {
            source,
            expected,
            actual,
        } => {
            let mut fields = vec![
                count_field("source", u64::from(source.raw())),
                count_field("expected_source_version", expected.raw()),
            ];

            if let Some(actual) = actual {
                fields.push(count_field("actual_source_version", actual.raw()));
            }

            ("product_query_source_snapshot_mismatch", fields)
        }
        Failure::TestProductMismatch {
            requested,
            compilation_package,
            compilation_kind,
        } => (
            "product_query_test_product_mismatch",
            vec![
                text_field("requested_product", requested.to_string()),
                text_field("compilation_package", compilation_package.as_str()),
                text_field("compilation_product_kind", product_kind(*compilation_kind)),
            ],
        ),
        Failure::TestCatalog {
            product,
            identity,
            cause,
        } => (
            "product_query_test_catalog",
            vec![
                text_field("product", product.to_string()),
                identity_field("test_identity", identity),
                text_field("test_catalog_cause", product_test_catalog_failure(*cause)),
            ],
        ),
        Failure::InvalidTestErrorTypeIdentity { digest } => (
            "product_query_invalid_test_error_type_identity",
            vec![DiagnosticFailureField::new(
                "type_digest",
                DiagnosticFailureValue::Identity(*digest),
            )],
        ),
        Failure::InvalidModulePath {
            declaration,
            segments,
        } => (
            "product_query_invalid_module_path",
            vec![
                identity_field("declaration", declaration),
                DiagnosticFailureField::new(
                    "module_path_segments",
                    DiagnosticFailureValue::TextList(segments.clone()),
                ),
            ],
        ),
        Failure::UnsupportedEntryResultType { ty, actual } => {
            let mut fields = vec![identity_field("semantic_type", ty)];

            if let Some(actual) = actual {
                fields.push(text_field("actual_representation", actual.as_str()));
            }

            ("product_query_unsupported_entry_result_type", fields)
        }
        Failure::InvalidCodegenRequest { unit, cause } => (
            "product_query_invalid_codegen_request",
            vec![
                identity_field("codegen_unit", unit),
                text_field("codegen_cause", codegen_request_failure(*cause)),
            ],
        ),
        Failure::CodegenBackendSelection { unit, cause } => {
            let (reason, backend) = backend_selection_failure(cause);

            (
                "product_query_codegen_backend_selection",
                vec![
                    identity_field("codegen_unit", unit),
                    text_field("codegen_cause", reason),
                    text_field("backend_name", backend.name()),
                    text_field("backend_revision", backend.revision()),
                    text_field("backend_toolchain_revision", backend.toolchain_revision()),
                ],
            )
        }
    };

    DiagnosticProductQueryFailure::new(reason, context)
}

const fn product_test_catalog_failure(
    cause: crate::compilation::ProductTestCatalogFailureKind,
) -> &'static str {
    use crate::compilation::ProductTestCatalogFailureKind as Error;

    match cause {
        Error::ProductMismatch => "product_mismatch",
        Error::DuplicateIdentity => "duplicate_identity",
        Error::InvalidResultMetadata => "invalid_result_metadata",
    }
}

const fn codegen_request_failure(cause: bray_codegen::CodegenRequestBuildError) -> &'static str {
    use bray_codegen::CodegenRequestBuildError as Error;

    match cause {
        Error::ArtifactUnitMismatch => "artifact_unit_mismatch",
        Error::MappingUnitMismatch => "mapping_unit_mismatch",
        Error::MappingTargetMismatch => "mapping_target_mismatch",
        Error::DebugInformationMismatch => "debug_information_mismatch",
        Error::DebugMappingCoverageMismatch => "debug_mapping_coverage_mismatch",
        Error::TargetMismatch => "target_mismatch",
    }
}

fn backend_selection_failure(
    cause: &bray_codegen::BackendSelectionError,
) -> (&'static str, &bray_codegen::BackendIdentity) {
    use bray_codegen::BackendSelectionError as Error;

    match cause {
        Error::Unavailable(backend) => ("unavailable", backend),
        Error::NotSelected(backend) => ("not_selected", backend),
    }
}

fn context_with_data(
    context: &ProductQueryContext,
    data: ProductDataKind,
) -> Vec<DiagnosticFailureField> {
    let mut fields = product_query_context(context);
    fields.push(text_field("data_kind", product_data_kind(data)));

    fields
}

fn push_optional_identity<T: std::hash::Hash>(
    fields: &mut Vec<DiagnosticFailureField>,
    name: &'static str,
    value: Option<&T>,
) {
    if let Some(value) = value {
        fields.push(identity_field(name, value));
    }
}

fn push_implementation_selection(
    fields: &mut Vec<DiagnosticFailureField>,
    selection: &bray_symbols::ImplementationSelection,
) {
    use bray_symbols::ImplementationSelection as Selection;

    let kind = match selection {
        Selection::Selected(implementation) => {
            fields.push(identity_field("selected_implementation", implementation));

            "selected"
        }
        Selection::Deferred => "deferred",
        Selection::Unavailable => "unavailable",
        Selection::Ambiguous(ambiguity) => {
            fields.extend([
                DiagnosticFailureField::new(
                    "ambiguous_candidate_keys",
                    DiagnosticFailureValue::IdentityList(
                        ambiguity
                            .candidates()
                            .iter()
                            .map(|candidate| identity(candidate.key()))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    ),
                ),
                DiagnosticFailureField::new(
                    "ambiguous_candidate_implementations",
                    DiagnosticFailureValue::IdentityList(
                        ambiguity
                            .candidates()
                            .iter()
                            .map(|candidate| identity(&candidate.instance()))
                            .collect::<Vec<_>>()
                            .into_boxed_slice(),
                    ),
                ),
            ]);

            "ambiguous"
        }
    };

    fields.push(text_field("actual_selection_kind", kind));
}

const fn proof_outcome(outcome: bray_symbols::ProofOutcome) -> &'static str {
    match outcome {
        bray_symbols::ProofOutcome::Proven => "proven",
        bray_symbols::ProofOutcome::Disproven => "disproven",
        bray_symbols::ProofOutcome::Unknown => "unknown",
        bray_symbols::ProofOutcome::Recovered => "recovered",
    }
}

const fn product_synchronization_component(
    component: ProductSynchronizationComponent,
) -> &'static str {
    match component {
        ProductSynchronizationComponent::LifecycleNeeds => "lifecycle_needs",
    }
}

const fn executable_entry_result(
    result: &bray_runtime_interface::ExecutableEntryResult,
) -> &'static str {
    match result {
        bray_runtime_interface::ExecutableEntryResult::Unit => "unit",
        bray_runtime_interface::ExecutableEntryResult::I32 => "i32",
        bray_runtime_interface::ExecutableEntryResult::Fallible { .. } => "fallible",
    }
}

const fn native_boundary_kind(
    kind: &bray_package_interface::InterfaceNativeBoundaryKind,
) -> &'static str {
    match kind {
        bray_package_interface::InterfaceNativeBoundaryKind::Callable => "callable",
        bray_package_interface::InterfaceNativeBoundaryKind::Static { .. } => "static",
    }
}

const fn mir_operation_kind(operation: &bray_ir::MirOperationKind) -> &'static str {
    use bray_ir::MirOperationKind as Operation;

    match operation {
        Operation::AnonymousCallable(_) => "anonymous_callable",
        Operation::DeclaredCallable(_) => "declared_callable",
        Operation::Store { .. } => "store",
        Operation::Borrow { .. } => "borrow",
        Operation::Unary { .. } => "unary",
        Operation::Binary { .. } => "binary",
        Operation::Aggregate(_) => "aggregate",
        Operation::Construct(_) => "construct",
        Operation::Convert { .. } => "convert",
        Operation::NumericConversion { .. } => "numeric_conversion",
        Operation::NullableQuery(_) => "nullable_query",
        Operation::PatternProjection { .. } => "pattern_projection",
        Operation::Generator(_) => "generator",
        Operation::Call(_) => "call",
        Operation::Memory(_) => "memory",
        Operation::Text(_) => "text",
        Operation::PanicReport(_) => "panic_report",
        Operation::Finalize(_) => "finalize",
        Operation::Destroy(_) => "destroy",
        Operation::Cleanup { .. } => "cleanup",
        Operation::Async(_) => "async",
        Operation::AdmitOutgoing { .. } => "outgoing_admission",
        Operation::DischargeOutgoing { .. } => "outgoing_discharge",
        Operation::Host(_) => "host",
    }
}

const fn mir_lifecycle_role(role: bray_ir::MirGeneratedLifecycleRole) -> &'static str {
    match role {
        bray_ir::MirGeneratedLifecycleRole::Finalize => "finalize",
        bray_ir::MirGeneratedLifecycleRole::StaticFinalize => "static_finalize",
        bray_ir::MirGeneratedLifecycleRole::Destroy => "destroy",
        bray_ir::MirGeneratedLifecycleRole::Cleanup(bray_ir::MirCleanupPhase::TaskCancellation) => {
            "cleanup_task_cancellation"
        }
        bray_ir::MirGeneratedLifecycleRole::Cleanup(
            bray_ir::MirCleanupPhase::LifecycleResolution,
        ) => "cleanup_lifecycle_resolution",
    }
}

const fn product_value_kind(kind: ProductValueKind) -> &'static str {
    match kind {
        ProductValueKind::TypeArgument => "type_argument",
        ProductValueKind::ConstantArgument => "constant_argument",
        ProductValueKind::TraitSatisfactionConstraint => "trait_satisfaction_constraint",
        ProductValueKind::PredicateConstraint => "predicate_constraint",
        ProductValueKind::TypeEqualityConstraint => "type_equality_constraint",
        ProductValueKind::GenericTypeArgument => "generic_type_argument",
        ProductValueKind::ClosedStaticReference => "closed_static_reference",
        ProductValueKind::OpenStaticReference => "open_static_reference",
        ProductValueKind::NamedType => "named_type",
        ProductValueKind::CallableType => "callable_type",
        ProductValueKind::LifecycleRepresentableType => "lifecycle_representable_type",
    }
}

const fn product_data_kind(kind: ProductDataKind) -> &'static str {
    match kind {
        ProductDataKind::Symbol => "symbol",
        ProductDataKind::TestResult => "test_result",
        ProductDataKind::ContainingModule => "containing_module",
        ProductDataKind::ContainingSymbol => "containing_symbol",
        ProductDataKind::MemberName => "member_name",
        ProductDataKind::SourceAnchor => "source_anchor",
        ProductDataKind::SourceSnapshot => "source_snapshot",
        ProductDataKind::SymbolKey => "symbol_key",
        ProductDataKind::ConcreteInstance => "concrete_instance",
        ProductDataKind::CallableInstance => "callable_instance",
        ProductDataKind::AnonymousCallableInstance => "anonymous_callable_instance",
        ProductDataKind::BoundHelperInstance => "bound_helper_instance",
        ProductDataKind::GeneratedLifecycleInstance => "generated_lifecycle_instance",
        ProductDataKind::ContextualSelfWitness => "contextual_self_witness",
        ProductDataKind::TraitDispatch => "trait_dispatch",
        ProductDataKind::GenericSubstitution => "generic_substitution",
        ProductDataKind::GenericOwner => "generic_owner",
        ProductDataKind::ImplementationWitness => "implementation_witness",
        ProductDataKind::CallableFulfillment => "callable_fulfillment",
        ProductDataKind::GenericConstraint => "generic_constraint",
        ProductDataKind::Intrinsic => "intrinsic",
        ProductDataKind::ConversionPlan => "conversion_plan",
        ProductDataKind::CompilerKnownRepresentation => "compiler_known_representation",
        ProductDataKind::LifecycleRole => "lifecycle_role",
        ProductDataKind::LifecycleType => "lifecycle_type",
        ProductDataKind::RealizedStatic => "realized_static",
        ProductDataKind::StaticDependencyCounter => "static_dependency_counter",
        ProductDataKind::StaticInitializer => "static_initializer",
        ProductDataKind::ImplementationHeader => "implementation_header",
        ProductDataKind::TestDiscovery => "test_discovery",
        ProductDataKind::DeclarationChunk => "declaration_chunk",
        ProductDataKind::DeclarationContainer => "declaration_container",
        ProductDataKind::ModulePath => "module_path",
        ProductDataKind::PackageRoot => "package_root",
        ProductDataKind::Module => "module",
        ProductDataKind::ReachabilityRealization => "reachability_realization",
        ProductDataKind::ReachabilityEvaluation => "reachability_evaluation",
        ProductDataKind::PartitionInstance => "partition_instance",
        ProductDataKind::CodegenUnitMapping => "codegen_unit_mapping",
        ProductDataKind::ProductHostOwnerUnit => "product_host_owner_unit",
        ProductDataKind::ProductHostStaticMapping => "product_host_static_mapping",
        ProductDataKind::ResultRepresentation => "result_representation",
        ProductDataKind::CallableSignature => "callable_signature",
        ProductDataKind::RuntimeDefaultSubject => "runtime_default_subject",
        ProductDataKind::RuntimeDefaultUnit => "runtime_default_unit",
        ProductDataKind::OperationResultType => "operation_result_type",
        ProductDataKind::SourceLineIndex => "source_line_index",
        ProductDataKind::SourceLocation => "source_location",
        ProductDataKind::NativeStaticContract => "native_static_contract",
        ProductDataKind::CallableParameters => "callable_parameters",
        ProductDataKind::CallableReceiver => "callable_receiver",
        ProductDataKind::StaticSubstitution => "static_substitution",
        ProductDataKind::ImportedSemanticAddress => "imported_semantic_address",
        ProductDataKind::ResolvedType => "resolved_type",
        ProductDataKind::CompilationDiagnostics => "compilation_diagnostics",
        ProductDataKind::PackageInterfaceContribution => "package_interface_contribution",
        ProductDataKind::PackageImplementationArtifact => "package_implementation_artifact",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticFailureValue;
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolId, SymbolKind};

    use super::{
        diagnostic_product_query_failure, product_query_context, push_mir_call_target,
        push_mir_helper,
    };
    use crate::compilation::{
        ProductQueryError, ProductQueryFailure, ProductSynchronizationComponent,
    };

    #[test]
    fn product_query_conversion_retains_categorical_values_as_text() {
        let error = ProductQueryError::from(ProductQueryFailure::SynchronizationPoisoned {
            component: ProductSynchronizationComponent::LifecycleNeeds,
        });

        let failure = diagnostic_product_query_failure(&error);

        assert_eq!(failure.as_str(), "product_query_synchronization_poisoned");

        assert_eq!(
            failure.context()[0].value(),
            &DiagnosticFailureValue::Text("lifecycle_needs".to_owned())
        );
    }

    #[test]
    fn product_query_conversion_uses_typed_symbol_fields() {
        let symbol = AnySymbolId::Function(FunctionSymbolId::from_symbol_id(SymbolId::new(11)));

        let error = ProductQueryError::from(ProductQueryFailure::UnexpectedSymbolKind {
            symbol,
            expected: SymbolKind::Trait,
            actual: SymbolKind::Function,
        });

        let failure = diagnostic_product_query_failure(&error);

        assert_eq!(failure.as_str(), "product_query_unexpected_symbol_kind");
        assert_eq!(failure.context()[0].name(), "symbol_kind");
        assert_eq!(failure.context()[1].name(), "symbol");
        assert_eq!(failure.context()[2].name(), "expected_symbol_kind");
        assert_eq!(failure.context()[3].name(), "actual_symbol_kind");
    }

    #[test]
    fn product_query_conversion_preserves_generic_substitution_leaf_payloads() {
        let error = ProductQueryError::from(ProductQueryFailure::GenericSubstitution {
            substitution: None,
            cause: bray_symbols::GenericSubstitutionShapeError::ArgumentCountMismatch {
                parameter_count: 3,
                argument_count: 2,
            },
        });

        let failure = diagnostic_product_query_failure(&error);
        let names: Vec<_> = failure.context().iter().map(|field| field.name()).collect();

        assert_eq!(failure.as_str(), "product_query_generic_substitution");

        assert_eq!(
            names,
            ["substitution_cause", "parameter_count", "argument_count",]
        );
    }

    #[test]
    fn source_location_context_preserves_source_and_byte_offset() {
        let fields =
            product_query_context(&crate::compilation::ProductQueryContext::SourceLocation {
                source: bray_source::SourceId::new(23),
                offset: bray_source::TextSize::new(47),
            });

        assert_eq!(
            fields.iter().map(|field| field.name()).collect::<Vec<_>>(),
            ["product_context_kind", "source", "source_offset"]
        );

        assert_eq!(fields[1].value(), &DiagnosticFailureValue::Count(23));
        assert_eq!(fields[2].value(), &DiagnosticFailureValue::Count(47));
    }

    #[test]
    fn mir_helper_context_preserves_category_abi_and_runtime_role() {
        let mut fields = Vec::new();

        push_mir_helper(&mut fields, &bray_ir::MirHelperReference::BeginGenerator);

        let names: Vec<_> = fields.iter().map(|field| field.name()).collect();

        assert_eq!(names, ["helper_kind", "helper_abi", "helper_runtime_role"]);

        assert_eq!(
            fields[2].value(),
            &DiagnosticFailureValue::Text("generator_begin".to_owned())
        );
    }

    #[test]
    fn mir_call_target_context_preserves_runtime_contract() {
        let mut fields = Vec::new();

        let target = bray_ir::MirCallTarget::Runtime(bray_ir::MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::TaskStart,
            bray_runtime_interface::RuntimeAbiVersion::new(3, 7),
        ));

        push_mir_call_target(&mut fields, &target);

        let names: Vec<_> = fields.iter().map(|field| field.name()).collect();

        assert_eq!(
            names,
            [
                "call_target_kind",
                "call_target_abi",
                "call_target_runtime_role",
                "call_target_runtime_abi_major",
                "call_target_runtime_abi_minor",
            ]
        );

        assert_eq!(
            fields[2].value(),
            &DiagnosticFailureValue::Text("task_start".to_owned())
        );

        assert_eq!(fields[3].value(), &DiagnosticFailureValue::Count(3));
        assert_eq!(fields[4].value(), &DiagnosticFailureValue::Count(7));
    }
}
