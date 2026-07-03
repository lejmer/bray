/// Severity assigned to a diagnostic record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SeverityKind {
    /// Prevents successful compilation of the affected product.
    Error,
    /// Does not prevent compilation unless invocation policy promotes it.
    Warning,
    /// Supporting information attached to another diagnostic.
    Note,
    /// Advisory information attached to another diagnostic.
    Help,
}

impl SeverityKind {
    /// Returns the stable machine key for this severity.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
            Self::Help => "help",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SeverityKind;

    #[test]
    fn severities_expose_stable_machine_keys() {
        assert_eq!(SeverityKind::Error.as_str(), "error");
        assert_eq!(SeverityKind::Warning.as_str(), "warning");
    }
}
