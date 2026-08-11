use bray_bound_tree::BoundOperator;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticConstantOperation, DiagnosticKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_source::SourceSpan;

use super::literal::ConstantLiteralError;
use super::operation::ConstantOperationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstantDiagnostic {
    InvalidExpression,
    Literal(ConstantLiteralError),
    Operation {
        operation: DiagnosticConstantOperation,
        error: ConstantOperationError,
    },
    Limit {
        resource: ConstantLimitKind,
        actual: u64,
        maximum: u64,
    },
    Cycle {
        definition: Option<SourceSpan>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstantLimitKind {
    EvaluationSteps,
    AggregateElements,
    ExpandedElements,
    LiteralBytes,
}

impl ConstantDiagnostic {
    pub(super) const fn operation(
        operation: DiagnosticConstantOperation,
        error: ConstantOperationError,
    ) -> Self {
        Self::Operation { operation, error }
    }

    pub(super) const fn limit(resource: ConstantLimitKind, actual: u64, maximum: u64) -> Self {
        Self::Limit {
            resource,
            actual,
            maximum,
        }
    }

    pub(super) const fn kind(self) -> DiagnosticKind {
        match self {
            Self::InvalidExpression | Self::Literal(ConstantLiteralError::Invalid) => {
                DiagnosticKind::CheckingInvalidConstantExpression
            }
            Self::Literal(ConstantLiteralError::NotRepresentable) => {
                DiagnosticKind::CheckingConstantLiteralNotRepresentable
            }
            Self::Literal(ConstantLiteralError::SizeLimitExceeded { .. }) => {
                DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded
            }
            Self::Operation {
                error: ConstantOperationError::Invalid,
                ..
            } => DiagnosticKind::CheckingInvalidConstantOperation,
            Self::Operation {
                error: ConstantOperationError::DivisionByZero,
                ..
            } => DiagnosticKind::CheckingConstantDivisionByZero,
            Self::Operation {
                error: ConstantOperationError::NotRepresentable,
                ..
            } => DiagnosticKind::CheckingConstantValueNotRepresentable,
            Self::Operation {
                error: ConstantOperationError::ResourceLimitExceeded { .. },
                ..
            } => DiagnosticKind::CheckingConstantIntegerSizeLimitExceeded,
            Self::Limit {
                resource: ConstantLimitKind::EvaluationSteps,
                ..
            } => DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
            Self::Limit {
                resource: ConstantLimitKind::AggregateElements,
                ..
            } => DiagnosticKind::CheckingConstantAggregateLimitExceeded,
            Self::Limit {
                resource: ConstantLimitKind::ExpandedElements,
                ..
            } => DiagnosticKind::CheckingConstantExpansionLimitExceeded,
            Self::Limit {
                resource: ConstantLimitKind::LiteralBytes,
                ..
            } => DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
            Self::Cycle { .. } => DiagnosticKind::CheckingCyclicConstantDefinition,
        }
    }

    pub(super) fn apply(self, diagnostic: Diagnostic) -> Diagnostic {
        match self {
            Self::Operation {
                operation,
                error: ConstantOperationError::ResourceLimitExceeded { actual, maximum },
            } => diagnostic
                .with_arg(DiagnosticArg::constant_operation(operation))
                .with_arg(DiagnosticArg::actual_count(actual))
                .with_arg(DiagnosticArg::maximum_count(maximum)),
            Self::Operation { operation, .. } => {
                diagnostic.with_arg(DiagnosticArg::constant_operation(operation))
            }
            Self::Literal(ConstantLiteralError::SizeLimitExceeded { actual, maximum })
            | Self::Limit {
                actual, maximum, ..
            } => diagnostic
                .with_arg(DiagnosticArg::actual_count(actual))
                .with_arg(DiagnosticArg::maximum_count(maximum)),
            Self::Cycle {
                definition: Some(definition),
            } if diagnostic.primary_span() != Some(definition) => {
                diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                    DiagnosticRelatedLocationKind::FirstDeclaration,
                    definition,
                ))
            }
            Self::InvalidExpression | Self::Literal(_) | Self::Cycle { .. } => diagnostic,
        }
    }
}

pub(super) const fn diagnostic_operation(operation: BoundOperator) -> DiagnosticConstantOperation {
    match operation {
        BoundOperator::LogicalOr => DiagnosticConstantOperation::LogicalOr,
        BoundOperator::LogicalAnd => DiagnosticConstantOperation::LogicalAnd,
        BoundOperator::Equal => DiagnosticConstantOperation::Equal,
        BoundOperator::NotEqual => DiagnosticConstantOperation::NotEqual,
        BoundOperator::Less => DiagnosticConstantOperation::Less,
        BoundOperator::LessEqual => DiagnosticConstantOperation::LessEqual,
        BoundOperator::Greater => DiagnosticConstantOperation::Greater,
        BoundOperator::GreaterEqual => DiagnosticConstantOperation::GreaterEqual,
        BoundOperator::BitwiseOr => DiagnosticConstantOperation::BitwiseOr,
        BoundOperator::BitwiseXor => DiagnosticConstantOperation::BitwiseXor,
        BoundOperator::BitwiseAnd => DiagnosticConstantOperation::BitwiseAnd,
        BoundOperator::ShiftLeft => DiagnosticConstantOperation::ShiftLeft,
        BoundOperator::ShiftRight => DiagnosticConstantOperation::ShiftRight,
        BoundOperator::Add => DiagnosticConstantOperation::Add,
        BoundOperator::Subtract => DiagnosticConstantOperation::Subtract,
        BoundOperator::Multiply => DiagnosticConstantOperation::Multiply,
        BoundOperator::Divide => DiagnosticConstantOperation::Divide,
        BoundOperator::Remainder => DiagnosticConstantOperation::Remainder,
        BoundOperator::MatrixMultiply => DiagnosticConstantOperation::MatrixMultiply,
        BoundOperator::Exponentiate => DiagnosticConstantOperation::Exponentiate,
        BoundOperator::BitwiseNot => DiagnosticConstantOperation::BitwiseNot,
        BoundOperator::LogicalNot => DiagnosticConstantOperation::LogicalNot,
    }
}
