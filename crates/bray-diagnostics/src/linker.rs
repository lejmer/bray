/// Stable category of a selected native linker or archiver driver.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkerDriverKind {
    /// Linker embedded in the compiler process.
    EmbeddedLld,
    /// Explicit external LLD executable.
    ExternalLld,
    /// Configured platform system linker.
    System,
    /// Static-library archiver.
    Archiver,
    /// Explicit target-specific driver.
    TargetSpecific,
}

impl DiagnosticLinkerDriverKind {
    /// Returns the stable machine key for this driver category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::EmbeddedLld => "embedded_lld",
            Self::ExternalLld => "external_lld",
            Self::System => "system",
            Self::Archiver => "archiver",
            Self::TargetSpecific => "target_specific",
        }
    }
}

/// Complete locale-neutral identity of a selected linker or archiver driver.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticLinkerDriverIdentity {
    kind: DiagnosticLinkerDriverKind,
    name: String,
    capability_revision: String,
    toolchain_revision: String,
}

impl DiagnosticLinkerDriverIdentity {
    /// Creates a complete selected-driver identity.
    pub fn new(
        kind: DiagnosticLinkerDriverKind,
        name: impl Into<String>,
        capability_revision: impl Into<String>,
        toolchain_revision: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            name: name.into(),
            capability_revision: capability_revision.into(),
            toolchain_revision: toolchain_revision.into(),
        }
    }

    /// Returns the selected driver category.
    pub const fn kind(&self) -> DiagnosticLinkerDriverKind {
        self.kind
    }

    /// Returns the stable selected driver name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the driver capability-contract revision.
    pub fn capability_revision(&self) -> &str {
        &self.capability_revision
    }

    /// Returns the compatible native toolchain revision.
    pub fn toolchain_revision(&self) -> &str {
        &self.toolchain_revision
    }
}

/// Bounded captured output from one external native-link tool stream.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticExternalToolStreamCapture {
    text: String,
    original_byte_count: u64,
    captured_byte_count: u64,
    omitted_byte_count: u64,
    lossy_utf8: bool,
}

impl DiagnosticExternalToolStreamCapture {
    const MAX_CAPTURED_BYTES: usize = 16 * 1024;

    /// Captures a deterministic bounded prefix and records every omitted or lossy byte property.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let captured = &bytes[..bytes.len().min(Self::MAX_CAPTURED_BYTES)];
        let original_byte_count = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let captured_byte_count = u64::try_from(captured.len()).unwrap_or(u64::MAX);

        Self {
            text: String::from_utf8_lossy(captured).into_owned(),
            original_byte_count,
            captured_byte_count,
            omitted_byte_count: original_byte_count.saturating_sub(captured_byte_count),
            lossy_utf8: std::str::from_utf8(bytes).is_err(),
        }
    }

    /// Returns the bounded rendered stream prefix.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the complete stream byte count before diagnostic bounding.
    pub const fn original_byte_count(&self) -> u64 {
        self.original_byte_count
    }

    /// Returns the exact number of source bytes represented in the captured prefix.
    pub const fn captured_byte_count(&self) -> u64 {
        self.captured_byte_count
    }

    /// Returns the exact number of bytes omitted after the captured prefix.
    pub const fn omitted_byte_count(&self) -> u64 {
        self.omitted_byte_count
    }

    /// Returns whether invalid UTF-8 was replaced while rendering the stream.
    pub const fn is_lossy_utf8(&self) -> bool {
        self.lossy_utf8
    }
}

/// Exact completed failure result from an external native linker or archiver.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticExternalToolExit {
    code: Option<i32>,
    standard_output: DiagnosticExternalToolStreamCapture,
    standard_error: DiagnosticExternalToolStreamCapture,
}

impl DiagnosticExternalToolExit {
    /// Creates a bounded diagnostic payload from the exact completed tool result.
    pub fn new(code: Option<i32>, standard_output: &[u8], standard_error: &[u8]) -> Self {
        Self {
            code,
            standard_output: DiagnosticExternalToolStreamCapture::from_bytes(standard_output),
            standard_error: DiagnosticExternalToolStreamCapture::from_bytes(standard_error),
        }
    }

    /// Returns the portable process exit code when one was available.
    pub const fn code(&self) -> Option<i32> {
        self.code
    }

    /// Returns bounded standard output with explicit byte accounting.
    pub const fn standard_output(&self) -> &DiagnosticExternalToolStreamCapture {
        &self.standard_output
    }

    /// Returns bounded standard error with explicit byte accounting.
    pub const fn standard_error(&self) -> &DiagnosticExternalToolStreamCapture {
        &self.standard_error
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticExternalToolStreamCapture;

    #[test]
    fn external_tool_output_bounding_reports_omission_and_encoding_loss() {
        let mut bytes = vec![b'x'; DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES + 3];
        bytes[0] = 0xff;

        let capture = DiagnosticExternalToolStreamCapture::from_bytes(&bytes);

        assert_eq!(
            capture.original_byte_count(),
            u64::try_from(bytes.len())
                .unwrap_or_else(|_| panic!("test byte count must fit the diagnostic contract")),
        );

        assert_eq!(capture.captured_byte_count(), 16 * 1024);
        assert_eq!(capture.omitted_byte_count(), 3);
        assert!(capture.is_lossy_utf8());
        assert!(capture.text().starts_with('\u{fffd}'));
    }
}
