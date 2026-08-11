use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};
use crate::DiagnosticInterfaceLimit;

impl DiagnosticArg {
    /// Creates the exact package-interface resource category that exceeded its limit.
    pub const fn interface_limit(limit: DiagnosticInterfaceLimit) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceLimit,
            DiagnosticArgValue::InterfaceLimit(limit),
        )
    }
}
