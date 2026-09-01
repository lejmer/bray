use super::failure::{
    DiagnosticEmissionFieldJson, count_field, count_u64_field, digest_field, text_field,
};
use bray_diagnostics::DiagnosticProductQueryFailure as Failure;

pub(in crate::output::diagnostic::json) fn product_query_failure_context(
    failure: &Failure,
) -> Vec<DiagnosticEmissionFieldJson> {
    let mut fields = vec![text_field("cause", failure.as_str())];

    match failure {
        Failure::Missing { context, data } | Failure::Conflict { context, data } => {
            append_context_data_fields(&mut fields, context, *data);
        }
        Failure::UnexpectedKind {
            context,
            expected,
            actual,
        } => append_unexpected_kind_fields(&mut fields, context, *expected, *actual),
        Failure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => append_count_mismatch_fields(&mut fields, context, *data, *expected, *actual),
        Failure::UnsupportedConstantValue { value, kind } => fields.extend([
            text_field("constant_value", value),
            text_field("constant_value_kind", kind),
        ]),
        Failure::GenericSubstitution {
            substitution,
            cause,
        } => append_generic_substitution_fields(&mut fields, substitution.as_deref(), cause),
        Failure::TraitApplicationMismatch {
            witness,
            expected,
            actual,
        } => fields.extend([
            text_field("witness", witness),
            text_field("expected_trait_application", expected),
            text_field("actual_trait_application", actual),
        ]),
        Failure::ConflictingImplementationWitness {
            identity,
            existing,
            actual,
        } => fields.extend([
            text_field("witness_identity", identity),
            text_field("existing_implementation", existing),
            text_field("actual_implementation", actual),
        ]),
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => fields.extend([
            text_field("semantic_type", ty),
            text_field("expected_value_kind", expected.as_str()),
            text_field("actual_semantic_type", actual),
        ]),
        Failure::SynchronizationPoisoned { component } => {
            fields.push(text_field("synchronization_component", component));
        }
        Failure::StaticDependencyOverflow { static_instance }
        | Failure::StaticDependencyUnderflow { static_instance } => {
            fields.push(text_field("static_instance", static_instance));
        }
        Failure::StaticLifecycleCycle { instances } => {
            fields.push(text_field("static_instances", instances.join(" -> ")));
        }
        Failure::ConflictingConcreteInstance { key } => {
            fields.push(text_field("instance", key));
        }
        Failure::ConflictingStaticRelocation { value } => {
            fields.push(text_field("constant_value", value));
        }
        Failure::CallableDefinitionMismatch {
            instance,
            expected,
            actual,
        } => fields.extend([
            text_field("instance", instance),
            count_field("expected_callable_definition", *expected),
            count_field("actual_callable_definition", *actual),
        ]),
        Failure::TraitDefinitionMismatch {
            context,
            expected,
            actual,
        } => {
            fields.extend(context_fields(context));

            fields.extend([
                count_field("expected_trait_definition", *expected),
                count_field("actual_trait_definition", *actual),
            ]);
        }
        Failure::InvalidCodegenSourceFile { source, path } => {
            if let Some(source) = source {
                fields.push(count_field("source", *source));
            }

            fields.push(text_field("path", path));
        }
        Failure::SourceIndex { source, cause } => fields.extend([
            count_field("source", *source),
            text_field("source_index_cause", cause),
        ]),
        Failure::ExternalSymbolIdentity { symbol, cause } => fields.extend([
            text_field("symbol", symbol),
            text_field("external_symbol_cause", cause),
        ]),
        Failure::InvalidCompilerKnownDeclarationKey { key }
        | Failure::InvalidRecognizedStandardLibraryDeclarationKey { key } => {
            fields.push(text_field("declaration_key", key));
        }
        Failure::InvalidPackageIdentity { identity } => {
            fields.push(text_field("package_identity", identity));
        }
        Failure::UnexpectedEntryResult { actual } => {
            fields.push(text_field("actual_entry_result", actual));
        }
        Failure::CompilerKnownRepresentationMismatch {
            ty,
            expected,
            actual,
        } => append_representation_mismatch_fields(&mut fields, ty, expected, actual.as_deref()),
        Failure::NativeBoundaryKindMismatch { reference, actual } => fields.extend([
            text_field("static_reference", reference),
            text_field("actual_boundary_kind", actual),
        ]),
        Failure::UnexpectedSymbolKind {
            symbol,
            expected,
            actual,
        } => fields.extend([
            text_field("symbol", symbol),
            text_field("expected_symbol_kind", expected),
            text_field("actual_symbol_kind", actual),
        ]),
        Failure::ImplementationSymbolKeyExpected { key, actual } => fields.extend([
            text_field("symbol_key", key),
            text_field("actual_symbol_kind", actual),
        ]),
        Failure::UnsupportedRuntimeRole { role } | Failure::UnsupportedLifecycleRole { role } => {
            fields.push(text_field("runtime_role", role));
        }
        Failure::InvalidHelperOperation {
            context,
            helper,
            operation,
        } => {
            fields.extend(context_fields(context));

            fields.extend([
                text_field("helper", helper),
                text_field("operation", operation),
            ]);
        }
        Failure::InvalidHelperCallTarget {
            context,
            helper,
            target,
        } => {
            fields.extend(context_fields(context));

            fields.extend([
                text_field("helper", helper),
                text_field("call_target", target),
            ]);
        }
        Failure::LifecycleRoleMismatch {
            instance,
            expected,
            actual,
        } => append_lifecycle_role_mismatch_fields(
            &mut fields,
            instance,
            expected.as_deref(),
            actual.as_deref(),
        ),
        Failure::BuiltInProofMismatch {
            requirement,
            actual,
        } => append_built_in_proof_fields(&mut fields, requirement, actual.as_deref()),
        Failure::ImplementationSelectionMismatch {
            requirement,
            actual,
        } => fields.extend([
            text_field("requirement", requirement),
            text_field("actual_selection", actual),
        ]),
        Failure::UnsupportedRuntimeDefaultSubject { provider, subject } => fields.extend([
            text_field("provider", provider),
            text_field("subject", subject),
        ]),
        Failure::InvalidCallableDefinitionSymbol { symbol, actual } => fields.extend([
            text_field("symbol", symbol),
            text_field("actual_symbol_kind", actual),
        ]),
        Failure::SourceSnapshotMismatch {
            source,
            expected,
            actual,
        } => {
            fields.extend([
                count_field("source", *source),
                text_field("expected_source_version", expected),
            ]);

            push_optional_text(&mut fields, "actual_source_version", actual.as_deref());
        }
        Failure::TestProductMismatch {
            requested,
            compilation_package,
            compilation_kind,
        } => fields.extend([
            text_field("requested_product", requested),
            text_field("compilation_package", compilation_package),
            text_field("compilation_product_kind", compilation_kind),
        ]),
        Failure::TestCatalog {
            product,
            identity,
            cause,
        } => fields.extend([
            text_field("product", product),
            text_field("test_identity", identity),
            text_field("test_catalog_cause", cause),
        ]),
        Failure::InvalidTestErrorTypeIdentity { digest } => {
            fields.push(digest_field("type_digest", digest));
        }
        Failure::InvalidModulePath {
            declaration,
            segments,
        } => fields.extend([
            count_field("declaration", *declaration),
            text_field("module_path_segments", segments.join("::")),
        ]),
        Failure::UnsupportedEntryResultType { ty, actual } => {
            fields.push(text_field("semantic_type", ty));
            push_optional_text(&mut fields, "actual_representation", actual.as_deref());
        }
        Failure::InvalidCodegenRequest { unit, cause }
        | Failure::CodegenBackendSelection { unit, cause } => fields.extend([
            digest_field("codegen_unit", unit),
            text_field("codegen_cause", cause),
        ]),
    }

    fields
}

fn append_built_in_proof_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    requirement: &str,
    actual: Option<&str>,
) {
    fields.push(text_field("requirement", requirement));
    push_optional_text(fields, "actual_proof", actual);
}

fn append_generic_substitution_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    substitution: Option<&str>,
    cause: &str,
) {
    push_optional_text(fields, "substitution", substitution);
    fields.push(text_field("substitution_cause", cause));
}

fn append_representation_mismatch_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    ty: &str,
    expected: &str,
    actual: Option<&str>,
) {
    fields.extend([
        text_field("semantic_type", ty),
        text_field("expected_representation", expected),
    ]);

    push_optional_text(fields, "actual_representation", actual);
}

fn append_lifecycle_role_mismatch_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    instance: &str,
    expected: Option<&str>,
    actual: Option<&str>,
) {
    fields.push(text_field("instance", instance));
    push_optional_text(fields, "expected_lifecycle_role", expected);
    push_optional_text(fields, "actual_lifecycle_role", actual);
}

fn append_context_data_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    data: bray_diagnostics::DiagnosticProductDataKind,
) {
    fields.extend(context_fields(context));
    fields.push(text_field("data_kind", data.as_str()));
}

fn append_unexpected_kind_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    expected: bray_diagnostics::DiagnosticProductValueKind,
    actual: bray_diagnostics::DiagnosticProductValueKind,
) {
    fields.extend(context_fields(context));

    fields.extend([
        text_field("expected_value_kind", expected.as_str()),
        text_field("actual_value_kind", actual.as_str()),
    ]);
}

fn append_count_mismatch_fields(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    data: bray_diagnostics::DiagnosticProductDataKind,
    expected: usize,
    actual: usize,
) {
    fields.extend(context_fields(context));

    fields.extend([
        text_field("data_kind", data.as_str()),
        count_usize_field("expected_count", expected),
        count_usize_field("actual_count", actual),
    ]);
}

fn context_fields(
    context: &bray_diagnostics::DiagnosticProductQueryContext,
) -> [DiagnosticEmissionFieldJson; 2] {
    [
        text_field("product_context_kind", context.kind().as_str()),
        text_field("product_context_identity", context.identity()),
    ]
}

fn count_usize_field(name: &'static str, value: usize) -> DiagnosticEmissionFieldJson {
    let value = u64::try_from(value).expect("supported pointer widths fit diagnostic counts");

    count_u64_field(name, value)
}

fn push_optional_text(
    fields: &mut Vec<DiagnosticEmissionFieldJson>,
    name: &'static str,
    value: Option<&str>,
) {
    if let Some(value) = value {
        fields.push(text_field(name, value));
    }
}
