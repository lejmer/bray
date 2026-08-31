use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates the stage at which a native-code generator rejected its input.
    pub const fn codegen_verification_stage(kind: DiagnosticCodegenVerificationStage) -> Self {
        Self::new(
            DiagnosticArgName::CodegenVerificationStage,
            DiagnosticArgValue::CodegenVerificationStage(kind),
        )
    }

    /// Creates the exact report returned by a native-code generator.
    pub fn codegen_backend_report(report: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::CodegenBackendReport,
            DiagnosticArgValue::CodegenBackendReport(report.into()),
        )
    }

    /// Creates the debug-information amount rejected by a backend.
    pub const fn actual_debug_information_mode(kind: DiagnosticDebugInformationMode) -> Self {
        Self::new(
            DiagnosticArgName::ActualDebugInformationMode,
            DiagnosticArgValue::DebugInformationMode(kind),
        )
    }

    /// Creates the debug-information placement rejected by a backend.
    pub const fn actual_debug_output_mode(kind: DiagnosticDebugOutputMode) -> Self {
        Self::new(
            DiagnosticArgName::ActualDebugOutputMode,
            DiagnosticArgValue::DebugOutputMode(kind),
        )
    }

    /// Creates the assembly syntax rejected by a backend.
    pub const fn actual_assembly_syntax(kind: DiagnosticAssemblySyntaxKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualAssemblySyntax,
            DiagnosticArgValue::AssemblySyntax(kind),
        )
    }
}

/// Locale-neutral stage at which generated native-code input is validated.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCodegenVerificationStage {
    BeforeOptimization,
    AfterOptimization,
}

impl DiagnosticCodegenVerificationStage {
    /// Returns the stable machine key for this validation stage.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforeOptimization => "before_optimization",
            Self::AfterOptimization => "after_optimization",
        }
    }
}

/// Locale-neutral amount of generated debug information.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDebugInformationMode {
    None,
    LineTables,
    Full,
}

impl DiagnosticDebugInformationMode {
    /// Returns the stable machine key for this debug-information amount.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LineTables => "line_tables",
            Self::Full => "full",
        }
    }
}

/// Locale-neutral placement of generated debug information.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDebugOutputMode {
    Omit,
    Embedded,
    Separate,
}

impl DiagnosticDebugOutputMode {
    /// Returns the stable machine key for this debug-information placement.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Omit => "omit",
            Self::Embedded => "embedded",
            Self::Separate => "separate",
        }
    }
}

/// Locale-neutral assembly syntax selected for backend output.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticAssemblySyntaxKind {
    TargetDefault,
    Intel,
    Att,
}

impl DiagnosticAssemblySyntaxKind {
    /// Returns the stable machine key for this assembly syntax.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TargetDefault => "target_default",
            Self::Intel => "intel",
            Self::Att => "att",
        }
    }
}
