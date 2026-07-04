/// Numeric code for a diagnostic category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticCode(u32);

impl DiagnosticCode {
    /// Creates a diagnostic code from its raw value.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the raw diagnostic code.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl From<DiagnosticCode> for u32 {
    fn from(code: DiagnosticCode) -> Self {
        code.raw()
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticCode;

    #[test]
    fn diagnostic_codes_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<DiagnosticCode>(), size_of::<u32>());

        let code = DiagnosticCode::new(2001);
        let copied = code;

        assert_eq!(copied.raw(), 2001);
    }
}
