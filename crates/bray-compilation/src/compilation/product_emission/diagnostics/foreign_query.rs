use bray_diagnostics::{DiagnosticFailureField, DiagnosticForeignQueryFailure};

use crate::compilation::{
    ForeignDataKind, ForeignQueryContext, ForeignQueryError, ForeignQueryFailure,
};
use crate::fact::diagnostic_context::{
    boolean_field, count_field, identity_field, natural_field, push_semantic_type_data,
    push_symbol, push_type_template_data, text_field,
};

pub(super) fn diagnostic_foreign_query_failure(
    error: &ForeignQueryError,
) -> DiagnosticForeignQueryFailure {
    use ForeignQueryFailure as Failure;

    let (reason, context) = match error.cause() {
        Failure::Missing { context, data } => {
            ("foreign_query_missing", context_with_data(context, *data))
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

            ("foreign_query_count_mismatch", fields)
        }
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => {
            let mut fields = vec![
                identity_field("semantic_type", ty),
                text_field("expected_type_kind", foreign_type_kind(*expected)),
            ];

            push_semantic_type_data(&mut fields, actual);

            ("foreign_query_unexpected_semantic_type", fields)
        }
        Failure::UnexpectedTypeTemplate {
            context,
            expected,
            actual,
        } => {
            let mut fields = foreign_query_context(context);

            fields.push(text_field(
                "expected_type_kind",
                foreign_type_kind(*expected),
            ));

            push_type_template_data(&mut fields, actual);

            ("foreign_query_unexpected_type_template", fields)
        }
        Failure::UnexpectedGenericArgument {
            substitution,
            expected,
            actual,
        } => (
            "foreign_query_unexpected_generic_argument",
            vec![
                identity_field("substitution", substitution),
                text_field("expected_argument_kind", generic_argument_kind(*expected)),
                text_field("actual_argument_kind", generic_argument_kind(*actual)),
            ],
        ),
        Failure::NumericOverflow {
            context,
            value,
            target,
        } => {
            let mut fields = foreign_query_context(context);

            fields.extend([
                natural_field("value", *value),
                text_field("integer_width", foreign_integer_width(*target)),
            ]);

            ("foreign_query_numeric_overflow", fields)
        }
        Failure::InvalidPlatformServiceRole { role } => (
            "foreign_query_invalid_platform_service_role",
            vec![text_field("platform_service_role", role.as_str())],
        ),
        Failure::CallableSignature { function, cause } => {
            let mut fields = vec![
                identity_field("function", function),
                text_field(
                    "signature_cause",
                    crate::fact::callable_signature_reason(cause),
                ),
            ];

            if let bray_symbols::CallableSignatureTemplateError::SemanticValue(cause) = cause {
                crate::fact::push_semantic_value_failure(&mut fields, *cause);
            }

            ("foreign_query_callable_signature", fields)
        }
        Failure::ConflictingSourceRoles {
            function,
            runtime,
            platform,
        } => (
            "foreign_query_conflicting_source_roles",
            vec![
                identity_field("function", function),
                text_field("runtime_role", runtime.as_str()),
                text_field("platform_role", platform.as_str()),
            ],
        ),
        Failure::DuplicateSourceRole {
            function,
            first,
            duplicate,
        } => (
            "foreign_query_duplicate_source_role",
            vec![
                identity_field("function", function),
                text_field("first_role", foreign_source_role(*first)),
                text_field("duplicate_role", foreign_source_role(*duplicate)),
            ],
        ),
    };

    DiagnosticForeignQueryFailure::new(reason, context)
}

const fn foreign_type_kind(kind: crate::compilation::ForeignTypeKind) -> &'static str {
    match kind {
        crate::compilation::ForeignTypeKind::Callable => "callable",
    }
}

const fn foreign_integer_width(width: crate::compilation::ForeignIntegerWidth) -> &'static str {
    match width {
        crate::compilation::ForeignIntegerWidth::U64 => "u64",
    }
}

const fn generic_argument_kind(kind: bray_symbols::GenericArgumentKind) -> &'static str {
    kind.as_str()
}

const fn foreign_source_role(role: crate::compilation::ForeignSourceRole) -> &'static str {
    match role {
        crate::compilation::ForeignSourceRole::Runtime(role) => role.as_str(),
        crate::compilation::ForeignSourceRole::Platform(role) => role.as_str(),
    }
}

fn foreign_query_context(context: &ForeignQueryContext) -> Vec<DiagnosticFailureField> {
    use ForeignQueryContext as Context;

    let mut fields = Vec::new();

    let kind = match context {
        Context::Symbol(symbol) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", *symbol);

            "symbol"
        }
        Context::Function(function) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", (*function).into());

            "function"
        }
        Context::Static(value) => {
            push_symbol(&mut fields, "symbol_kind", "symbol", (*value).into());

            "static"
        }
        Context::Directive(anchor) => {
            fields.extend([
                count_field("source", u64::from(anchor.source_id().raw())),
                count_field(
                    "source_start",
                    u64::from(anchor.full_range().start().bytes()),
                ),
                count_field("source_end", u64::from(anchor.full_range().end().bytes())),
                boolean_field("source_recovered", anchor.is_recovered()),
            ]);

            "directive"
        }
        Context::Source(source) => {
            fields.push(count_field("source", u64::from(source.raw())));

            "source"
        }
        Context::Substitution(substitution) => {
            fields.push(identity_field("substitution", substitution));

            "substitution"
        }
        Context::CompilerKnownRepresentation { role, ty } => {
            fields.push(text_field("representation_role", role.as_str()));

            if let Some(ty) = ty {
                fields.push(identity_field("semantic_type", ty));
            }

            "compiler_known_representation"
        }
        Context::PlatformService(role) => {
            fields.push(text_field("platform_service_role", role.as_str()));

            "platform_service"
        }
    };

    fields.insert(0, text_field("foreign_context_kind", kind));

    fields
}

fn context_with_data(
    context: &ForeignQueryContext,
    data: ForeignDataKind,
) -> Vec<DiagnosticFailureField> {
    let mut fields = foreign_query_context(context);
    fields.push(text_field("data_kind", foreign_data_kind(data)));

    fields
}

const fn foreign_data_kind(kind: ForeignDataKind) -> &'static str {
    match kind {
        ForeignDataKind::ContainingModule => "containing_module",
        ForeignDataKind::SourceAnchor => "source_anchor",
        ForeignDataKind::FunctionBindingRecord => "function_binding_record",
        ForeignDataKind::StaticBindingRecord => "static_binding_record",
        ForeignDataKind::FunctionDeclarationSyntax => "function_declaration_syntax",
        ForeignDataKind::StaticDeclarationSyntax => "static_declaration_syntax",
        ForeignDataKind::SourceSnapshot => "source_snapshot",
        ForeignDataKind::SourceText => "source_text",
        ForeignDataKind::StructureRecord => "structure_record",
        ForeignDataKind::UnionRecord => "union_record",
        ForeignDataKind::UnionVariantRecord => "union_variant_record",
        ForeignDataKind::UnaryRepresentationArgument => "unary_representation_argument",
        ForeignDataKind::RepresentationSymbol => "representation_symbol",
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::{CallableSignatureTemplateError, FunctionSymbolId, SymbolId};

    use super::{diagnostic_foreign_query_failure, foreign_query_context};
    use crate::compilation::{ForeignQueryContext, ForeignQueryError, ForeignQueryFailure};

    #[test]
    fn foreign_query_conversion_uses_typed_function_and_cause_fields() {
        let error = ForeignQueryError::from(ForeignQueryFailure::CallableSignature {
            function: FunctionSymbolId::from_symbol_id(SymbolId::new(13)),
            cause: CallableSignatureTemplateError::InvalidCallableType,
        });

        let failure = diagnostic_foreign_query_failure(&error);

        assert_eq!(failure.as_str(), "foreign_query_callable_signature");
        assert_eq!(failure.context()[0].name(), "function");
        assert_eq!(failure.context()[1].name(), "signature_cause");
    }

    #[test]
    fn foreign_query_conversion_preserves_callable_signature_leaf_payloads() {
        let expected_store = bray_symbols::SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("expected semantic store should build: {error:?}"));

        let actual_store = bray_symbols::SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("actual semantic store should build: {error:?}"));

        let error = ForeignQueryError::from(ForeignQueryFailure::CallableSignature {
            function: FunctionSymbolId::from_symbol_id(SymbolId::new(17)),
            cause: CallableSignatureTemplateError::SemanticValue(
                bray_symbols::SemanticValueStoreError::ForeignId {
                    expected: expected_store.id(),
                    actual: actual_store.id(),
                },
            ),
        });

        let failure = diagnostic_foreign_query_failure(&error);
        let names: Vec<_> = failure.context().iter().map(|field| field.name()).collect();

        assert_eq!(failure.as_str(), "foreign_query_callable_signature");

        assert_eq!(
            names,
            [
                "function",
                "signature_cause",
                "expected_store",
                "actual_store",
            ]
        );
    }

    #[test]
    fn foreign_context_preserves_compiler_known_and_platform_roles() {
        let representation =
            foreign_query_context(&ForeignQueryContext::CompilerKnownRepresentation {
                role: bray_compiler_known::RepresentationRole::ScalarBool,
                ty: None,
            });

        let platform = foreign_query_context(&ForeignQueryContext::PlatformService(
            bray_runtime_interface::PlatformServiceRole::StandardOutputWrite,
        ));

        assert_eq!(representation[1].name(), "representation_role");

        assert_eq!(
            representation[1].value(),
            &bray_diagnostics::DiagnosticFailureValue::Text("ScalarBool".to_owned())
        );

        assert_eq!(platform[1].name(), "platform_service_role");

        assert_eq!(
            platform[1].value(),
            &bray_diagnostics::DiagnosticFailureValue::Text(
                "platform.standard_output.write".to_owned(),
            )
        );
    }
}
