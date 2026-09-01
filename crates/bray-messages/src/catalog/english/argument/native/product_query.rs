use super::super::source::format_english_artifact_digest;

pub(super) fn format_english_product_query_failure(
    failure: &bray_diagnostics::DiagnosticProductQueryFailure,
) -> String {
    use bray_diagnostics::DiagnosticProductQueryFailure as Failure;

    let detail = match failure {
        Failure::Missing { context, data } => {
            format_context_data_failure("could not obtain", context, *data)
        }
        Failure::Conflict { context, data } => {
            format_context_data_failure("found conflicting", context, *data)
        }
        Failure::UnexpectedKind {
            context,
            expected,
            actual,
        } => format_unexpected_kind(context, *expected, *actual),
        Failure::CountMismatch {
            context,
            data,
            expected,
            actual,
        } => format_count_mismatch(context, *data, *expected, *actual),
        Failure::UnsupportedConstantValue { value, kind } => format!(
            "cannot use constant value '{value}' of category '{kind}' in specialization identity"
        ),
        Failure::GenericSubstitution {
            substitution,
            cause,
        } => match substitution {
            Some(substitution) => {
                format!("found malformed generic substitution '{substitution}': {cause}")
            }
            None => format!("found malformed generic substitution input: {cause}"),
        },
        Failure::TraitApplicationMismatch {
            witness: implementation_instance,
            expected,
            actual,
        } => format!(
            "found trait application '{actual}' instead of '{expected}' for implementation instance '{implementation_instance}'"
        ),
        Failure::ConflictingImplementationWitness {
            identity,
            existing,
            actual,
        } => format!(
            "mapped implementation instances '{existing}' and '{actual}' to the same code-generation identity '{identity}'"
        ),
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => format!(
            "found semantic type data '{actual}' instead of {} for type '{ty}'",
            product_query_name(expected.as_str()),
        ),
        Failure::SynchronizationPoisoned { component } => {
            format!("could not access coordinated product state '{component}'")
        }
        Failure::StaticDependencyOverflow { static_instance } => format!(
            "overflowed the lifecycle dependency count for static instance '{static_instance}'"
        ),
        Failure::StaticDependencyUnderflow { static_instance } => format!(
            "underflowed the lifecycle dependency count for static instance '{static_instance}'"
        ),
        Failure::StaticLifecycleCycle { instances } => format!(
            "found a static lifecycle cycle among [{}]",
            instances.join(", "),
        ),
        Failure::ConflictingConcreteInstance { key } => {
            format!("produced conflicting realizations for code-generation instance '{key}'")
        }
        Failure::ConflictingStaticRelocation { value } => {
            format!("produced conflicting static relocations for constant value '{value}'")
        }
        Failure::CallableDefinitionMismatch {
            instance,
            expected,
            actual,
        } => format!(
            "found callable definition #{actual} instead of #{expected} for code-generation instance '{instance}'"
        ),
        Failure::TraitDefinitionMismatch {
            context,
            expected,
            actual,
        } => format!(
            "found trait definition #{actual} instead of #{expected} for {} '{}'",
            product_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::InvalidCodegenSourceFile { source, path } => match source {
            Some(source) => {
                format!("could not assign source #{source} at '{path}' a native-code file identity")
            }
            None => format!("could not assign source path '{path}' a native-code file identity"),
        },
        Failure::SourceIndex { source, cause } => {
            format!("could not index source #{source}: {cause}")
        }
        Failure::ExternalSymbolIdentity { symbol, cause } => {
            format!("could not construct external identity for symbol '{symbol}': {cause}")
        }
        Failure::InvalidCompilerKnownDeclarationKey { key } => {
            format!("found invalid compiler-known declaration key '{key}'")
        }
        Failure::InvalidRecognizedStandardLibraryDeclarationKey { key } => {
            format!("found invalid recognized standard-library declaration key '{key}'")
        }
        Failure::InvalidPackageIdentity { identity } => {
            format!("found invalid package identity '{identity}'")
        }
        Failure::UnexpectedEntryResult { actual } => {
            format!("found unsupported executable entry result '{actual}'")
        }
        Failure::CompilerKnownRepresentationMismatch {
            ty,
            expected,
            actual,
        } => format!(
            "found compiler-known representation '{}' instead of '{expected}' for semantic type '{ty}'",
            actual.as_deref().unwrap_or("none"),
        ),
        Failure::NativeBoundaryKindMismatch { reference, actual } => {
            format!("found native-boundary kind '{actual}' for static reference '{reference}'")
        }
        Failure::UnexpectedSymbolKind {
            symbol,
            expected,
            actual,
        } => format!(
            "found symbol category '{actual}' instead of '{expected}' for symbol '{symbol}'"
        ),
        Failure::ImplementationSymbolKeyExpected { key, actual } => format!(
            "found symbol-key category '{actual}' instead of an implementation for key '{key}'"
        ),
        Failure::UnsupportedRuntimeRole { role } => {
            format!("found unsupported runtime ABI role '{role}'")
        }
        Failure::InvalidHelperOperation {
            context,
            helper,
            operation,
        } => format!(
            "found MIR operation '{operation}' incompatible with helper '{helper}' for {} '{}'",
            product_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::InvalidHelperCallTarget {
            context,
            helper,
            target,
        } => format!(
            "found call target '{target}' incompatible with helper '{helper}' for {} '{}'",
            product_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::LifecycleRoleMismatch {
            instance,
            expected,
            actual,
        } => format!(
            "found lifecycle role '{}' instead of '{}' for code-generation instance '{instance}'",
            actual.as_deref().unwrap_or("none"),
            expected.as_deref().unwrap_or("none"),
        ),
        Failure::BuiltInProofMismatch {
            requirement,
            actual,
        } => format!(
            "found proof outcome '{}' for implementation requirement '{requirement}'",
            actual.as_deref().unwrap_or("none"),
        ),
        Failure::ImplementationSelectionMismatch {
            requirement,
            actual,
        } => format!("found implementation selection '{actual}' for requirement '{requirement}'"),
        Failure::UnsupportedRuntimeDefaultSubject { provider, subject } => format!(
            "found unsupported runtime-default subject '{subject}' for provider '{provider}'"
        ),
        Failure::InvalidCallableDefinitionSymbol { symbol, actual } => {
            format!("cannot use symbol '{symbol}' of category '{actual}' as a callable definition")
        }
        Failure::UnsupportedLifecycleRole { role } => {
            format!("cannot emit lifecycle role '{role}'")
        }
        Failure::SourceSnapshotMismatch {
            source,
            expected,
            actual,
        } => format!(
            "loaded source #{source} at version '{}' instead of '{expected}'",
            actual.as_deref().unwrap_or("unavailable"),
        ),
        Failure::TestProductMismatch {
            requested,
            compilation_package,
            compilation_kind,
        } => format!(
            "requested tests for product '{requested}' from {compilation_kind} compilation package '{compilation_package}'"
        ),
        Failure::TestCatalog {
            product,
            identity,
            cause,
        } => format!(
            "could not add test '{identity}' to the catalog for product '{product}': {cause}"
        ),
        Failure::InvalidTestErrorTypeIdentity { digest } => format!(
            "could not construct the test error-type identity from digest {}",
            format_english_artifact_digest(digest),
        ),
        Failure::InvalidModulePath {
            declaration,
            segments,
        } => format!(
            "could not construct a module path for declaration #{declaration} from [{}]",
            segments.join(", "),
        ),
        Failure::UnsupportedEntryResultType { ty, actual } => format!(
            "cannot finalize entry result type '{ty}' with representation '{}'",
            actual.as_deref().unwrap_or("none"),
        ),
        Failure::InvalidCodegenRequest { unit, cause } => format!(
            "could not construct the native-code request for work item {}: {cause}",
            format_english_artifact_digest(unit),
        ),
        Failure::CodegenBackendSelection { unit, cause } => format!(
            "could not select the requested backend for native-code work item {}: {cause}",
            format_english_artifact_digest(unit),
        ),
    };

    super::format_internal_compiler_error(detail)
}

fn format_context_data_failure(
    action: &str,
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    data: bray_diagnostics::DiagnosticProductDataKind,
) -> String {
    format!(
        "{action} {} for {} '{}'",
        product_query_name(data.as_str()),
        product_query_name(context.kind().as_str()),
        context.identity(),
    )
}

fn format_unexpected_kind(
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    expected: bray_diagnostics::DiagnosticProductValueKind,
    actual: bray_diagnostics::DiagnosticProductValueKind,
) -> String {
    format!(
        "found {} instead of {} for {} '{}'",
        product_query_name(actual.as_str()),
        product_query_name(expected.as_str()),
        product_query_name(context.kind().as_str()),
        context.identity(),
    )
}

fn format_count_mismatch(
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    data: bray_diagnostics::DiagnosticProductDataKind,
    expected: usize,
    actual: usize,
) -> String {
    format!(
        "found {actual} {} values instead of {expected} for {} '{}'",
        product_query_name(data.as_str()),
        product_query_name(context.kind().as_str()),
        context.identity(),
    )
}

fn product_query_name(name: &str) -> String {
    name.replace('_', " ")
}
