use bray_diagnostics::{DiagnosticFailureField, DiagnosticForeignQueryFailure};

use crate::compilation::{
    ForeignDataKind, ForeignQueryContext, ForeignQueryError, ForeignQueryFailure,
};
use crate::fact::diagnostic_context::{
    identity_field, natural_field, semantic_type_kind, text_field,
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
        } => (
            "foreign_query_unexpected_semantic_type",
            vec![
                identity_field("semantic_type", ty),
                text_field("expected_type_kind", foreign_type_kind(*expected)),
                text_field("actual_semantic_type_kind", semantic_type_kind(actual)),
            ],
        ),
        Failure::UnexpectedTypeTemplate {
            context,
            expected,
            actual,
        } => {
            let mut fields = foreign_query_context(context);

            fields.extend([
                text_field("expected_type_kind", foreign_type_kind(*expected)),
                text_field("actual_type_template_kind", type_template_kind(actual)),
            ]);

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
    match kind {
        bray_symbols::GenericArgumentKind::Type => "type",
        bray_symbols::GenericArgumentKind::Constant => "constant",
    }
}

const fn foreign_source_role(role: crate::compilation::ForeignSourceRole) -> &'static str {
    match role {
        crate::compilation::ForeignSourceRole::Runtime(role) => role.as_str(),
        crate::compilation::ForeignSourceRole::Platform(role) => role.as_str(),
    }
}

const fn type_template_kind(template: &bray_symbols::TypeExpressionTemplate) -> &'static str {
    use bray_symbols::TypeExpressionTemplate as Type;

    match template {
        Type::Resolved(_) => "resolved",
        Type::Named { .. } => "named",
        Type::CallableContract { .. } => "callable_contract",
        Type::TypeValuedMemberProjection { .. } => "type_valued_member_projection",
        Type::Tuple(_) => "tuple",
        Type::Array { .. } => "array",
        Type::FlexibleArray(_) => "flexible_array",
        Type::Slice(_) => "slice",
        Type::Nullable(_) => "nullable",
        Type::Borrow { .. } => "borrow",
        Type::TraitView(_) => "trait_view",
        Type::OwnedIndirection { .. } => "owned_indirection",
        Type::Callable(_) => "callable",
    }
}

fn foreign_query_context(context: &ForeignQueryContext) -> Vec<DiagnosticFailureField> {
    use ForeignQueryContext as Context;

    let kind = match context {
        Context::Symbol(_) => "symbol",
        Context::Function(_) => "function",
        Context::Static(_) => "static",
        Context::Directive(_) => "directive",
        Context::Source(_) => "source",
        Context::Substitution(_) => "substitution",
        Context::CompilerKnownRepresentation { .. } => "compiler_known_representation",
        Context::PlatformService(_) => "platform_service",
    };

    vec![
        text_field("foreign_context_kind", kind),
        identity_field("foreign_context_identity", context),
    ]
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

    use super::diagnostic_foreign_query_failure;
    use crate::compilation::{ForeignQueryError, ForeignQueryFailure};

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
}
