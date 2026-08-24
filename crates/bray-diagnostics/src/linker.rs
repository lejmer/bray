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

    /// Captures deterministic bounded output and records every omitted or lossy byte property.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let (text, captured_byte_count, lossy_utf8) = if bytes.len() <= Self::MAX_CAPTURED_BYTES {
            let (text, lossy) = rendered_bytes(bytes);

            (text, bytes.len(), lossy)
        } else {
            let prefix_limit = Self::MAX_CAPTURED_BYTES / 2;
            let suffix_limit = Self::MAX_CAPTURED_BYTES - prefix_limit;

            let prefix_end = utf8_safe_prefix_end(bytes, prefix_limit);
            let suffix_start = utf8_safe_suffix_start(bytes, bytes.len() - suffix_limit);

            let prefix = &bytes[..prefix_end];
            let suffix = &bytes[suffix_start..];

            let (prefix, prefix_lossy) = rendered_bytes(prefix);

            let (suffix, suffix_lossy) = rendered_bytes(suffix);

            (
                format!("{prefix}\n...\n{suffix}"),
                prefix_end.saturating_add(bytes.len() - suffix_start),
                prefix_lossy || suffix_lossy,
            )
        };

        let original_byte_count = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        let captured_byte_count = u64::try_from(captured_byte_count).unwrap_or(u64::MAX);

        Self {
            text,
            original_byte_count,
            captured_byte_count,
            omitted_byte_count: original_byte_count.saturating_sub(captured_byte_count),
            lossy_utf8,
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

    /// Returns whether invalid UTF-8 was replaced while rendering the captured prefix.
    pub const fn is_lossy_utf8(&self) -> bool {
        self.lossy_utf8
    }
}

fn rendered_bytes(bytes: &[u8]) -> (String, bool) {
    match std::str::from_utf8(bytes) {
        Ok(text) => (text.to_owned(), false),
        Err(_) => (String::from_utf8_lossy(bytes).into_owned(), true),
    }
}

fn utf8_safe_prefix_end(bytes: &[u8], maximum_end: usize) -> usize {
    if maximum_end == bytes.len() {
        return maximum_end;
    }

    for start in maximum_end.saturating_sub(3)..maximum_end {
        let scalar_length = match bytes[start].leading_ones() {
            2 => 2,
            3 => 3,
            4 => 4,
            _ => continue,
        };

        let scalar_end = start + scalar_length;

        if maximum_end < scalar_end
            && scalar_end <= bytes.len()
            && std::str::from_utf8(&bytes[start..scalar_end]).is_ok()
        {
            return start;
        }
    }

    maximum_end
}

fn utf8_safe_suffix_start(bytes: &[u8], minimum_start: usize) -> usize {
    let maximum_start = minimum_start.saturating_add(3).min(bytes.len());

    (minimum_start..=maximum_start)
        .find(|start| {
            bytes
                .get(*start)
                .is_none_or(|byte| byte & 0b1100_0000 != 0b1000_0000)
        })
        .unwrap_or(maximum_start)
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

    #[test]
    fn valid_external_tool_output_bounding_preserves_utf8_boundaries() {
        let mut bytes = vec![b'x'; DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES - 1];
        bytes.extend_from_slice("€".as_bytes());
        bytes.push(b'y');

        let capture = DiagnosticExternalToolStreamCapture::from_bytes(&bytes);

        assert_eq!(
            capture.captured_byte_count(),
            u64::try_from(DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES)
                .unwrap_or_else(|_| panic!("test capture bound must fit the diagnostic contract")),
        );

        assert_eq!(capture.omitted_byte_count(), 3);
        assert!(!capture.is_lossy_utf8());
        assert!(capture.text().starts_with(&"x".repeat(8 * 1024)));
        assert!(capture.text().ends_with('y'));
    }

    #[test]
    fn invalid_external_tool_bytes_in_the_failure_tail_are_reported_as_lossy() {
        let mut bytes = vec![b'x'; DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES + 1];
        bytes[DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES] = 0xff;

        let capture = DiagnosticExternalToolStreamCapture::from_bytes(&bytes);

        assert_eq!(capture.captured_byte_count(), 16 * 1024);
        assert_eq!(capture.omitted_byte_count(), 1);
        assert!(capture.is_lossy_utf8());
        assert!(capture.text().starts_with(&"x".repeat(8 * 1024)));
        assert!(capture.text().ends_with('\u{fffd}'));
    }

    #[test]
    fn later_invalid_output_does_not_split_a_valid_scalar_at_the_capture_boundary() {
        let mut bytes = vec![b'x'; DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES - 1];
        bytes.extend_from_slice("€".as_bytes());
        bytes.push(0xff);

        let capture = DiagnosticExternalToolStreamCapture::from_bytes(&bytes);

        assert_eq!(
            capture.captured_byte_count(),
            u64::try_from(DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES)
                .unwrap_or_else(|_| panic!("test capture bound must fit the diagnostic contract")),
        );

        assert_eq!(capture.omitted_byte_count(), 3);
        assert!(capture.is_lossy_utf8());
        assert!(capture.text().starts_with(&"x".repeat(8 * 1024)));
        assert!(capture.text().ends_with('\u{fffd}'));
    }

    #[test]
    fn long_external_tool_output_preserves_the_failure_tail() {
        let mut bytes = vec![b'w'; DiagnosticExternalToolStreamCapture::MAX_CAPTURED_BYTES];
        bytes.extend_from_slice(b"linker failure");

        let capture = DiagnosticExternalToolStreamCapture::from_bytes(&bytes);

        assert!(capture.text().starts_with(&"w".repeat(8 * 1024)));
        assert!(capture.text().ends_with("linker failure"));
        assert_eq!(capture.captured_byte_count(), 16 * 1024);
        assert_eq!(capture.omitted_byte_count(), 14);
    }
}
