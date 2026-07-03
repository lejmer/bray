use bray_source::{SourceSpan, TextSize};

/// Stable typed argument attached to a diagnostic message component.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticArg {
    name: DiagnosticArgName,
    value: DiagnosticArgValue,
}

impl DiagnosticArg {
    /// Creates a typed diagnostic argument.
    pub const fn new(name: DiagnosticArgName, value: DiagnosticArgValue) -> Self {
        Self { name, value }
    }

    /// Returns the stable argument name.
    pub const fn name(self) -> DiagnosticArgName {
        self.name
    }

    /// Returns the typed argument value.
    pub const fn value(self) -> DiagnosticArgValue {
        self.value
    }
}

/// Stable name for a diagnostic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgName {
    /// Byte that participates in the diagnostic.
    Byte,
    /// Number of bytes that participate in the diagnostic.
    ByteCount,
    /// Character that participates in the diagnostic.
    Character,
    /// Start location of a block comment or another paired source construct.
    ConstructStart,
    /// Byte offset inside a source input.
    TextOffset,
    /// Source span that participates in the diagnostic.
    SourceSpan,
}

/// Locale-neutral typed value for a diagnostic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArgValue {
    /// Raw source byte.
    Byte(u8),
    /// Source byte count.
    ByteCount(u32),
    /// Source character.
    Character(char),
    /// Byte offset inside source text.
    TextOffset(TextSize),
    /// Source span.
    SourceSpan(SourceSpan),
}

#[cfg(test)]
mod tests {
    use super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

    #[test]
    fn diagnostic_args_pair_stable_names_with_typed_values() {
        let arg = DiagnosticArg::new(
            DiagnosticArgName::Character,
            DiagnosticArgValue::Character('\u{0}'),
        );

        assert_eq!(arg.name(), DiagnosticArgName::Character);
        assert_eq!(arg.value(), DiagnosticArgValue::Character('\u{0}'));
    }
}
