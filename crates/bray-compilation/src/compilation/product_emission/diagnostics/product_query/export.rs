use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

use crate::fact::diagnostic_context::{count_field, identity_field, push_source_span, text_field};

pub(super) fn push_package_interface_export_failure(
    fields: &mut Vec<DiagnosticFailureField>,
    error: &crate::compilation::PackageInterfaceExportError,
) {
    use crate::compilation::PackageInterfaceExportError as Error;

    let reason = match error {
        Error::Cancelled => "cancelled",
        Error::Query(cause) => {
            push_evaluation_failure(fields, cause);

            "query"
        }
        Error::InvalidCompilation => "invalid_compilation",
        Error::SemanticValueStoreCreate(_) => "semantic_value_store_create",
        Error::SemanticValueStore(cause) => {
            crate::fact::push_semantic_value_failure(fields, *cause);

            "semantic_value_store"
        }
        Error::RecoveredPublicSymbol(kind) => {
            fields.push(text_field("recovered_symbol_kind", kind.as_str()));

            "recovered_public_symbol"
        }
        Error::ConstantCallableEvaluation { declaration, cause } => {
            push_interface_symbol_identity(fields, "declaration", declaration);
            push_evaluation_failure(fields, cause);

            "constant_callable_evaluation"
        }
        Error::ExecutableTemplateEvaluation { declaration, cause } => {
            push_interface_symbol_identity(fields, "declaration", declaration);
            push_evaluation_failure(fields, cause);

            "executable_template_evaluation"
        }
        Error::IncompletePublicDeclarationSemantics(kind) => {
            fields.push(text_field("declaration_kind", kind.as_str()));

            "incomplete_public_declaration_semantics"
        }
        Error::IncompleteSemanticFragment {
            declaration,
            table,
            reference,
        }
        | Error::CyclicSemanticFragment {
            declaration,
            table,
            reference,
        } => {
            if let Some(declaration) = declaration {
                push_interface_symbol_identity(fields, "declaration", declaration);
            }

            fields.push(text_field("semantic_table", table.as_str()));
            fields.push(count_field("semantic_reference", u64::from(*reference)));

            if matches!(error, Error::IncompleteSemanticFragment { .. }) {
                "incomplete_semantic_fragment"
            } else {
                "cyclic_semantic_fragment"
            }
        }
        Error::ConflictingSemanticFragment {
            first,
            second,
            first_span,
            second_span,
            identity,
        } => {
            push_interface_symbol_identity(fields, "first_declaration", first);
            push_interface_symbol_identity(fields, "second_declaration", second);
            fields.push(identity_field("external_symbol", identity));
            push_optional_source_span(fields, "first_declaration", *first_span);
            push_optional_source_span(fields, "second_declaration", *second_span);

            "conflicting_semantic_fragment"
        }
        Error::FragmentCoordination(cause) => {
            push_evaluation_failure(fields, cause);

            "fragment_coordination"
        }
        Error::FragmentCommit(cause) => {
            push_fragment_commit_failure(fields, cause);

            "fragment_commit"
        }
        Error::Surface(cause) => {
            push_export_surface_failure(fields, cause);

            "surface"
        }
        Error::Bundle(cause) => {
            push_export_bundle_failure(fields, cause);

            "bundle"
        }
    };

    fields.push(text_field("external_symbol_cause", reason));
}

fn push_evaluation_failure(
    fields: &mut Vec<DiagnosticFailureField>,
    error: &crate::fact::FactQueryError,
) {
    fields.push(DiagnosticFailureField::new(
        "evaluation_cause",
        DiagnosticFailureValue::Evaluation(Box::new(super::super::diagnostic_evaluation_failure(
            error,
        ))),
    ));
}

fn push_interface_symbol_identity(
    fields: &mut Vec<DiagnosticFailureField>,
    name: &'static str,
    identity: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
) {
    fields.push(DiagnosticFailureField::new(
        name,
        DiagnosticFailureValue::InterfaceSymbolIdentity(identity.clone()),
    ));
}

fn push_optional_source_span(
    fields: &mut Vec<DiagnosticFailureField>,
    prefix: &'static str,
    span: Option<bray_source::SourceSpan>,
) {
    if let Some(span) = span {
        let (source, start, end) = match prefix {
            "first_declaration" => ("first_source", "first_source_start", "first_source_end"),
            "second_declaration" => ("second_source", "second_source_start", "second_source_end"),
            _ => unreachable!(),
        };

        push_source_span(fields, source, start, end, span);
    }
}

fn push_fragment_commit_failure(
    fields: &mut Vec<DiagnosticFailureField>,
    error: &bray_package_interface::InterfaceSemanticCommitError,
) {
    use bray_package_interface::InterfaceSemanticCommitError as Error;

    let reason = match error {
        Error::MissingReference { table, reference } => {
            fields.push(text_field("semantic_table", table.as_str()));
            fields.push(count_field("semantic_reference", u64::from(*reference)));

            "missing_reference"
        }
        Error::UnexpectedPackageRecord(table) => {
            fields.push(text_field("semantic_table", table.as_str()));

            "unexpected_package_record"
        }
        Error::ConflictingRecord { owner, kind } => {
            fields.push(identity_field("record_owner", owner));
            fields.push(text_field("record_kind", kind.as_str()));

            "conflicting_record"
        }
        Error::CyclicReference(table) => {
            fields.push(text_field("semantic_table", table.as_str()));

            "cyclic_reference"
        }
        Error::IdentityOverflow(table) => {
            fields.push(text_field("semantic_table", table.as_str()));

            "identity_overflow"
        }
    };

    fields.push(text_field("fragment_commit_cause", reason));
}

fn push_export_surface_failure(
    fields: &mut Vec<DiagnosticFailureField>,
    error: &bray_package_interface::PackageInterfaceExportSurfaceError,
) {
    use bray_package_interface::PackageInterfaceExportSurfaceError as Error;

    let reason = match error {
        Error::DuplicateSymbol(symbol) => {
            fields.push(identity_field("external_symbol", symbol));

            "duplicate_symbol"
        }
        Error::MissingSymbol(symbol) => {
            fields.push(identity_field("external_symbol", symbol));

            "missing_symbol"
        }
        Error::SymbolCountOverflow => "symbol_count_overflow",
        Error::Surface(problem) => {
            fields.push(DiagnosticFailureField::new(
                "surface_problem",
                DiagnosticFailureValue::InterfaceSymbolGraphProblem(
                    bray_package_interface::diagnostic_surface_problem(problem),
                ),
            ));

            "surface_contract"
        }
    };

    fields.push(text_field("export_surface_cause", reason));
}

fn push_export_bundle_failure(
    fields: &mut Vec<DiagnosticFailureField>,
    error: &bray_package_interface::PackageInterfaceExportBuildError,
) {
    use bray_package_interface::PackageInterfaceExportBuildError as Error;

    let reason = match error {
        Error::MissingSemantics(symbol) => {
            fields.push(identity_field("external_symbol", symbol));

            "missing_semantics"
        }
        Error::Validation(cause) => {
            fields.push(DiagnosticFailureField::new(
                "interface_validation_cause",
                DiagnosticFailureValue::InterfaceValidationFailure(
                    cause.diagnostic_failure(),
                ),
            ));

            "validation"
        }
        Error::DuplicateExecutableTemplate(owner) => {
            fields.push(count_field("owner", u64::from(owner.raw())));

            "duplicate_executable_template"
        }
        Error::DuplicateConstantCallableBody(owner) => {
            fields.push(count_field("owner", u64::from(owner.raw())));

            "duplicate_constant_callable_body"
        }
        Error::InvalidExecutableTemplateFamily(owner) => {
            fields.push(count_field("owner", u64::from(owner.raw())));

            "invalid_executable_template_family"
        }
        Error::DuplicateNativeBoundary(owner) => {
            fields.push(count_field("owner", u64::from(owner.raw())));

            "duplicate_native_boundary"
        }
    };

    fields.push(text_field("export_bundle_cause", reason));
}
