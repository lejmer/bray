use bray_diagnostics::{
    DiagnosticForeignDataKind, DiagnosticForeignQueryContext, DiagnosticForeignQueryContextKind,
    DiagnosticForeignQueryFailure,
};

use crate::compilation::foreign::ForeignSourceRole;
use crate::compilation::{
    ForeignDataKind, ForeignQueryContext, ForeignQueryError, ForeignQueryFailure, ForeignTypeKind,
};

pub(in crate::compilation) fn diagnostic_foreign_query_failure(
    error: &ForeignQueryError,
) -> DiagnosticForeignQueryFailure {
    match error.cause() {
        ForeignQueryFailure::Missing { context, data } => DiagnosticForeignQueryFailure::Missing {
            context: diagnostic_foreign_context(context),
            data: diagnostic_foreign_data_kind(*data),
        },
        ForeignQueryFailure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => DiagnosticForeignQueryFailure::CountMismatch {
            context: diagnostic_foreign_context(context),
            data: diagnostic_foreign_data_kind(*data),
            expected: *expected,
            actual: *actual,
        },
        ForeignQueryFailure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => DiagnosticForeignQueryFailure::UnexpectedSemanticType {
            ty: format!("{ty:?}"),
            expected: diagnostic_foreign_type_kind(*expected),
            actual: format!("{actual:?}"),
        },
        ForeignQueryFailure::UnexpectedTypeTemplate {
            context,
            expected,
            actual,
        } => DiagnosticForeignQueryFailure::UnexpectedTypeTemplate {
            context: diagnostic_foreign_context(context),
            expected: diagnostic_foreign_type_kind(*expected),
            actual: format!("{actual:?}"),
        },
        ForeignQueryFailure::UnexpectedGenericArgument {
            substitution,
            expected,
            actual,
        } => DiagnosticForeignQueryFailure::UnexpectedGenericArgument {
            substitution: format!("{substitution:?}"),
            expected: format!("{expected:?}"),
            actual: format!("{actual:?}"),
        },
        ForeignQueryFailure::NumericOverflow {
            context,
            value,
            target,
        } => DiagnosticForeignQueryFailure::NumericOverflow {
            context: diagnostic_foreign_context(context),
            value: *value,
            target: format!("{target:?}"),
        },
        ForeignQueryFailure::InvalidPlatformServiceRole { role } => {
            DiagnosticForeignQueryFailure::InvalidPlatformServiceRole {
                role: role.as_str().to_owned(),
            }
        }
        ForeignQueryFailure::CallableSignature { function, cause } => {
            DiagnosticForeignQueryFailure::CallableSignature {
                function: format!("{function:?}"),
                cause: format!("{cause:?}"),
            }
        }
        ForeignQueryFailure::ConflictingSourceRoles {
            function,
            runtime,
            platform,
        } => DiagnosticForeignQueryFailure::ConflictingSourceRoles {
            function: format!("{function:?}"),
            runtime: runtime.as_str().to_owned(),
            platform: platform.as_str().to_owned(),
        },
        ForeignQueryFailure::DuplicateSourceRole {
            function,
            first,
            duplicate,
        } => DiagnosticForeignQueryFailure::DuplicateSourceRole {
            function: format!("{function:?}"),
            first: diagnostic_source_role(*first),
            duplicate: diagnostic_source_role(*duplicate),
        },
    }
}

fn diagnostic_foreign_context(context: &ForeignQueryContext) -> DiagnosticForeignQueryContext {
    let kind = match context {
        ForeignQueryContext::Symbol(_) => DiagnosticForeignQueryContextKind::Symbol,
        ForeignQueryContext::Function(_) => DiagnosticForeignQueryContextKind::Function,
        ForeignQueryContext::Static(_) => DiagnosticForeignQueryContextKind::Static,
        ForeignQueryContext::Directive(_) => DiagnosticForeignQueryContextKind::Directive,
        ForeignQueryContext::Source(_) => DiagnosticForeignQueryContextKind::Source,
        ForeignQueryContext::Substitution(_) => DiagnosticForeignQueryContextKind::Substitution,
        ForeignQueryContext::CompilerKnownRepresentation { .. } => {
            DiagnosticForeignQueryContextKind::CompilerKnownRepresentation
        }
        ForeignQueryContext::PlatformService(_) => {
            DiagnosticForeignQueryContextKind::PlatformService
        }
    };

    DiagnosticForeignQueryContext::new(kind, format!("{context:?}"))
}

const fn diagnostic_foreign_data_kind(kind: ForeignDataKind) -> DiagnosticForeignDataKind {
    match kind {
        ForeignDataKind::ContainingModule => DiagnosticForeignDataKind::ContainingModule,
        ForeignDataKind::SourceAnchor => DiagnosticForeignDataKind::SourceAnchor,
        ForeignDataKind::FunctionBindingRecord => DiagnosticForeignDataKind::FunctionBindingRecord,
        ForeignDataKind::StaticBindingRecord => DiagnosticForeignDataKind::StaticBindingRecord,
        ForeignDataKind::FunctionDeclarationSyntax => {
            DiagnosticForeignDataKind::FunctionDeclarationSyntax
        }
        ForeignDataKind::StaticDeclarationSyntax => {
            DiagnosticForeignDataKind::StaticDeclarationSyntax
        }
        ForeignDataKind::SourceSnapshot => DiagnosticForeignDataKind::SourceSnapshot,
        ForeignDataKind::SourceText => DiagnosticForeignDataKind::SourceText,
        ForeignDataKind::StructureRecord => DiagnosticForeignDataKind::StructureRecord,
        ForeignDataKind::UnionRecord => DiagnosticForeignDataKind::UnionRecord,
        ForeignDataKind::UnionVariantRecord => DiagnosticForeignDataKind::UnionVariantRecord,
        ForeignDataKind::UnaryRepresentationArgument => {
            DiagnosticForeignDataKind::UnaryRepresentationArgument
        }
        ForeignDataKind::RepresentationSymbol => DiagnosticForeignDataKind::RepresentationSymbol,
    }
}

fn diagnostic_foreign_type_kind(kind: ForeignTypeKind) -> String {
    match kind {
        ForeignTypeKind::Callable => "callable".to_owned(),
    }
}

fn diagnostic_source_role(role: ForeignSourceRole) -> String {
    match role {
        ForeignSourceRole::Runtime(role) => format!("runtime:{}", role.as_str()),
        ForeignSourceRole::Platform(role) => format!("platform:{}", role.as_str()),
    }
}
