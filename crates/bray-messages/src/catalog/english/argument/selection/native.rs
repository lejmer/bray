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
    match problem {
        bray_diagnostics::DiagnosticNativeSymbolDirectiveProblem::Argument(problem) => {
            format_english_directive_argument_problem(problem)
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
        0x0001 => "process context measurement service",
        0x0002 => "process context copy service",
        0x0003 => "environment key comparison service",
        0x0101 => "stream read service",
        0x0102 => "stream write service",
        0x0103 => "stream flush service",
        0x0104 => "stream seek service",
        0x0105 => "stream close service",
        0x0106 => "stream lock service",
        0x0107 => "stream unlock service",
        0x0201 => "file open service",
        0x0202 => "file metadata service",
        0x0203 => "path metadata service",
        0x0204 => "directory open service",
        0x0205 => "directory iteration service",
        0x0206 => "directory close service",
        0x0210 => "directory creation service",
        0x0211 => "file removal service",
        0x0212 => "directory removal service",
        0x0213 => "path rename service",
        0x0301 => "child process spawn service",
        0x0302 => "child process wait service",
        0x0303 => "child process termination service",
        0x0304 => "child process reap service",
        0x0305 => "child process disposal service",
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
