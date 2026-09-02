/// Exact locale-neutral foreign-boundary query failure retained for diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticForeignQueryFailure {
    reason: &'static str,
    context: Box<[crate::DiagnosticFailureField]>,
}

impl DiagnosticForeignQueryFailure {
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

    /// Returns the exact structured fields retained at the foreign-query boundary.
    pub const fn context(&self) -> &[crate::DiagnosticFailureField] {
        &self.context
    }
}
