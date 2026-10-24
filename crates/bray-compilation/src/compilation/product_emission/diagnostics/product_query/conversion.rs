use bray_diagnostics::{DiagnosticFailureField, DiagnosticProductQueryFailure};

use crate::compilation::{ProductQueryError, ProductQueryFailure};
use crate::fact::diagnostic_context::{
    constant_value_kind, count_field, identity_field, natural_field, product_kind, push_symbol,
    text_field,
};

pub(in crate::compilation::product_emission::diagnostics) fn diagnostic_product_query_failure(
    error: &ProductQueryError,
) -> DiagnosticProductQueryFailure {
    use ProductQueryFailure as Failure;

    let (reason, context) = match error.cause() {
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
        Failure::UnsupportedEntryResultType { ty, actual } => {
            let mut fields = vec![identity_field("semantic_type", ty)];

            if let Some(actual) = actual {
                fields.push(text_field("actual_representation", actual.as_str()));
            }

            ("product_query_unsupported_entry_result_type", fields)
        }
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

fn backend_selection_failure(
    cause: &bray_codegen::BackendSelectionError,
) -> (&'static str, &bray_codegen::BackendIdentity) {
    use bray_codegen::BackendSelectionError as Error;

    match cause {
        Error::Unavailable(backend) => ("unavailable", backend),
        Error::NotSelected(backend) => ("not_selected", backend),
    }
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

#[cfg(test)]
mod tests {
    use super::diagnostic_product_query_failure;
    use crate::compilation::{ProductQueryError, ProductQueryFailure};

    #[test]
    fn substitution_capacity_retains_its_specific_cause() {
        let error = ProductQueryError::from(ProductQueryFailure::GenericSubstitution {
            substitution: None,
            cause: bray_symbols::GenericSubstitutionShapeError::OrdinalOverflow,
        });

        let failure = diagnostic_product_query_failure(&error);

        assert_eq!(failure.as_str(), "product_query_generic_substitution");

        assert!(failure.context().iter().any(|field| matches!(field.value(),
            bray_diagnostics::DiagnosticFailureValue::Text(value) if value == "generic_substitution_ordinal_overflow"
        )));
    }
}
