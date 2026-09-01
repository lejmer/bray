use super::super::source::format_english_artifact_digest;

pub(super) fn format_english_product_query_failure(
    failure: &bray_diagnostics::DiagnosticProductQueryFailure,
) -> String {
    use bray_diagnostics::DiagnosticProductQueryFailure as Failure;

    match failure {
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
            "an internal compiler error cannot use constant value '{value}' of category '{kind}' in specialization identity"
        ),
        Failure::GenericSubstitution {
            substitution,
            cause,
        } => match substitution {
            Some(substitution) => format!(
                "an internal compiler error found malformed generic substitution '{substitution}': {cause}"
            ),
            None => format!(
                "an internal compiler error found malformed generic substitution input: {cause}"
            ),
        },
        Failure::TraitApplicationMismatch {
            witness,
            expected,
            actual,
        } => format!(
            "an internal compiler error found trait application '{actual}' instead of '{expected}' for implementation witness '{witness}'"
        ),
        Failure::ConflictingImplementationWitness {
            identity,
            existing,
            actual,
        } => format!(
            "an internal compiler error mapped implementation instances '{existing}' and '{actual}' to code-generation witness '{identity}'"
        ),
        Failure::UnexpectedSemanticType {
            ty,
            expected,
            actual,
        } => format!(
            "an internal compiler error found semantic type data '{actual}' instead of {} for type '{ty}'",
            product_query_name(expected.as_str()),
        ),
        Failure::SynchronizationPoisoned { component } => format!(
            "an internal compiler error could not access coordinated product state '{component}'"
        ),
        Failure::StaticDependencyOverflow { static_instance } => format!(
            "an internal compiler error overflowed the lifecycle dependency count for static instance '{static_instance}'"
        ),
        Failure::StaticDependencyUnderflow { static_instance } => format!(
            "an internal compiler error underflowed the lifecycle dependency count for static instance '{static_instance}'"
        ),
        Failure::StaticLifecycleCycle { instances } => format!(
            "an internal compiler error found a static lifecycle cycle among [{}]",
            instances.join(", "),
        ),
        Failure::ConflictingConcreteInstance { key } => format!(
            "an internal compiler error produced conflicting realizations for code-generation instance '{key}'"
        ),
        Failure::ConflictingStaticRelocation { value } => format!(
            "an internal compiler error produced conflicting static relocations for constant value '{value}'"
        ),
        Failure::CallableDefinitionMismatch {
            instance,
            expected,
            actual,
        } => format!(
            "an internal compiler error found callable definition #{actual} instead of #{expected} for code-generation instance '{instance}'"
        ),
        Failure::TraitDefinitionMismatch {
            context,
            expected,
            actual,
        } => format!(
            "an internal compiler error found trait definition #{actual} instead of #{expected} for {} '{}'",
            product_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::InvalidCodegenSourceFile { source, path } => match source {
            Some(source) => format!(
                "an internal compiler error could not assign source #{source} at '{path}' a native-code file identity"
            ),
            None => format!(
                "an internal compiler error could not assign source path '{path}' a native-code file identity"
            ),
        },
        Failure::SourceIndex { source, cause } => {
            format!("an internal compiler error could not index source #{source}: {cause}")
        }
        Failure::ExternalSymbolIdentity { symbol, cause } => format!(
            "an internal compiler error could not construct external identity for symbol '{symbol}': {cause}"
        ),
        Failure::InvalidCompilerKnownDeclarationKey { key } => format!(
            "an internal compiler error found invalid compiler-known declaration key '{key}'"
        ),
        Failure::InvalidRecognizedStandardLibraryDeclarationKey { key } => format!(
            "an internal compiler error found invalid recognized standard-library declaration key '{key}'"
        ),
        Failure::InvalidPackageIdentity { identity } => {
            format!("an internal compiler error found invalid package identity '{identity}'")
        }
        Failure::UnexpectedEntryResult { actual } => format!(
            "an internal compiler error found unsupported executable entry result '{actual}'"
        ),
        Failure::CompilerKnownRepresentationMismatch {
            ty,
            expected,
            actual,
        } => format!(
            "an internal compiler error found compiler-known representation '{}' instead of '{expected}' for semantic type '{ty}'",
            actual.as_deref().unwrap_or("none"),
        ),
        Failure::NativeBoundaryKindMismatch { reference, actual } => format!(
            "an internal compiler error found native-boundary kind '{actual}' for static reference '{reference}'"
        ),
        Failure::UnexpectedSymbolKind {
            symbol,
            expected,
            actual,
        } => format!(
            "an internal compiler error found symbol category '{actual}' instead of '{expected}' for symbol '{symbol}'"
        ),
        Failure::ImplementationSymbolKeyExpected { key, actual } => format!(
            "an internal compiler error found symbol-key category '{actual}' instead of an implementation for key '{key}'"
        ),
        Failure::UnsupportedRuntimeRole { role } => {
            format!("an internal compiler error found unsupported runtime ABI role '{role}'")
        }
        Failure::InvalidHelperOperation {
            context,
            helper,
            operation,
        } => format!(
            "an internal compiler error found MIR operation '{operation}' incompatible with helper '{helper}' for {} '{}'",
            product_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::InvalidHelperCallTarget {
            context,
            helper,
            target,
        } => format!(
            "an internal compiler error found call target '{target}' incompatible with helper '{helper}' for {} '{}'",
            product_query_name(context.kind().as_str()),
            context.identity(),
        ),
        Failure::LifecycleRoleMismatch {
            instance,
            expected,
            actual,
        } => format!(
            "an internal compiler error found lifecycle role '{}' instead of '{}' for code-generation instance '{instance}'",
            actual.as_deref().unwrap_or("none"),
            expected.as_deref().unwrap_or("none"),
        ),
        Failure::BuiltInProofMismatch {
            requirement,
            actual,
        } => format!(
            "an internal compiler error found proof outcome '{}' for implementation requirement '{requirement}'",
            actual.as_deref().unwrap_or("none"),
        ),
        Failure::ImplementationSelectionMismatch {
            requirement,
            actual,
        } => format!(
            "an internal compiler error found implementation selection '{actual}' for requirement '{requirement}'"
        ),
        Failure::UnsupportedRuntimeDefaultSubject { provider, subject } => format!(
            "an internal compiler error found unsupported runtime-default subject '{subject}' for provider '{provider}'"
        ),
        Failure::InvalidCallableDefinitionSymbol { symbol, actual } => format!(
            "an internal compiler error cannot use symbol '{symbol}' of category '{actual}' as a callable definition"
        ),
        Failure::UnsupportedLifecycleRole { role } => {
            format!("an internal compiler error cannot emit lifecycle role '{role}'")
        }
        Failure::SourceSnapshotMismatch {
            source,
            expected,
            actual,
        } => format!(
            "an internal compiler error loaded source #{source} at version '{}' instead of '{expected}'",
            actual.as_deref().unwrap_or("unavailable"),
        ),
        Failure::TestProductMismatch {
            requested,
            compilation_package,
            compilation_kind,
        } => format!(
            "an internal compiler error requested tests for product '{requested}' from {compilation_kind} compilation package '{compilation_package}'"
        ),
        Failure::TestCatalog {
            product,
            identity,
            cause,
        } => format!(
            "an internal compiler error could not add test '{identity}' to the catalog for product '{product}': {cause}"
        ),
        Failure::InvalidTestErrorTypeIdentity { digest } => format!(
            "an internal compiler error could not construct the test error-type identity from digest {}",
            format_english_artifact_digest(digest),
        ),
        Failure::InvalidModulePath {
            declaration,
            segments,
        } => format!(
            "an internal compiler error could not construct a module path for declaration #{declaration} from [{}]",
            segments.join(", "),
        ),
        Failure::UnsupportedEntryResultType { ty, actual } => format!(
            "an internal compiler error cannot finalize entry result type '{ty}' with representation '{}'",
            actual.as_deref().unwrap_or("none"),
        ),
        Failure::InvalidCodegenRequest { unit, cause } => format!(
            "an internal compiler error could not construct the native-code request for work item {}: {cause}",
            format_english_artifact_digest(unit),
        ),
        Failure::CodegenBackendSelection { unit, cause } => format!(
            "an internal compiler error could not select the requested backend for native-code work item {}: {cause}",
            format_english_artifact_digest(unit),
        ),
    }
}

fn format_context_data_failure(
    action: &str,
    context: &bray_diagnostics::DiagnosticProductQueryContext,
    data: bray_diagnostics::DiagnosticProductDataKind,
) -> String {
    format!(
        "an internal compiler error {action} {} for {} '{}'",
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
        "an internal compiler error found {} instead of {} for {} '{}'",
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
        "an internal compiler error found {actual} {} values instead of {expected} for {} '{}'",
        product_query_name(data.as_str()),
        product_query_name(context.kind().as_str()),
        context.identity(),
    )
}

fn product_query_name(name: &str) -> String {
    name.replace('_', " ")
}
