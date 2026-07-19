use std::num::NonZeroU16;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, CheckedExpressionTypes, CheckedLiteralValueEntry,
    CheckedLiteralValues, ExpressionTypeResult,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{ConstantValueData, ConstantValueKind};

use crate::constant::literal::{
    LiteralValueError, parse_literal, parse_literal_without_target_width,
};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, ConstantEvaluationLimits,
    UnitCheckRequest,
};

/// Target and resource inputs used while adapting source literals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LiteralAdaptationInput<'facts> {
    expression_types: &'facts CheckedExpressionTypes,
    target_integer_width_bits: Option<NonZeroU16>,
    limits: ConstantEvaluationLimits,
}

impl<'facts> LiteralAdaptationInput<'facts> {
    /// Creates an input with standard deterministic resource limits.
    ///
    /// Target-sized integer values remain exact while target-width range validation is deferred.
    pub fn new(expression_types: &'facts CheckedExpressionTypes) -> Self {
        Self {
            expression_types,
            target_integer_width_bits: None,
            limits: ConstantEvaluationLimits::default(),
        }
    }

    /// Supplies the selected target width used by `isize` and `usize`.
    pub const fn with_target_integer_width_bits(mut self, width: NonZeroU16) -> Self {
        self.target_integer_width_bits = Some(width);

        self
    }

    /// Uses explicit deterministic resource limits.
    pub const fn with_limits(mut self, limits: ConstantEvaluationLimits) -> Self {
        self.limits = limits;

        self
    }

    /// Returns the complete expression types used by adaptation.
    pub const fn expression_types(self) -> &'facts CheckedExpressionTypes {
        self.expression_types
    }

    pub(crate) const fn target_integer_width_bits(self) -> Option<NonZeroU16> {
        self.target_integer_width_bits
    }

    pub(crate) const fn limits(self) -> ConstantEvaluationLimits {
        self.limits
    }
}

pub(crate) fn adapt_literals<C>(
    request: UnitCheckRequest<'_, C>,
    input: LiteralAdaptationInput<'_>,
) -> CheckerOutcome<CheckedLiteralValues>
where
    C: CheckerRequestContext + ?Sized,
{
    let types = input.expression_types();

    if types.unit() != request.view().unit() || types.kind() != request.view().kind() {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidLiteralAdaptationInput,
        );
    }

    let mut remaining_bytes = input.limits().literal_bytes();
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();

    for entry in types.entries() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let adapted = match adapt_literal_entry(
            request,
            input,
            entry.expression(),
            entry.result(),
            &mut remaining_bytes,
            diagnostics.len(),
        ) {
            Ok(adapted) => adapted,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        let Some(adapted) = adapted else {
            continue;
        };

        entries.push(adapted.entry);

        if let Some(diagnostic) = adapted.diagnostic {
            diagnostics.push(diagnostic);
        }
    }

    let values = match CheckedLiteralValues::try_new(
        request.unit(),
        types,
        request.semantic_values(),
        entries,
    ) {
        Ok(values) => values,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidLiteralAdaptationInput,
            );
        }
    };

    CheckerOutcome::complete(values, DiagnosticBag::from(diagnostics))
}

fn adapt_literal_entry<C>(
    request: UnitCheckRequest<'_, C>,
    input: LiteralAdaptationInput<'_>,
    expression: BoundExpressionId,
    result: ExpressionTypeResult,
    remaining_bytes: &mut u64,
    next_diagnostic: usize,
) -> Result<Option<AdaptedLiteral>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Literal(literal)) = request.view().expression(expression) else {
        return Ok(None);
    };

    if result.is_recovered() {
        let entry = intern_literal_value(request, expression, result, ConstantValueKind::Error)?;

        return Ok(Some(AdaptedLiteral::new(entry, None)));
    }

    let source = request.source(literal.origin().source_anchor())?;

    let Some(spelling) = source.text_for_range(literal.spelling_range()) else {
        return Err(CheckerInfrastructureError::InvalidSourceRange {
            span: bray_source::SourceSpan::new(source.span().source_id(), literal.spelling_range()),
        });
    };

    let bytes = u64::try_from(spelling.len()).unwrap_or(u64::MAX);
    let parsed = match remaining_bytes.checked_sub(bytes) {
        Some(remaining) => {
            *remaining_bytes = remaining;

            parse_adapted_literal(request, input, *literal, spelling, result.ty())
        }
        None => Err(LiteralAdaptationFailure::Diagnostic(
            DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        )),
    };

    let (kind, diagnostic) = match parsed {
        Ok(kind) => (kind, None),
        Err(LiteralAdaptationFailure::Diagnostic(kind)) => {
            let span = expression_span(request, expression)?;
            let diagnostic =
                Diagnostic::new(diagnostic_id(next_diagnostic), kind, SeverityKind::Error)
                    .with_primary_span(span);

            (ConstantValueKind::Error, Some(diagnostic))
        }
        Err(LiteralAdaptationFailure::Infrastructure(error)) => return Err(error),
    };

    let entry = intern_literal_value(request, expression, result, kind)?;

    Ok(Some(AdaptedLiteral::new(entry, diagnostic)))
}

fn intern_literal_value<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
    result: ExpressionTypeResult,
    kind: ConstantValueKind,
) -> Result<CheckedLiteralValueEntry, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let value = request
        .semantic_values()
        .intern_constant_value(ConstantValueData::new(result.ty(), kind))
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(CheckedLiteralValueEntry::new(expression, value))
}

fn parse_adapted_literal<C>(
    request: UnitCheckRequest<'_, C>,
    input: LiteralAdaptationInput<'_>,
    literal: bray_bound_tree::BoundLiteralExpression,
    spelling: &str,
    ty: bray_symbols::TypeId,
) -> Result<ConstantValueKind, LiteralAdaptationFailure>
where
    C: CheckerRequestContext + ?Sized,
{
    let representation =
        type_representation(request, ty).map_err(LiteralAdaptationFailure::Infrastructure)?;

    let Some(representation) = representation else {
        return Err(LiteralAdaptationFailure::Diagnostic(
            DiagnosticKind::CheckingInvalidConstantExpression,
        ));
    };

    let parsed = match input.target_integer_width_bits() {
        Some(width) => parse_literal(literal.kind(), spelling, representation, Some(width)),
        None => parse_literal_without_target_width(literal.kind(), spelling, representation),
    };

    parsed.map_err(|error| match error {
        LiteralValueError::Invalid => {
            LiteralAdaptationFailure::Diagnostic(DiagnosticKind::CheckingInvalidConstantExpression)
        }
        LiteralValueError::NotRepresentable => LiteralAdaptationFailure::Diagnostic(
            DiagnosticKind::CheckingConstantLiteralNotRepresentable,
        ),
        LiteralValueError::SizeLimitExceeded => LiteralAdaptationFailure::Diagnostic(
            DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
        ),
        LiteralValueError::TargetIntegerWidthRequired => LiteralAdaptationFailure::Infrastructure(
            CheckerInfrastructureError::InvalidLiteralAdaptationInput,
        ),
    })
}

enum LiteralAdaptationFailure {
    Diagnostic(DiagnosticKind),
    Infrastructure(CheckerInfrastructureError),
}

struct AdaptedLiteral {
    entry: CheckedLiteralValueEntry,
    diagnostic: Option<Diagnostic>,
}

impl AdaptedLiteral {
    const fn new(entry: CheckedLiteralValueEntry, diagnostic: Option<Diagnostic>) -> Self {
        Self { entry, diagnostic }
    }
}
