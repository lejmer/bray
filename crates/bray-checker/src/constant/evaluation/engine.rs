use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, BoundStructuredExpressionKind,
};
use bray_compiler_known::{IntegerRepresentation, NumericRepresentationKind};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{
    ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind, RealConstantBits,
    TargetSizedIntegerType, TypeId,
};

use crate::constant::limits::EvaluationBudget;
use crate::constant::literal::{normalize_integer_literal, parse_literal};
use crate::constant::operation::negate_real;
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;

use super::support::{EvaluationFailure, integer_to_usize};

use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitRoot,
    CheckerUnitView, ConstantEvaluationInput, ConstantReferenceResolution,
};

pub(crate) fn evaluate_constant<C>(
    request: CheckerUnitView<'_, C>,
    input: &ConstantEvaluationInput<'_>,
) -> CheckerOutcome<ConstantValueId>
where
    C: CheckerRequestContext + ?Sized,
{
    let evaluated = match evaluate_checked(request, input, false) {
        Ok(evaluated) => evaluated,
        Err(EvaluationAbort::Cancelled) => return CheckerOutcome::Cancelled,
        Err(EvaluationAbort::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    };

    let value = match evaluated
        .evaluator
        .closed_value(evaluated.term, evaluated.root)
    {
        Ok(value) => value,
        Err(EvaluationFailure::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(EvaluationFailure::Cancelled) => return CheckerOutcome::Cancelled,
        Err(EvaluationFailure::Source { .. }) => {
            let value = match evaluated.evaluator.recovery_value(evaluated.root) {
                Ok(value) => value,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            let mut diagnostics = evaluated.evaluator.diagnostics;

            let span = match expression_span(request, evaluated.root) {
                Ok(span) => span,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            diagnostics.add(
                Diagnostic::new(
                    diagnostic_id(diagnostics.len()),
                    DiagnosticKind::CheckingInvalidConstantExpression,
                    SeverityKind::Error,
                )
                .with_primary_span(span),
            );

            return CheckerOutcome::complete(value, diagnostics);
        }
    };

    CheckerOutcome::complete(value, evaluated.evaluator.diagnostics)
}

pub(crate) fn check_constant_term<C>(
    request: CheckerUnitView<'_, C>,
    input: &ConstantEvaluationInput<'_>,
) -> CheckerOutcome<ConstantTermId>
where
    C: CheckerRequestContext + ?Sized,
{
    match evaluate_checked(request, input, true) {
        Ok(evaluated) => CheckerOutcome::complete(evaluated.term, evaluated.evaluator.diagnostics),
        Err(EvaluationAbort::Cancelled) => CheckerOutcome::Cancelled,
        Err(EvaluationAbort::Infrastructure(error)) => CheckerOutcome::InfrastructureFailure(error),
    }
}

fn evaluate_checked<'view, 'input, 'types, C>(
    request: CheckerUnitView<'view, C>,
    input: &'input ConstantEvaluationInput<'types>,
    retain_target_literals: bool,
) -> Result<EvaluatedConstant<'view, 'input, 'types, C>, EvaluationAbort>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return Err(EvaluationAbort::Cancelled);
    }

    let CheckerUnitRoot::Expression(root) = request.root() else {
        return Err(EvaluationAbort::Infrastructure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput,
        ));
    };

    if input.expression_types().unit() != request.view().unit()
        || input.expression_types().kind() != request.view().kind()
        || input.semantic_selections().unit() != request.view().unit()
        || input.semantic_selections().kind() != request.view().kind()
        || !input.references_are_consistent()
    {
        return Err(EvaluationAbort::Infrastructure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput,
        ));
    }

    let mut evaluator = Evaluator::new(request, input, retain_target_literals);

    match evaluator.evaluate(root) {
        Ok(term) => Ok(EvaluatedConstant {
            evaluator,
            root,
            term,
        }),
        Err(EvaluationFailure::Cancelled) => Err(EvaluationAbort::Cancelled),
        Err(EvaluationFailure::Infrastructure(error)) => {
            Err(EvaluationAbort::Infrastructure(error))
        }
        Err(EvaluationFailure::Source { expression, kind }) => {
            let span = match expression_span(request, expression) {
                Ok(span) => span,
                Err(error) => return Err(EvaluationAbort::Infrastructure(error)),
            };

            evaluator.diagnostics.add(
                Diagnostic::new(
                    diagnostic_id(evaluator.diagnostics.len()),
                    kind,
                    SeverityKind::Error,
                )
                .with_primary_span(span),
            );

            match evaluator.recovery_term(root) {
                Ok(term) => Ok(EvaluatedConstant {
                    evaluator,
                    root,
                    term,
                }),
                Err(error) => Err(EvaluationAbort::Infrastructure(error)),
            }
        }
    }
}

struct EvaluatedConstant<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    evaluator: Evaluator<'view, 'input, 'types, C>,
    root: BoundExpressionId,
    term: ConstantTermId,
}

enum EvaluationAbort {
    Cancelled,
    Infrastructure(CheckerInfrastructureError),
}

pub(super) struct Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'view, C>,
    pub(super) input: &'input ConstantEvaluationInput<'types>,
    pub(super) budget: EvaluationBudget,
    pub(super) diagnostics: DiagnosticBag,
    retain_target_literals: bool,
}

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn new(
        request: CheckerUnitView<'view, C>,
        input: &'input ConstantEvaluationInput<'types>,
        retain_target_literals: bool,
    ) -> Self {
        Self {
            request,
            input,
            budget: EvaluationBudget::new(input),
            diagnostics: DiagnosticBag::new(),
            retain_target_literals,
        }
    }

    pub(super) fn evaluate(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        self.observe_cancellation()?;
        self.budget.charge_step(expression)?;

        let Some(bound) = self.request.view().expression(expression) else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidExpressionTypeInput { expression },
            ));
        };

        let ty = self.expression_type(expression)?;

        match bound {
            BoundExpression::Literal(literal) => self.evaluate_literal(expression, *literal, ty),
            BoundExpression::Name(_) => self.evaluate_reference(expression, ty),
            BoundExpression::Unary(unary) => {
                self.evaluate_operator(expression, unary.operator(), unary.operands(), ty)
            }
            BoundExpression::Binary(binary) if self.is_complex_literal(binary) => {
                self.evaluate_complex_literal(expression, binary, ty)
            }
            BoundExpression::Binary(binary) => {
                self.evaluate_operator(expression, binary.operator(), binary.operands(), ty)
            }
            BoundExpression::Conversion(conversion) => {
                self.evaluate_conversion(expression, *conversion, ty)
            }
            BoundExpression::Structured(structured) => {
                self.evaluate_structured(expression, structured.kind(), structured.operands(), ty)
            }
            BoundExpression::MemberAccess(member) => {
                self.evaluate_member_projection(expression, member, ty)
            }
            BoundExpression::StructConstruction(_)
            | BoundExpression::LeadingDotVariant(_)
            | BoundExpression::Call(_) => self.evaluate_construction(expression, ty),
            BoundExpression::Block(_)
            | BoundExpression::UnresolvedReference(_)
            | BoundExpression::Assignment(_)
            | BoundExpression::ErrorCall(_)
            | BoundExpression::ErrorConversion(_)
            | BoundExpression::AnonymousCallable(_)
            | BoundExpression::Await(_)
            | BoundExpression::TraitQualifiedMember(_)
            | BoundExpression::ControlTransfer(_)
            | BoundExpression::For(_)
            | BoundExpression::Match(_)
            | BoundExpression::Generator(_)
            | BoundExpression::Error(_) => {
                // TODO(BRA-122): Extend constant checking when this expression category gains constant semantics.
                Err(EvaluationFailure::invalid_expression(expression))
            }
        }
    }

    pub(super) fn evaluate_literal(
        &mut self,
        expression: BoundExpressionId,
        literal: bray_bound_tree::BoundLiteralExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let source = self
            .request
            .source(literal.origin().source_anchor())
            .map_err(EvaluationFailure::Infrastructure)?;

        let Some(spelling) = source.text_for_range(literal.spelling_range()) else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidSourceRange {
                    span: bray_source::SourceSpan::new(
                        source.span().source_id(),
                        literal.spelling_range(),
                    ),
                },
            ));
        };

        self.budget.charge_literal(expression, spelling.len())?;

        let representation = type_representation(self.request, ty)
            .map_err(EvaluationFailure::Infrastructure)?
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?;

        if self.retain_target_literals
            && literal.kind() == bray_bound_tree::BoundLiteralKind::Integer
        {
            let target_type = match representation.integer_representation() {
                Some(IntegerRepresentation::TargetSigned) => Some(TargetSizedIntegerType::Isize),
                Some(IntegerRepresentation::TargetUnsigned) => Some(TargetSizedIntegerType::Usize),
                _ => None,
            };

            if let Some(target_type) = target_type {
                let value = normalize_integer_literal(spelling)
                    .map_err(|error| EvaluationFailure::literal(expression, error))?;

                return self.intern_term(ConstantTermData::IntegerLiteral {
                    ty: target_type,
                    value,
                });
            }
        }

        let kind = parse_literal(literal.kind(), spelling, representation, || {
            self.request
                .selected_target()
                .machine()
                .pointer_width_bits()
        })
        .map_err(|error| EvaluationFailure::literal(expression, error))?;

        self.intern_value_term(ty, kind)
    }

    pub(super) fn evaluate_reference(
        &self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        match self.input.reference(expression) {
            Some(ConstantReferenceResolution::Value(value)) => {
                let data = self
                    .request
                    .semantic_values()
                    .constant_value_data(value)
                    .map_err(|_| {
                        EvaluationFailure::Infrastructure(
                            CheckerInfrastructureError::SemanticValueUnavailable,
                        )
                    })?;

                if data.ty() != ty {
                    return Err(EvaluationFailure::Infrastructure(
                        CheckerInfrastructureError::InvalidConstantEvaluationInput,
                    ));
                }

                self.intern_term(ConstantTermData::Value(value))
            }
            Some(ConstantReferenceResolution::Term(term)) => {
                self.request
                    .semantic_values()
                    .constant_term_data(term)
                    .map_err(|_| {
                        EvaluationFailure::Infrastructure(
                            CheckerInfrastructureError::SemanticValueUnavailable,
                        )
                    })?;

                Ok(term)
            }
            Some(ConstantReferenceResolution::Cycle) => Err(EvaluationFailure::Source {
                expression,
                kind: DiagnosticKind::CheckingCyclicConstantDefinition,
            }),
            None => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    pub(super) fn evaluate_structured(
        &mut self,
        expression: BoundExpressionId,
        kind: BoundStructuredExpressionKind,
        operands: &[BoundExpressionId],
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        match kind {
            BoundStructuredExpressionKind::Unit => {
                self.intern_value_term(ty, ConstantValueKind::Unit)
            }
            BoundStructuredExpressionKind::Absence => {
                self.intern_value_term(ty, ConstantValueKind::NullableAbsent)
            }
            BoundStructuredExpressionKind::Tuple => {
                let values = self.evaluate_elements(expression, operands)?;

                self.intern_value_term(ty, ConstantValueKind::tuple(values))
            }
            BoundStructuredExpressionKind::Array => {
                let values = self.evaluate_elements(expression, operands)?;

                self.intern_value_term(ty, ConstantValueKind::array(values))
            }
            BoundStructuredExpressionKind::RepeatedArray => {
                self.evaluate_repeated_array(expression, operands, ty)
            }
            BoundStructuredExpressionKind::ElementIndex => {
                self.evaluate_index(expression, operands)
            }
            BoundStructuredExpressionKind::SliceIndex
            | BoundStructuredExpressionKind::NullablePropagation
            | BoundStructuredExpressionKind::Conditional
            | BoundStructuredExpressionKind::While
            | BoundStructuredExpressionKind::Loop
            | BoundStructuredExpressionKind::With
            | BoundStructuredExpressionKind::Borrow
            | BoundStructuredExpressionKind::TrustBoundary
            | BoundStructuredExpressionKind::Assertion
            | BoundStructuredExpressionKind::ResultPropagation
            | BoundStructuredExpressionKind::Catch
            | BoundStructuredExpressionKind::TypeFormConstruction
            | BoundStructuredExpressionKind::BooleanFold
            | BoundStructuredExpressionKind::Panic => {
                // TODO(BRA-122): Extend constant checking when this structured form gains constant semantics.
                Err(EvaluationFailure::invalid_expression(expression))
            }
        }
    }

    pub(super) fn evaluate_elements(
        &mut self,
        owner: BoundExpressionId,
        operands: &[BoundExpressionId],
    ) -> Result<Vec<ConstantValueId>, EvaluationFailure> {
        self.budget.charge_elements(owner, operands.len())?;

        operands
            .iter()
            .copied()
            .map(|operand| {
                let term = self.evaluate(operand)?;

                // TODO(BRA-122): Add aggregate constant terms before accepting open aggregates.
                self.closed_value(term, owner)
            })
            .collect()
    }

    pub(super) fn evaluate_repeated_array(
        &mut self,
        expression: BoundExpressionId,
        operands: &[BoundExpressionId],
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let [value, count] = operands else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let value = self.evaluate(*value)?;
        let value = self.closed_value(value, expression)?;
        let count = self.evaluate(*count)?;
        let count = self.closed_value(count, expression)?;
        let count = self.array_count(expression, count)?;

        self.budget.charge_elements(expression, count)?;

        self.intern_value_term(
            ty,
            ConstantValueKind::array(std::iter::repeat_n(value, count)),
        )
    }

    pub(super) fn array_count(
        &self,
        expression: BoundExpressionId,
        value: ConstantValueId,
    ) -> Result<usize, EvaluationFailure> {
        let data = self
            .request
            .semantic_values()
            .constant_value_data(value)
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        let ConstantValueKind::Integer(integer) = data.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        integer_to_usize(integer).ok_or_else(|| EvaluationFailure::invalid_expression(expression))
    }

    pub(super) fn evaluate_complex_literal(
        &mut self,
        expression: BoundExpressionId,
        binary: &bray_bound_tree::BoundBinaryExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        if !matches!(
            binary.operator(),
            BoundOperator::Add | BoundOperator::Subtract
        ) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let Some(representation) =
            type_representation(self.request, ty).map_err(EvaluationFailure::Infrastructure)?
        else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        if representation.numeric_kind() != Some(NumericRepresentationKind::Complex) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let [real, imaginary] = binary.operands() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let real = self.evaluate(*real)?;
        let real = self.closed_value(real, expression)?;
        let imaginary = self.evaluate(*imaginary)?;
        let imaginary = self.closed_value(imaginary, expression)?;
        let real = self.real_component(expression, real)?;
        let mut imaginary = self.real_component(expression, imaginary)?;

        if binary.operator() == BoundOperator::Subtract {
            imaginary = negate_real(imaginary);
        }

        self.intern_value_term(ty, ConstantValueKind::Complex { real, imaginary })
    }

    pub(super) fn real_component(
        &self,
        expression: BoundExpressionId,
        value: ConstantValueId,
    ) -> Result<RealConstantBits, EvaluationFailure> {
        let data = self
            .request
            .semantic_values()
            .constant_value_data(value)
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        match data.kind() {
            ConstantValueKind::Real(value) => Ok(*value),
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }
}
