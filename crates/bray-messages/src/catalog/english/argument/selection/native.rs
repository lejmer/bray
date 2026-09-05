use super::super::callable::format_english_callable_execution;
use super::super::source::format_english_type;
use super::kind::format_english_callable_abi;

pub(crate) fn format_english_native_link_directive_problem(
    problem: &bray_diagnostics::DiagnosticNativeLinkDirectiveProblem,
) -> String {
    use bray_diagnostics::DiagnosticNativeLinkDirectiveProblem as Problem;

    match problem {
        Problem::Argument(problem) => format_english_directive_argument_problem(problem),
        Problem::UnsupportedKind { provided } => {
            format!("link kind {provided} is not supported")
        }
        Problem::AmbiguousInput {
            name,
            kind,
            matches,
        } => {
            let kind = kind.map_or_else(String::new, |kind| {
                format!(" with kind {}", format_english_native_link_kind(kind))
            });

            format!("native input {name}{kind} matches {matches} configured inputs")
        }
    }
}

pub(crate) fn format_english_native_symbol_directive_problem(
    problem: &bray_diagnostics::DiagnosticNativeSymbolDirectiveProblem,
) -> String {
    use bray_diagnostics::DiagnosticNativeSymbolDirectiveProblem as Problem;

    match problem {
        Problem::Argument(problem) => format_english_directive_argument_problem(problem),
        Problem::MissingIdentity => String::from("one of name or ordinal is required"),
        Problem::ConflictingIdentity => {
            String::from("name and ordinal select different identities")
        }
        Problem::UnsupportedValue { name, provided } => {
            format!("argument {name} has unsupported value {provided}")
        }
        Problem::UnsupportedTargetOption { name } => {
            format!("the selected target does not support argument {name}")
        }
        Problem::IncompatiblePolicy { name } => {
            format!("argument {name} is unavailable for this native boundary")
        }
    }
}

fn format_english_directive_argument_problem(
    problem: &bray_diagnostics::DiagnosticDirectiveArgumentProblem,
) -> String {
    use bray_diagnostics::DiagnosticDirectiveArgumentProblem as Problem;

    match problem {
        Problem::Positional { ordinal } => format!(
            "argument {} is positional but must be named",
            ordinal.saturating_add(1)
        ),
        Problem::Unknown { name } => format!("argument name {name} is not accepted"),
        Problem::Duplicate { name } => format!("argument {name} is supplied more than once"),
        Problem::Missing { name } => format!("required argument {name} is missing"),
        Problem::EmptyString { name } => format!("argument {name} is an empty string"),
    }
}

const fn format_english_native_link_kind(
    kind: bray_diagnostics::DiagnosticNativeLinkKind,
) -> &'static str {
    use bray_diagnostics::DiagnosticNativeLinkKind;

    match kind {
        DiagnosticNativeLinkKind::Dynamic => "dynamic library",
        DiagnosticNativeLinkKind::Static => "static library",
        DiagnosticNativeLinkKind::System => "system library",
        DiagnosticNativeLinkKind::Framework => "framework",
    }
}

pub(crate) fn format_english_platform_service_signature_problem(
    problem: &bray_diagnostics::DiagnosticPlatformServiceSignatureProblem,
) -> String {
    use bray_diagnostics::DiagnosticPlatformServiceSignatureProblem as Problem;

    match problem {
        Problem::CallableAbi { role, actual } => format!(
            "{} uses the {} ABI instead of the required C ABI",
            format_english_platform_service_role(*role),
            format_english_callable_abi(*actual)
        ),
        Problem::Execution { role, actual } => format!(
            "{} is {} instead of synchronous",
            format_english_platform_service_role(*role),
            format_english_callable_execution(*actual)
        ),
        Problem::ParameterCount {
            role,
            expected,
            actual,
        } => format!(
            "{} has {actual} parameters but requires {expected}",
            format_english_platform_service_role(*role)
        ),
        Problem::ParameterType {
            role,
            ordinal,
            expected,
            actual,
        } => format!(
            "{} parameter {} has type {} but requires {}",
            format_english_platform_service_role(*role),
            ordinal.saturating_add(1),
            format_english_type(actual),
            format_english_platform_abi_type(*expected)
        ),
        Problem::ResultType {
            role,
            expected,
            actual,
        } => format!(
            "{} result has type {} but requires {}",
            format_english_platform_service_role(*role),
            format_english_type(actual),
            format_english_platform_abi_type(*expected)
        ),
    }
}

fn format_english_platform_service_role(
    role: bray_diagnostics::DiagnosticPlatformServiceRole,
) -> &'static str {
    use bray_diagnostics::DiagnosticPlatformServiceRole as Role;

    match role {
        Role::ContextIdentity => "process identity service",
        Role::ContextNativeTextWidth => "native text width service",
        Role::ContextWorkingDirectory => "startup working directory service",
        Role::ContextArgumentCount => "startup argument count service",
        Role::ContextArgument => "startup argument service",
        Role::ContextEnvironmentCount => "startup environment count service",
        Role::ContextEnvironmentEntry => "startup environment entry service",
        Role::ContextEnvironmentKeyEquals => "environment key comparison service",
        Role::StandardInputRead => "standard input read operation",
        Role::StandardInputLock => "standard input lock operation",
        Role::StandardInputUnlock => "standard input unlock operation",
        Role::StandardOutputWrite => "standard output write operation",
        Role::StandardOutputFlush => "standard output flush operation",
        Role::StandardOutputLock => "standard output lock operation",
        Role::StandardOutputUnlock => "standard output unlock operation",
        Role::StandardErrorWrite => "standard error write operation",
        Role::StandardErrorFlush => "standard error flush operation",
        Role::StandardErrorLock => "standard error lock operation",
        Role::StandardErrorUnlock => "standard error unlock operation",
        Role::FileRead => "file read operation",
        Role::FileWrite => "file write operation",
        Role::FileFlush => "file flush operation",
        Role::FileSeek => "file seek operation",
        Role::FileClose => "file close operation",
        Role::FileOpen => "file open operation",
        Role::FileMetadata => "file metadata operation",
        Role::PathMetadata => "path metadata operation",
        Role::DirectoryOpen => "directory open operation",
        Role::DirectoryNext => "directory iteration operation",
        Role::DirectoryClose => "directory close operation",
        Role::PathCreateDirectory => "directory creation operation",
        Role::PathRemoveFile => "file removal operation",
        Role::PathRemoveDirectory => "directory removal operation",
        Role::PathRename => "path rename operation",
        Role::ProcessPipeRead => "child process pipe read operation",
        Role::ProcessPipeWrite => "child process pipe write operation",
        Role::ProcessPipeFlush => "child process pipe flush operation",
        Role::ProcessPipeClose => "child process pipe close operation",
        Role::ChildSpawn => "child process spawn operation",
        Role::ChildWait => "child process wait operation",
        Role::ChildTerminate => "child process termination operation",
        Role::ChildReap => "child process reap operation",
        Role::ChildDispose => "child process disposal operation",
        Role::ThreadCreate => "thread creation operation",
        Role::ThreadJoin => "thread join operation",
        Role::ThreadDetach => "thread detachment operation",
        Role::ThreadStorageCreate => "thread storage creation operation",
        Role::ThreadStorageLoad => "thread storage load operation",
        Role::ThreadStorageStore => "thread storage store operation",
        Role::ThreadStorageDestroy => "thread storage destruction operation",
        Role::ClockMonotonicNow => "monotonic clock service",
        Role::ClockWallNow => "wall clock service",
        Role::ClockSleep => "clock sleep service",
        Role::EntropyFill => "entropy service",
        Role::TimeDateValidate => "date validation service",
        Role::TimeDateAdd => "date addition service",
        Role::TimeZoneLoad => "timezone loading service",
        Role::TimeZoneLocal => "local timezone service",
        Role::TimeZoneRetain => "timezone retain service",
        Role::TimeZoneClose => "timezone close service",
        Role::TimeZoneName => "timezone name service",
        Role::TimeObserve => "time observation service",
        Role::TimeResolve => "local time resolution service",
        Role::TimeParse => "time parsing service",
        Role::TimeFormat => "time formatting service",
        Role::DynamicLibraryOpenPath => "dynamic library path-open service",
        Role::DynamicLibraryOpenSystem => "system library open service",
        Role::DynamicLibrarySymbol => "dynamic library symbol service",
        Role::DynamicLibraryClose => "dynamic library close service",
    }
}

const fn format_english_platform_abi_type(
    ty: bray_diagnostics::DiagnosticPlatformAbiType,
) -> &'static str {
    use bray_diagnostics::DiagnosticPlatformAbiType as Type;

    match ty {
        Type::I32 => "a 32-bit signed integer",
        Type::U32 => "a 32-bit unsigned integer",
        Type::U64 => "a 64-bit unsigned integer",
        Type::I64 => "a 64-bit signed integer",
        Type::PointerU8 => "a raw pointer to bytes",
        Type::PointerU32 => "a raw pointer to 32-bit unsigned integers",
        Type::PointerU64 => "a raw pointer to 64-bit unsigned integers",
        Type::PointerI64 => "a raw pointer to 64-bit signed integers",
        Type::RawAddressPointer => "a raw address output pointer",
        Type::Path => "a platform path value",
        Type::NativeText => "a platform text value",
        Type::FileOptions => "a file options value",
        Type::FileMetadataPointer => "a file metadata output pointer",
        Type::ChildRequest => "a child process request value",
        Type::ExitStatusPointer => "a child exit-status output pointer",
        Type::TemporalDateTime => "a date-time value",
        Type::TemporalDateTimePointer => "a date-time pointer",
        Type::TemporalObservationPointer => "a timezone observation pointer",
        Type::TemporalResolutionPointer => "a local-time resolution pointer",
        Type::TemporalValue => "a temporal value",
        Type::TemporalValuePointer => "a temporal value pointer",
        Type::Status => "a platform status value",
    }
}
