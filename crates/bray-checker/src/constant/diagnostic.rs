use bray_bound_tree::BoundOperator;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticConstantOperation, DiagnosticKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_source::SourceSpan;
use bray_symbols::TypeId;

use super::literal::ConstantLiteralError;
use super::operation::ConstantOperationError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ConstantDiagnostic {
    InvalidExpression(Option<bray_diagnostics::DiagnosticExpressionCategory>),
    NonMaterializable(TypeId),
    Precondition {
        origin: Option<SourceSpan>,
    },
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
            Self::InvalidExpression(_) | Self::Literal(ConstantLiteralError::Invalid) => {
                DiagnosticKind::CheckingInvalidConstantExpression
            }
            Self::NonMaterializable(_) => DiagnosticKind::CheckingNonMaterializableConstant,
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
            Self::Precondition { .. } => DiagnosticKind::CheckingConstantCallRequirementNotProven,
        }
    }

    pub(super) fn render<C: crate::CheckerRequestContext + ?Sized>(
        self,
        context: &C,
        id: bray_diagnostics::DiagnosticId,
        span: Option<SourceSpan>,
        result_type: impl FnOnce() -> Result<
            bray_symbols::TypeId,
            crate::CheckerQueryError<C::UpstreamError>,
        >,
    ) -> Result<Diagnostic, crate::CheckerQueryError<C::UpstreamError>> {
        use bray_diagnostics::{
            DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
        };

        if let Self::InvalidExpression(Some(category)) = self {
            return Ok(invalid_expression_diagnostic(id, span, category));
        }

        let mut diagnostic = Diagnostic::new(id, self.kind(), SeverityKind::Error);

        let span = if matches!(self, Self::Precondition { .. }) {
            None
        } else {
            span
        };

        if let Some(span) = span {
            diagnostic = diagnostic
                .with_primary_span(span)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::InvalidConstantExpression,
                    span,
                ));
        }

        diagnostic = self.apply(diagnostic);

        let note = match self {
            Self::Precondition { .. } => {
                diagnostic = diagnostic.with_arg(DiagnosticArg::expression_category(
                    bray_diagnostics::DiagnosticExpressionCategory::Call,
                ));

                Some(DiagnosticNoteKind::ConstantCallArgumentsMustSatisfyRequirements)
            }
            Self::InvalidExpression(Some(_)) => {
                unreachable!("invalid expression diagnostics use the shared producer")
            }
            Self::NonMaterializable(ty) => {
                diagnostic = diagnostic.with_arg(DiagnosticArg::actual_type(
                    crate::diagnostic::diagnostic_type(context, ty)?,
                ));

                None
            }
            Self::InvalidExpression(None) | Self::Literal(ConstantLiteralError::Invalid) => {
                panic!(
                    "Constant-evaluation inputs do not describe the requested bound unit. in render"
                );
            }
            Self::Literal(ConstantLiteralError::NotRepresentable)
            | Self::Operation {
                error: ConstantOperationError::NotRepresentable,
                ..
            } => {
                diagnostic = diagnostic.with_arg(DiagnosticArg::actual_type(
                    crate::diagnostic::diagnostic_type(context, result_type()?)?,
                ));

                None
            }
            Self::Operation {
                error: ConstantOperationError::Invalid,
                ..
            } => Some(DiagnosticNoteKind::ConstantExpressionMustBeEvaluable),
            Self::Literal(ConstantLiteralError::SizeLimitExceeded { .. })
            | Self::Operation {
                error: ConstantOperationError::ResourceLimitExceeded { .. },
                ..
            }
            | Self::Limit { .. } => Some(DiagnosticNoteKind::ConstantEvaluationMustFitLimits),
            Self::Cycle { .. }
            | Self::Operation {
                error: ConstantOperationError::DivisionByZero,
                ..
            } => None,
        };

        if let Some(note) = note {
            diagnostic = diagnostic.with_note(DiagnosticNote::new(note));
        }

        Ok(diagnostic)
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
            Self::Precondition {
                origin: Some(origin),
            } => diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                DiagnosticRelatedLocationKind::RequirementOrigin,
                origin,
            )),
            Self::Cycle {
                definition: Some(definition),
            } if diagnostic.primary_span() != Some(definition) => {
                diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                    DiagnosticRelatedLocationKind::FirstDeclaration,
                    definition,
                ))
            }
            Self::Precondition { origin: None }
            | Self::InvalidExpression(_)
            | Self::NonMaterializable(_)
            | Self::Literal(_)
            | Self::Cycle { .. } => diagnostic,
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

fn invalid_expression_diagnostic(
    id: bray_diagnostics::DiagnosticId,
    span: Option<SourceSpan>,
    category: bray_diagnostics::DiagnosticExpressionCategory,
) -> Diagnostic {
    let mut diagnostic = Diagnostic::new(
        id,
        DiagnosticKind::CheckingInvalidConstantExpression,
        bray_diagnostics::SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expression_category(category))
    .with_note(bray_diagnostics::DiagnosticNote::new(
        bray_diagnostics::DiagnosticNoteKind::ConstantExpressionMustBeEvaluable,
    ));

    if let Some(span) = span {
        diagnostic = diagnostic.with_primary_span(span).with_label(
            bray_diagnostics::DiagnosticLabel::primary(
                bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
                span,
            ),
        );
    }

    diagnostic
}

/// Reports a checked initializer's eligibility failure at its most specific source expression.
pub fn constant_eligibility_diagnostic(
    id: bray_diagnostics::DiagnosticId,
    unit: &bray_bound_tree::BoundUnit,
    cause: SourceSpan,
) -> Diagnostic {
    use bray_bound_tree::{AnyBoundNodeId, BoundWalkControl, BoundWalkEvent};

    let mut expression = None;

    let _outcome = bray_bound_tree::walk_bound_unit_view(unit.view(), unit.root(), |event| {
        if let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(id)) = event {
            let value = unit
                .view()
                .expression(id)
                .expect("walked constant expression exists");

            let anchor = value.origin().source_anchor().syntax();

            if anchor.source_id() == cause.source_id()
                && anchor.full_range().contains_range(cause.range())
                && expression
                    .as_ref()
                    .is_none_or(|(range, _): &(bray_source::TextRange, _)| {
                        anchor.full_range().len() < range.len()
                    })
            {
                expression = Some((
                    anchor.full_range(),
                    crate::diagnostic::expression_category(value),
                ));
            }
        }

        BoundWalkControl::Continue
    });

    let root = unit.key().source().syntax();

    let (span, category) = expression.map_or(
        (
            SourceSpan::new(root.source_id(), root.full_range()),
            bray_diagnostics::DiagnosticExpressionCategory::Block,
        ),
        |(_, category)| (cause, category),
    );

    let mut diagnostic = invalid_expression_diagnostic(id, Some(span), category);

    if span != cause {
        diagnostic = diagnostic.with_label(bray_diagnostics::DiagnosticLabel::secondary(
            bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
            cause,
        ));
    }

    diagnostic
}

/// Adds caller context only to nested constant diagnostics that lack a source location.
pub(super) fn anchor_diagnostics(
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
    source: impl FnOnce() -> Option<SourceSpan>,
) {
    if !diagnostics
        .iter()
        .any(|diagnostic| diagnostic.primary_span().is_none())
    {
        return;
    }

    let Some(span) = source() else {
        return;
    };

    *diagnostics = diagnostics
        .iter()
        .map(|diagnostic| {
            if diagnostic.primary_span().is_some() {
                diagnostic.clone()
            } else {
                diagnostic.clone().with_primary_span(span).with_label(
                    bray_diagnostics::DiagnosticLabel::primary(
                        bray_diagnostics::DiagnosticLabelKind::InvalidConstantExpression,
                        span,
                    ),
                )
            }
        })
        .collect();
}
