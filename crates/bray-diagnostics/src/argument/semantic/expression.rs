use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates a source-expression category argument.
    pub const fn expression_category(kind: crate::DiagnosticExpressionCategory) -> Self {
        Self::new(
            DiagnosticArgName::ExpressionCategory,
            DiagnosticArgValue::ExpressionCategory(kind),
        )
    }

    /// Creates a compile-time operation argument.
    pub const fn constant_operation(operation: crate::DiagnosticConstantOperation) -> Self {
        Self::new(
            DiagnosticArgName::ConstantOperation,
            DiagnosticArgValue::ConstantOperation(operation),
        )
    }
}
