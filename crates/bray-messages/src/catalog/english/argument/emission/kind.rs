use bray_diagnostics::{
    DiagnosticArtifactRequirement, DiagnosticAssemblySyntaxKind, DiagnosticDebugInformationMode,
    DiagnosticDebugOutputMode, DiagnosticLinkInputKind, DiagnosticLinkedArtifactKind,
    DiagnosticLinkedProductKind, DiagnosticProductKind,
};

pub(crate) const fn format_english_link_input_kind(kind: DiagnosticLinkInputKind) -> &'static str {
    match kind {
        DiagnosticLinkInputKind::RelocatableObject => "relocatable object",
        DiagnosticLinkInputKind::Bitcode => "backend bitcode",
        DiagnosticLinkInputKind::Archive => "native archive",
        DiagnosticLinkInputKind::StartupObject => "startup object",
        DiagnosticLinkInputKind::TerminationObject => "termination object",
        DiagnosticLinkInputKind::RuntimeComponent => "runtime component",
        DiagnosticLinkInputKind::NativeLibrary => "native library",
        DiagnosticLinkInputKind::Framework => "platform framework",
    }
}

pub(crate) const fn format_english_linked_artifact_kind(
    kind: DiagnosticLinkedArtifactKind,
) -> &'static str {
    match kind {
        DiagnosticLinkedArtifactKind::Executable => "executable",
        DiagnosticLinkedArtifactKind::SharedLibrary => "shared library",
        DiagnosticLinkedArtifactKind::StaticLibrary => "static library",
        DiagnosticLinkedArtifactKind::ImportLibrary => "import library",
        DiagnosticLinkedArtifactKind::DebugCompanion => "debug companion",
        DiagnosticLinkedArtifactKind::PlatformCompanion => "platform companion",
    }
}

pub(crate) const fn format_english_linked_product_kind(
    kind: DiagnosticLinkedProductKind,
) -> &'static str {
    match kind {
        DiagnosticLinkedProductKind::Executable => "executable",
        DiagnosticLinkedProductKind::SharedLibrary => "shared library",
        DiagnosticLinkedProductKind::StaticLibrary => "static library",
    }
}

pub(crate) const fn format_english_product_kind(kind: DiagnosticProductKind) -> &'static str {
    match kind {
        DiagnosticProductKind::Executable => "executable",
        DiagnosticProductKind::Library => "library",
        DiagnosticProductKind::Test => "test product",
    }
}

pub(crate) const fn format_english_debug_information_mode(
    kind: DiagnosticDebugInformationMode,
) -> &'static str {
    match kind {
        DiagnosticDebugInformationMode::None => "no debug information",
        DiagnosticDebugInformationMode::LineTables => "line-table debug information",
        DiagnosticDebugInformationMode::Full => "full debug information",
    }
}

pub(crate) const fn format_english_debug_output_mode(
    kind: DiagnosticDebugOutputMode,
) -> &'static str {
    match kind {
        DiagnosticDebugOutputMode::Omit => "omitted debug output",
        DiagnosticDebugOutputMode::Embedded => "embedded debug output",
        DiagnosticDebugOutputMode::Separate => "separate debug output",
    }
}

pub(crate) const fn format_english_assembly_syntax(
    kind: DiagnosticAssemblySyntaxKind,
) -> &'static str {
    match kind {
        DiagnosticAssemblySyntaxKind::TargetDefault => "the target-default assembly syntax",
        DiagnosticAssemblySyntaxKind::Intel => "Intel assembly syntax",
        DiagnosticAssemblySyntaxKind::Att => "AT&T assembly syntax",
    }
}

pub(crate) const fn format_english_artifact_requirement(
    kind: DiagnosticArtifactRequirement,
) -> &'static str {
    match kind {
        DiagnosticArtifactRequirement::Required => "required",
        DiagnosticArtifactRequirement::Optional => "optional",
    }
}

pub(crate) const fn format_english_emission_artifact_operation(
    kind: bray_diagnostics::DiagnosticEmissionArtifactOperation,
) -> &'static str {
    use bray_diagnostics::DiagnosticEmissionArtifactOperation as Kind;

    match kind {
        Kind::CreateStagingStorage => "create private staging storage",
        Kind::ReadContribution => "read the completed contribution",
        Kind::WriteStagingStorage => "write private staging storage",
        Kind::FlushStagingStorage => "flush private staging storage",
    }
}
