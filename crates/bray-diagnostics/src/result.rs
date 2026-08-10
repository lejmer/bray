use crate::DiagnosticBag;

/// An immutable compiler fact value and the diagnostics owned by that fact.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DiagnosticResult<T> {
    value: T,
    diagnostics: DiagnosticBag,
}

impl<T> DiagnosticResult<T> {
    /// Creates a fact result from its semantic value and owned diagnostics.
    pub const fn new(value: T, diagnostics: DiagnosticBag) -> Self {
        Self { value, diagnostics }
    }

    /// Creates a successful fact result without diagnostics.
    pub fn without_diagnostics(value: T) -> Self {
        Self::new(value, DiagnosticBag::new())
    }

    /// Returns the immutable semantic value.
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Returns the diagnostics owned by this fact.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes this result into its semantic value and diagnostic bag.
    pub fn into_parts(self) -> (T, DiagnosticBag) {
        (self.value, self.diagnostics)
    }

    /// Transforms the semantic value while preserving diagnostic ownership.
    pub fn map<U>(self, map: impl FnOnce(T) -> U) -> DiagnosticResult<U> {
        DiagnosticResult::new(map(self.value), self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use crate::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::DiagnosticResult;

    #[test]
    fn results_keep_values_and_owned_diagnostics_together() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(4),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Error,
        );

        let result = DiagnosticResult::new(7_u32, DiagnosticBag::single(diagnostic.clone()));

        assert_eq!(result.value(), &7);

        assert_eq!(
            result.diagnostics().diagnostics(),
            std::slice::from_ref(&diagnostic)
        );

        let (value, diagnostics) = result.into_parts();

        assert_eq!(value, 7);
        assert_eq!(diagnostics.diagnostics(), &[diagnostic]);
    }

    #[test]
    fn mapping_values_preserves_diagnostic_ownership() {
        let diagnostic = Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::DeclarationDuplicateName,
            SeverityKind::Warning,
        );

        let result = DiagnosticResult::new(3_u32, DiagnosticBag::single(diagnostic.clone()))
            .map(|value| value.to_string());

        assert_eq!(result.value(), "3");
        assert_eq!(result.diagnostics().diagnostics(), &[diagnostic]);
    }

    #[test]
    fn results_are_send_and_sync_when_their_values_are() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DiagnosticResult<u32>>();
    }
}
