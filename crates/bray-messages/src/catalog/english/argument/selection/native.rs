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
    match role.id() {
        0x0001 => "process identity service",
        0x0002 => "native text width service",
        0x0003 => "startup working directory service",
        0x0004 => "startup argument count service",
        0x0005 => "startup argument service",
        0x0006 => "startup environment count service",
        0x0007 => "startup environment entry service",
        0x0008 => "environment key comparison service",
        0x0101 => "standard input read operation",
        0x0102 => "standard input lock operation",
        0x0103 => "standard input unlock operation",
        0x0111 => "standard output write operation",
        0x0112 => "standard output flush operation",
        0x0113 => "standard output lock operation",
        0x0114 => "standard output unlock operation",
        0x0121 => "standard error write operation",
        0x0122 => "standard error flush operation",
        0x0123 => "standard error lock operation",
        0x0124 => "standard error unlock operation",
        0x0201 => "file read operation",
        0x0202 => "file write operation",
        0x0203 => "file flush operation",
        0x0204 => "file seek operation",
        0x0205 => "file close operation",
        0x0211 => "file open operation",
        0x0212 => "file metadata operation",
        0x0213 => "path metadata operation",
        0x0221 => "directory open operation",
        0x0222 => "directory iteration operation",
        0x0223 => "directory close operation",
        0x0230 => "directory creation operation",
        0x0231 => "file removal operation",
        0x0232 => "directory removal operation",
        0x0233 => "path rename operation",
        0x0301 => "child process pipe read operation",
        0x0302 => "child process pipe write operation",
        0x0303 => "child process pipe flush operation",
        0x0304 => "child process pipe close operation",
        0x0311 => "child process spawn operation",
        0x0312 => "child process wait operation",
        0x0313 => "child process termination operation",
        0x0314 => "child process reap operation",
        0x0315 => "child process disposal operation",
        0x0401 => "monotonic clock service",
        0x0402 => "wall clock service",
        0x0403 => "clock sleep service",
        0x0501 => "entropy service",
        0x0701 => "date validation service",
        0x0702 => "date addition service",
        0x0710 => "timezone loading service",
        0x0711 => "local timezone service",
        0x0712 => "timezone retain service",
        0x0713 => "timezone close service",
        0x0714 => "timezone name service",
        0x0720 => "time observation service",
        0x0721 => "local time resolution service",
        0x0730 => "time parsing service",
        0x0731 => "time formatting service",
        0x0801 => "dynamic library path-open service",
        0x0802 => "system library open service",
        0x0803 => "dynamic library symbol service",
        0x0804 => "dynamic library close service",
        _ => unreachable!("platform-service roles are validated before diagnostic construction"),
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
