/// Identity assigned to one emitted diagnostic record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticId(u32);

impl DiagnosticId {
    /// Creates a diagnostic identity from its raw value.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Creates an identity from an ordered diagnostic index.
    ///
    /// Indexes beyond the compact identity domain saturate at the last
    /// representable identity.
    pub fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).unwrap_or(u32::MAX))
    }

    /// Returns the raw diagnostic identity.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl From<DiagnosticId> for u32 {
    fn from(diagnostic_id: DiagnosticId) -> Self {
        diagnostic_id.raw()
    }
}

#[cfg(test)]
mod tests {
    use super::DiagnosticId;

    #[test]
    fn diagnostic_ids_are_compact_copyable_wrappers() {
        assert_eq!(size_of::<DiagnosticId>(), size_of::<u32>());

        let diagnostic_id = DiagnosticId::new(12);
        let copied = diagnostic_id;

        assert_eq!(copied.raw(), 12);
        assert_eq!(DiagnosticId::from_index(7).raw(), 7);
        assert_eq!(DiagnosticId::from_index(usize::MAX).raw(), u32::MAX);
    }
}
