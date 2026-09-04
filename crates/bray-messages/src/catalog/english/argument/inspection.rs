pub(super) fn format_english_inspection_failure(
    failure: &bray_diagnostics::DiagnosticInspectionFailure,
) -> String {
    use bray_diagnostics::DiagnosticInspectionFailure as Failure;

    match failure {
        Failure::Source { format, cause } => format!(
            "{} source inspection {}",
            inspection_format(*format),
            source_inspection_cause(cause)
        ),
        Failure::Token { format, cause } => format!(
            "{} token inspection {}",
            inspection_format(*format),
            token_inspection_cause(cause)
        ),
        Failure::Syntax { format, cause } => format!(
            "{} syntax inspection {}",
            inspection_format(*format),
            syntax_inspection_cause(cause)
        ),
        Failure::Declaration { format, cause } => {
            format!(
                "{} declaration inspection {}",
                inspection_format(*format),
                declaration_inspection_cause(cause)
            )
        }
        Failure::Symbol { format, cause } => format!(
            "{} symbol inspection {}",
            inspection_format(*format),
            symbol_inspection_cause(cause)
        ),
        Failure::Bound {
            target,
            format,
            cause,
        } => format!(
            "{} bound-tree inspection for {} {}",
            inspection_format(*format),
            inspection_target(*target),
            bound_inspection_cause(cause)
        ),
        Failure::Lowered {
            target,
            format,
            cause,
        } => {
            format!(
                "{} lowered-tree inspection for {} {}",
                inspection_format(*format),
                inspection_target(*target),
                lowered_inspection_cause(cause)
            )
        }
        Failure::Mir {
            target,
            format,
            cause,
        } => format!(
            "{} MIR inspection for {} {}",
            inspection_format(*format),
            inspection_target(*target),
            lowered_inspection_cause(cause)
        ),
    }
}

const fn inspection_format(
    format: bray_diagnostics::DiagnosticInspectionOutputFormat,
) -> &'static str {
    use bray_diagnostics::DiagnosticInspectionOutputFormat as Format;

    match format {
        Format::Text => "text",
        Format::Json => "JSON",
    }
}

fn inspection_target(target: bray_diagnostics::DiagnosticInspectionTarget) -> String {
    target.position.map_or_else(
        || format!("source {}", target.source_id),
        |position| format!("source {} at byte {position}", target.source_id),
    )
}

const fn source_inspection_cause(
    cause: &bray_diagnostics::DiagnosticSourceInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticSourceInspectionFailure as Cause;

    match cause {
        Cause::SourceIndex => "could not index source lines",
        Cause::Json => "could not serialize JSON",
        Cause::Detail(_) => inspection_detail_cause(),
    }
}

const fn token_inspection_cause(
    cause: &bray_diagnostics::DiagnosticTokenInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticTokenInspectionFailure as Cause;

    match cause {
        Cause::SourceIndex => "could not index source lines",
        Cause::TokenText => "could not correlate token text with its source",
        Cause::TriviaText => "could not correlate trivia text with its source",
        Cause::Json => "could not serialize JSON",
        Cause::Detail(_) => inspection_detail_cause(),
    }
}

const fn syntax_inspection_cause(
    cause: &bray_diagnostics::DiagnosticSyntaxInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticSyntaxInspectionFailure as Cause;

    match cause {
        Cause::SourceIndex => "could not index source lines",
        Cause::SourceMismatch => "found a syntax result owned by another source",
        Cause::TokenText => "could not correlate token text with its source",
        Cause::TriviaText => "could not correlate trivia text with its source",
        Cause::TreeStructure => "found an invalid syntax-tree traversal",
        Cause::Json => "could not serialize JSON",
        Cause::Detail(_) => inspection_detail_cause(),
    }
}

const fn declaration_inspection_cause(
    cause: &bray_diagnostics::DiagnosticDeclarationInspectionFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticDeclarationInspectionFailure as Cause;

    match cause {
        Cause::Container => "found an invalid declaration container reference",
        Cause::Declaration => "found an invalid declaration reference",
        Cause::ModulePart => "found an invalid module-part reference",
        Cause::Source => "could not correlate a declaration with source",
        Cause::SourceIndex => "could not index source lines",
        Cause::Json => "could not serialize JSON",
        Cause::Detail(_) => inspection_detail_cause(),
    }
}

fn symbol_inspection_cause(cause: &bray_diagnostics::DiagnosticSymbolInspectionFailure) -> String {
    use bray_diagnostics::DiagnosticSymbolInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => {
            return super::native::format_english_emission_evaluation_failure(failure);
        }
        Cause::Declaration => "found an invalid declaration reference",
        Cause::Json => "could not serialize JSON",
        Cause::Source => "could not correlate a symbol with source",
        Cause::SourceIndex => "could not index source lines",
        Cause::Symbol => "found an invalid symbol reference",
        Cause::SymbolCycle => "found a cycle in symbol identity traversal",
        Cause::Type => "could not represent a semantic type",
        Cause::UnsupportedRelationship => "found an unsupported relationship category",
        Cause::Detail(_) => inspection_detail_cause(),
    }
    .to_owned()
}

fn bound_inspection_cause(cause: &bray_diagnostics::DiagnosticBoundInspectionFailure) -> String {
    use bray_diagnostics::DiagnosticBoundInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => {
            return super::native::format_english_emission_evaluation_failure(failure);
        }
        Cause::Json => "could not serialize JSON",
        Cause::MissingNode => "could not find a selected bound node",
        Cause::Source => "could not correlate a bound node with source",
        Cause::SourceIndex => "could not index source lines",
        Cause::Symbol => "found an invalid symbol reference",
        Cause::Type => "could not represent a semantic type",
        Cause::Selection => "could not represent a semantic selection",
        Cause::Detail(_) => inspection_detail_cause(),
    }
    .to_owned()
}

fn lowered_inspection_cause(
    cause: &bray_diagnostics::DiagnosticLoweredInspectionFailure,
) -> String {
    use bray_diagnostics::DiagnosticLoweredInspectionFailure as Cause;

    match cause {
        Cause::Evaluation(failure) => {
            return super::native::format_english_emission_evaluation_failure(failure);
        }
        Cause::Json => "could not serialize JSON",
        Cause::Model => "could not construct the MIR report model",
        Cause::Source => "could not correlate a lowered node with source",
        Cause::Detail(_) => inspection_detail_cause(),
    }
    .to_owned()
}

const fn inspection_detail_cause() -> &'static str {
    "failed because an internal report operation could not complete"
}

pub(super) fn format_english_native_linker_build_failure(
    failure: bray_diagnostics::DiagnosticNativeLinkerBuildFailure,
) -> String {
    use bray_diagnostics::DiagnosticNativeLinkerBuildFailure as Failure;

    match failure {
        Failure::ArchiveIdentity => "archive-driver identity".to_owned(),
        Failure::ArchiveDriverKindMismatch => "archive-driver category".to_owned(),
        Failure::ArchiveCapabilities(cause) => format!(
            "archive-driver capability ({})",
            format_english_linker_capability_build_failure(cause)
        ),
        Failure::ArchiveProgramPathNotExplicit => "archive-driver program path".to_owned(),
        Failure::ArchiveInvocation(cause) => format!(
            "archive-driver invocation ({})",
            format_english_invocation_build_failure(cause)
        ),
        Failure::SystemIdentity => "system-linker identity".to_owned(),
        Failure::TargetIdentity => "linker target identity".to_owned(),
        Failure::SystemProgramPathNotExplicit => "system-linker program path".to_owned(),
        Failure::SystemInvocation(cause) => format!(
            "system-linker invocation ({})",
            format_english_invocation_build_failure(cause)
        ),
        Failure::SystemThinLtoCacheRootEmpty => "system-linker ThinLTO cache root".to_owned(),
        Failure::SystemDriverKindMismatch => "system-linker category".to_owned(),
        Failure::SystemCapabilities(cause) => format!(
            "system-linker capability ({})",
            format_english_linker_capability_build_failure(cause)
        ),
        Failure::DuplicateDriver => "linker-driver registry".to_owned(),
    }
}

const fn format_english_linker_capability_build_failure(
    failure: bray_diagnostics::DiagnosticLinkerCapabilityBuildFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticLinkerCapabilityBuildFailure as Failure;

    match failure {
        Failure::DriverKindMismatch => "the driver category cannot own this capability",
        Failure::UnsupportedTarget => "the command family does not support the target",
        Failure::MissingTargets => "no supported target is declared",
        Failure::DuplicateTarget => "an architecture and object format are declared twice",
    }
}

const fn format_english_invocation_build_failure(
    failure: bray_diagnostics::DiagnosticInvocationBuildFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticInvocationBuildFailure as Failure;

    match failure {
        Failure::EmptyProgram => "the program path is empty",
        Failure::EmptyCurrentDirectory => "the working directory is empty",
        Failure::EmptyEnvironmentVariableName => "an environment variable name is empty",
        Failure::DuplicateEnvironmentVariableName => {
            "an environment variable name occurs more than once"
        }
        Failure::DuplicateResponseFile => "a response-file path occurs more than once",
    }
}
