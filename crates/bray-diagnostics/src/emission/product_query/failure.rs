/// Exact locale-neutral product-query contract failure retained for emission diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticProductQueryFailure {
    reason: &'static str,
    context: Box<[crate::DiagnosticFailureField]>,
}

impl DiagnosticProductQueryFailure {
    pub fn new(
        reason: &'static str,
        context: impl Into<Box<[crate::DiagnosticFailureField]>>,
    ) -> Self {
        Self {
            reason,
            context: context.into(),
        }
    }

    /// Returns this failure category's stable machine-readable name.
    pub const fn as_str(&self) -> &'static str {
        self.reason
    }

    /// Returns the exact structured fields retained at the product-query boundary.
    pub const fn context(&self) -> &[crate::DiagnosticFailureField] {
        &self.context
    }
}
