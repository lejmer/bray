use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a zero-based input-index argument from a platform index.
    pub fn input_index(input_index: usize) -> Option<Self> {
        let input_index = u64::try_from(input_index).ok()?;

        Some(Self::new(
            DiagnosticArgName::InputIndex,
            DiagnosticArgValue::InputIndex(input_index),
        ))
    }

    /// Creates the ordinal of one native link input.
    pub const fn link_input_ordinal(ordinal: u32) -> Self {
        Self::new(
            DiagnosticArgName::LinkInputOrdinal,
            DiagnosticArgValue::Count(ordinal as u64),
        )
    }

    /// Creates the native input category found while building a link plan.
    pub const fn actual_link_input_kind(kind: DiagnosticLinkInputKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualLinkInputKind,
            DiagnosticArgValue::LinkInputKind(kind),
        )
    }
}

/// Locale-neutral category of one native link input.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkInputKind {
    RelocatableObject,
    Bitcode,
    Archive,
    StartupObject,
    TerminationObject,
    RuntimeComponent,
    NativeLibrary,
    Framework,
}

impl DiagnosticLinkInputKind {
    /// Returns the stable machine key for this input category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RelocatableObject => "relocatable_object",
            Self::Bitcode => "bitcode",
            Self::Archive => "archive",
            Self::StartupObject => "startup_object",
            Self::TerminationObject => "termination_object",
            Self::RuntimeComponent => "runtime_component",
            Self::NativeLibrary => "native_library",
            Self::Framework => "framework",
        }
    }
}
