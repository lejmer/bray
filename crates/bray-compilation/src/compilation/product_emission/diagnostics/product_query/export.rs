use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

use crate::fact::diagnostic_context::text_field;

pub(super) fn push_package_interface_export_failure(
    fields: &mut Vec<DiagnosticFailureField>,
    error: &crate::compilation::PackageInterfaceExportError,
) {
    use crate::compilation::PackageInterfaceExportError as Error;

    let reason = match error {
        Error::SourceDiagnostic(diagnostic) => {
            fields.push(text_field(
                "execution_evidence_diagnostic",
                format!("{diagnostic:?}"),
            ));

            "execution_evidence"
        }
        Error::Cancelled => "cancelled",
        Error::Query(cause) => {
            push_evaluation_failure(fields, cause);

            "query"
        }
        Error::InvalidCompilation => "invalid_compilation",
        Error::InvalidCompilationCause(cause) => {
            push_invalid_compilation_cause(fields, cause);

            "invalid_compilation_cause"
        }
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
        Error::FragmentCoordination(cause) => {
            push_evaluation_failure(fields, cause);

            "fragment_coordination"
        }
    };

    fields.push(text_field("external_symbol_cause", reason));
}

fn push_invalid_compilation_cause(
    fields: &mut Vec<DiagnosticFailureField>,
    cause: &crate::compilation::PackageInterfaceInvalidCompilationCause,
) {
    use crate::compilation::PackageInterfaceInvalidCompilationCause as Cause;

    match cause {
        Cause::Capacity { field, actual } => {
            fields.push(text_field("capacity_field", *field));
            fields.push(text_field("capacity_actual", actual));
        }
        Cause::RuntimeRequirements(cause) => {
            fields.push(text_field(
                "runtime_requirements_cause",
                format!("{cause:?}"),
            ));
        }
        Cause::ExportRequestMismatch => {
            fields.push(text_field("export_contract", "request_mismatch"));
        }
    }
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
