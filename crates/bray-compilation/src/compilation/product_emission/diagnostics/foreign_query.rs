use bray_diagnostics::{DiagnosticFailureField, DiagnosticForeignQueryFailure};

use crate::compilation::{ForeignQueryContext, ForeignQueryError, ForeignQueryFailure};
use crate::fact::diagnostic_context::{
    boolean_field, count_field, identity_field, natural_field, push_symbol, text_field,
};

pub(super) fn diagnostic_foreign_query_failure(
    error: &ForeignQueryError,
) -> DiagnosticForeignQueryFailure {
    use ForeignQueryFailure as Failure;

    let (reason, context) = match error.cause() {
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

const fn foreign_integer_width(width: crate::compilation::ForeignIntegerWidth) -> &'static str {
    match width {
        crate::compilation::ForeignIntegerWidth::U64 => "u64",
    }
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

#[cfg(test)]
mod tests {
    use bray_symbols::{FunctionSymbolId, SymbolId};

    use super::{diagnostic_foreign_query_failure, foreign_query_context};
    use crate::compilation::{ForeignQueryContext, ForeignQueryError, ForeignQueryFailure};

    #[test]
    fn foreign_query_conversion_uses_typed_function_and_cause_fields() {
        let error = ForeignQueryError::from(ForeignQueryFailure::ConflictingSourceRoles {
            function: FunctionSymbolId::from_symbol_id(SymbolId::new(13)),
            runtime: bray_runtime_interface::RuntimeAbiRole::RootExecution,
            platform: bray_runtime_interface::PlatformServiceRole::StandardOutputFlush,
        });

        let failure = diagnostic_foreign_query_failure(&error);

        assert_eq!(failure.as_str(), "foreign_query_conflicting_source_roles");

        assert!(
            failure
                .context()
                .iter()
                .any(|field| field.name() == "function")
        );

        assert!(
            failure
                .context()
                .iter()
                .any(|field| field.name() == "runtime_role")
        );

        assert!(
            failure
                .context()
                .iter()
                .any(|field| field.name() == "platform_role")
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
