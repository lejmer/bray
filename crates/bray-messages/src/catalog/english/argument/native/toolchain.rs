use super::super::source::{
    format_english_io_error_kind, format_english_path, format_english_quoted_text,
};
use bray_diagnostics::{DiagnosticLinkerDriverIdentity, DiagnosticLinkerDriverKind};

pub(crate) fn format_english_linker_driver_identity(
    identity: &DiagnosticLinkerDriverIdentity,
) -> String {
    let kind = match identity.kind() {
        DiagnosticLinkerDriverKind::EmbeddedLld => "embedded LLD",
        DiagnosticLinkerDriverKind::ExternalLld => "external LLD",
        DiagnosticLinkerDriverKind::System => "system linker",
        DiagnosticLinkerDriverKind::Archiver => "archiver",
        DiagnosticLinkerDriverKind::TargetSpecific => "target-specific linker",
    };

    format!(
        "{} ({kind}, capability revision {}, toolchain revision {})",
        format_english_quoted_text(identity.name()),
        format_english_quoted_text(identity.capability_revision()),
        format_english_quoted_text(identity.toolchain_revision()),
    )
}

pub(crate) fn format_english_unsupported_emission_reason(
    reason: &bray_diagnostics::DiagnosticUnsupportedEmissionReason,
) -> String {
    use bray_diagnostics::DiagnosticUnsupportedEmissionReason as Reason;

    match reason {
        Reason::ToolUnavailable { tool, configured } => configured.as_ref().map_or_else(
            || format!("the required {} is unavailable", llvm_tool_role(*tool)),
            |path| {
                format!(
                    "the required {} is absent at configured path {} and no bundled tool is available",
                    llvm_tool_role(*tool),
                    format_english_path(path)
                )
            },
        ),
        Reason::ToolInspectionFailed { tool, path, error } => format!(
            "inspection of the required {} at {} failed because the host reported {}",
            llvm_tool_role(*tool),
            format_english_path(path),
            format_english_io_error_kind(*error)
        ),
        Reason::InvalidToolFile { tool, path } => format!(
            "the required {} path {} is not a regular executable file",
            llvm_tool_role(*tool),
            format_english_path(path)
        ),
        Reason::MissingHostEnvironment(variable) => format!(
            "the host environment does not define {}",
            format_english_quoted_text(variable.as_str())
        ),
    }
}

const fn llvm_tool_role(role: bray_diagnostics::DiagnosticLlvmToolRole) -> &'static str {
    use bray_diagnostics::DiagnosticLlvmToolRole as Role;

    match role {
        Role::CompilerDriver => "LLVM compiler driver",
        Role::Linker => "LLVM linker",
        Role::Archiver => "LLVM archiver",
        Role::Optimizer => "LLVM optimizer",
        Role::SymbolInspector => "LLVM symbol inspector",
        Role::ObjectInspector => "LLVM object inspector",
        Role::BitcodeInspector => "LLVM bitcode inspector",
    }
}
