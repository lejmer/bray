use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, BoundStructuredExpressionKind,
    ExpressionTypeStatus,
};
use bray_compiler_known::NumericRepresentationKind;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{
    ConstantValueData, ConstantValueId, ConstantValueKind, IntegerConstant, RealConstantBits,
    TypeData, TypeId,
};

use crate::constant::limits::EvaluationBudget;
use crate::constant::literal::{LiteralValueError, parse_literal};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, ConstantEvaluationInput,
    ConstantReferenceResolution, UnitCheckRequest, UnitCheckRoot,
};

pub(crate) fn evaluate_constant<C>(
    request: UnitCheckRequest<'_, C>,
    input: &ConstantEvaluationInput<'_>,
) -> CheckerOutcome<ConstantValueId>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let UnitCheckRoot::Expression(root) = request.root() else {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput,
        );
    };

    if input.expression_types().unit() != request.view().unit()
        || input.expression_types().kind() != request.view().kind()
        || !input.references_are_consistent()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput,
        );
    }

    let mut evaluator = Evaluator::new(request, input);

    match evaluator.evaluate(root) {
        Ok(value) => CheckerOutcome::complete(value, evaluator.diagnostics),
        Err(EvaluationFailure::Cancelled) => CheckerOutcome::Cancelled,
        Err(EvaluationFailure::Infrastructure(error)) => {
            CheckerOutcome::InfrastructureFailure(error)
        }
        Err(EvaluationFailure::Source { expression, kind }) => {
            let span = match expression_span(request, expression) {
                Ok(span) => span,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            evaluator.diagnostics.add(
                Diagnostic::new(
                    diagnostic_id(evaluator.diagnostics.len()),
                    kind,
                    SeverityKind::Error,
                )
                .with_primary_span(span),
            );

            match evaluator.recovery_value(root) {
                Ok(value) => CheckerOutcome::complete(value, evaluator.diagnostics),
                Err(error) => CheckerOutcome::InfrastructureFailure(error),
            }
        }
    }
}

struct Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: UnitCheckRequest<'view, C>,
    input: &'input ConstantEvaluationInput<'types>,
    budget: EvaluationBudget,
    diagnostics: DiagnosticBag,
}

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new(
        request: UnitCheckRequest<'view, C>,
        input: &'input ConstantEvaluationInput<'types>,
    ) -> Self {
        Self {
            request,
            input,
            budget: EvaluationBudget::new(input),
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn evaluate(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
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
            // TODO(checker): Consume BRA-205 selection facts for non-literal binary operations.
            BoundExpression::Binary(binary) => {
                self.evaluate_complex_literal(expression, binary, ty)
            }
            BoundExpression::Structured(structured) => {
                self.evaluate_structured(expression, structured.kind(), structured.operands(), ty)
            }
            // TODO(checker): Evaluate selection-dependent constant forms after BRA-205.
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    fn evaluate_literal(
        &mut self,
        expression: BoundExpressionId,
        literal: bray_bound_tree::BoundLiteralExpression,
        ty: TypeId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
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

        let kind = parse_literal(
            literal.kind(),
            spelling,
            representation,
            self.input.target_integer_width_bits(),
        )
        .map_err(|error| {
            let kind = match error {
                LiteralValueError::Invalid => DiagnosticKind::CheckingInvalidConstantExpression,
                LiteralValueError::NotRepresentable => {
                    DiagnosticKind::CheckingConstantLiteralNotRepresentable
                }
                LiteralValueError::SizeLimitExceeded => {
                    DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded
                }
                LiteralValueError::TargetIntegerWidthRequired => {
                    return EvaluationFailure::Infrastructure(
                        CheckerInfrastructureError::InvalidConstantEvaluationInput,
                    );
                }
            };

            EvaluationFailure::Source { expression, kind }
        })?;

        self.intern(ty, kind)
    }

    fn evaluate_reference(
        &self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
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

                Ok(value)
            }
            Some(ConstantReferenceResolution::Cycle) => Err(EvaluationFailure::Source {
                expression,
                kind: DiagnosticKind::CheckingCyclicConstantDefinition,
            }),
            None => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    fn evaluate_structured(
        &mut self,
        expression: BoundExpressionId,
        kind: BoundStructuredExpressionKind,
        operands: &[BoundExpressionId],
        ty: TypeId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        match kind {
            BoundStructuredExpressionKind::Unit => self.intern(ty, ConstantValueKind::Unit),
            BoundStructuredExpressionKind::Absence => {
                self.intern(ty, ConstantValueKind::NullableAbsent)
            }
            BoundStructuredExpressionKind::Tuple => {
                let values = self.evaluate_elements(expression, operands)?;

                self.intern(ty, ConstantValueKind::tuple(values))
            }
            BoundStructuredExpressionKind::Array => {
                let values = self.evaluate_elements(expression, operands)?;

                self.intern(ty, ConstantValueKind::array(values))
            }
            BoundStructuredExpressionKind::RepeatedArray => {
                self.evaluate_repeated_array(expression, operands, ty)
            }
            // TODO(checker): Evaluate selected construction, projection, and control forms after BRA-205.
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    fn evaluate_elements(
        &mut self,
        owner: BoundExpressionId,
        operands: &[BoundExpressionId],
    ) -> Result<Vec<ConstantValueId>, EvaluationFailure> {
        self.budget.charge_elements(owner, operands.len())?;

        operands
            .iter()
            .copied()
            .map(|operand| self.evaluate(operand))
            .collect()
    }

    fn evaluate_repeated_array(
        &mut self,
        expression: BoundExpressionId,
        operands: &[BoundExpressionId],
        ty: TypeId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        let [value, count] = operands else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let value = self.evaluate(*value)?;
        let count = self.evaluate(*count)?;
        let count = self.array_count(expression, count)?;

        self.budget.charge_elements(expression, count)?;

        self.intern(
            ty,
            ConstantValueKind::array(std::iter::repeat_n(value, count)),
        )
    }

    fn array_count(
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

    fn evaluate_complex_literal(
        &mut self,
        expression: BoundExpressionId,
        binary: &bray_bound_tree::BoundBinaryExpression,
        ty: TypeId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
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
        let imaginary = self.evaluate(*imaginary)?;
        let real = self.real_component(expression, real)?;
        let mut imaginary = self.real_component(expression, imaginary)?;

        if binary.operator() == BoundOperator::Subtract {
            imaginary = negate_real(imaginary);
        }

        self.intern(ty, ConstantValueKind::Complex { real, imaginary })
    }

    fn real_component(
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

    fn expression_type(&self, expression: BoundExpressionId) -> Result<TypeId, EvaluationFailure> {
        let Some(result) = self.input.expression_types().expression(expression) else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        };

        if result.status() == ExpressionTypeStatus::Recovered {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        Ok(result.ty())
    }

    fn recovery_value(
        &self,
        root: BoundExpressionId,
    ) -> Result<ConstantValueId, CheckerInfrastructureError> {
        let ty = self
            .input
            .expression_types()
            .expression(root)
            .map(|result| result.ty())
            .map_or_else(
                || self.request.semantic_values().intern_type(TypeData::Error),
                Ok,
            )
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        self.request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Error))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }

    fn intern(
        &self,
        ty: TypeId,
        kind: ConstantValueKind,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        self.request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, kind))
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })
    }

    fn observe_cancellation(&self) -> Result<(), EvaluationFailure> {
        if self.request.is_cancelled() {
            Err(EvaluationFailure::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub(super) enum EvaluationFailure {
    Cancelled,
    Infrastructure(CheckerInfrastructureError),
    Source {
        expression: BoundExpressionId,
        kind: DiagnosticKind,
    },
}

impl EvaluationFailure {
    const fn invalid_expression(expression: BoundExpressionId) -> Self {
        Self::Source {
            expression,
            kind: DiagnosticKind::CheckingInvalidConstantExpression,
        }
    }
}

fn integer_to_usize(integer: &IntegerConstant) -> Option<usize> {
    if integer.sign() != bray_symbols::IntegerSign::NonNegative {
        return None;
    }

    let mut value = 0_usize;

    for byte in integer.magnitude() {
        value = value.checked_mul(256)?.checked_add(usize::from(*byte))?;
    }

    Some(value)
}

fn negate_real(value: RealConstantBits) -> RealConstantBits {
    match value {
        RealConstantBits::Binary16(bits) => RealConstantBits::Binary16(bits ^ (1 << 15)),
        RealConstantBits::Binary32(bits) => RealConstantBits::Binary32(bits ^ (1 << 31)),
        RealConstantBits::Binary64(bits) => RealConstantBits::Binary64(bits ^ (1 << 63)),
        RealConstantBits::Binary128(mut bytes) => {
            bytes[0] ^= 1 << 7;

            RealConstantBits::Binary128(bytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBinaryExpression, BoundExpression, BoundExpressionId, BoundLiteralExpression,
        BoundLiteralKind, BoundNameExpression, BoundNodeOrigin, BoundOperator,
        BoundReferenceTarget, BoundStructuredExpression, BoundStructuredExpressionKind,
        BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitKey, BoundUnitRoot,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
    use bray_diagnostics::DiagnosticKind;
    use bray_parser::parse_source_unit;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
    use bray_symbols::{
        AnySymbolId, ConstantSymbolId, ConstantValueData, ConstantValueKind, LocalScopeBoundary,
        LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
        LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, RealConstantBits,
        SymbolFactKind, SymbolId, SymbolKey, SymbolKind, SymbolRootKey, TypeId,
    };
    use bray_syntax::LiteralExpressionSyntax;

    use crate::representation::representation_type;
    use crate::test_support::{TestCheckerContext, push_expression, semantic_values, tuple_type};
    use crate::{
        CheckerInfrastructureError, CheckerOutcome, ConstantEvaluationInput,
        ConstantEvaluationLimits, ConstantEvaluator, ConstantReferenceResolution,
        DeclaredUnitCheckEntry, DefaultConstantEvaluator, DefaultExpressionTypeChecker,
        ExpressionTypeChecker, ExpressionTypeExpectation, ExpressionTypeInput,
        UnitCheckEntryContext, UnitCheckRequest,
    };

    #[test]
    fn evaluation_publishes_canonical_typed_literal_values() {
        let (unit, root, context) = literal_unit(
            BoundUnitId::new(90),
            "module example;\nconst value: u16 = 0x00_ff;\n",
            BoundLiteralKind::Integer,
        );
        let expected = representation(&unit, &context, RepresentationRole::ScalarU16);
        let (types, result) = evaluate(&unit, root, &context, expected, None);
        let value = constant_value(*result.value());

        assert!(!types.is_recovered());
        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let ConstantValueKind::Integer(integer) = value.kind() else {
            panic!("integer literal must publish an integer constant");
        };

        assert_eq!(value.ty(), expected);
        assert_eq!(integer.magnitude(), &[0xff]);
    }

    #[test]
    fn complex_literals_publish_selected_component_bits() {
        let source = "module example;\nconst value: c64 = 1.5 + 2.0i;\n";
        let (unit, root, context) =
            expression_unit(BoundUnitId::new(91), source, |tree, origins, _| {
                let [real, imaginary] = origins else {
                    panic!("complex source must contain two literals");
                };
                let real = push_expression(
                    tree,
                    BoundExpression::Literal(BoundLiteralExpression::new(
                        real.origin,
                        real.spelling_range,
                        BoundLiteralKind::Real,
                        None,
                        false,
                    )),
                );
                let imaginary = push_expression(
                    tree,
                    BoundExpression::Literal(BoundLiteralExpression::new(
                        imaginary.origin,
                        imaginary.spelling_range,
                        BoundLiteralKind::Imaginary,
                        None,
                        false,
                    )),
                );

                push_expression(
                    tree,
                    BoundExpression::Binary(BoundBinaryExpression::new(
                        first_origin(origins),
                        BoundOperator::Add,
                        [real, imaginary],
                        None,
                        false,
                    )),
                )
            });
        let expected = representation(&unit, &context, RepresentationRole::ScalarC64);
        let (_, result) = evaluate(&unit, root, &context, expected, None);
        let value = constant_value(*result.value());

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );
        assert_eq!(
            value.kind(),
            &ConstantValueKind::Complex {
                real: RealConstantBits::Binary32(0x3fc0_0000),
                imaginary: RealConstantBits::Binary32(0x4000_0000),
            }
        );
    }

    #[test]
    fn aggregate_evaluation_is_bounded_and_recovers_with_a_typed_error_value() {
        let source = "module example;\nconst value: (u16, u16) = (1, 2);\n";
        let (unit, root, context) =
            expression_unit(BoundUnitId::new(92), source, |tree, origins, _| {
                let elements = origins
                    .iter()
                    .map(|origin| {
                        push_expression(
                            tree,
                            BoundExpression::Literal(BoundLiteralExpression::new(
                                origin.origin,
                                origin.spelling_range,
                                BoundLiteralKind::Integer,
                                None,
                                false,
                            )),
                        )
                    })
                    .collect::<Vec<_>>();

                push_expression(
                    tree,
                    BoundExpression::Structured(BoundStructuredExpression::new(
                        first_origin(origins),
                        BoundStructuredExpressionKind::Tuple,
                        elements,
                        [],
                        [],
                        None,
                        false,
                    )),
                )
            });
        let element = representation(&unit, &context, RepresentationRole::ScalarU16);
        let expected = tuple_type([element, element]);
        let limits = ConstantEvaluationLimits::new(16, 1, 64);
        let (_, result) = evaluate(&unit, root, &context, expected, Some(limits));
        let value = constant_value(*result.value());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingConstantAggregateLimitExceeded)
                .count(),
            1
        );
        assert_eq!(value.ty(), expected);
        assert_eq!(value.kind(), &ConstantValueKind::Error);
    }

    #[test]
    fn operation_and_literal_budgets_produce_distinct_diagnostics() {
        let (unit, root, context) = literal_unit(
            BoundUnitId::new(93),
            "module example;\nconst value: u16 = 255;\n",
            BoundLiteralKind::Integer,
        );
        let expected = representation(&unit, &context, RepresentationRole::ScalarU16);
        let cases = [
            (
                ConstantEvaluationLimits::new(0, 16, 16),
                DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
            ),
            (
                ConstantEvaluationLimits::new(16, 16, 2),
                DiagnosticKind::CheckingConstantLiteralSizeLimitExceeded,
            ),
        ];

        for (limits, diagnostic_kind) in cases {
            let (_, result) = evaluate(&unit, root, &context, expected, Some(limits));
            let value = constant_value(*result.value());

            assert_eq!(result.diagnostics().by_kind(diagnostic_kind).count(), 1);
            assert_eq!(value.kind(), &ConstantValueKind::Error);
        }
    }

    #[test]
    fn cancellation_publishes_neither_a_value_nor_diagnostics() {
        let (unit, root, context) = literal_unit(
            BoundUnitId::new(94),
            "module example;\nconst value: u16 = 1;\n",
            BoundLiteralKind::Integer,
        );
        let expected = representation(&unit, &context, RepresentationRole::ScalarU16);
        let types = checked_types(&unit, root, &context, expected);
        let input = ConstantEvaluationInput::new(&types);
        let cancelled = TestCheckerContext::new(true);
        let entry = checker_entry(&unit);
        let request = match UnitCheckRequest::new(&unit, &entry, &cancelled) {
            Ok(request) => request,
            Err(error) => panic!("constant checker request must be valid: {error:?}"),
        };
        let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

        assert!(matches!(outcome, CheckerOutcome::Cancelled));
    }

    #[test]
    fn constant_references_reuse_values_and_recover_dependency_cycles() {
        let (seed, seed_root, seed_context) = literal_unit(
            BoundUnitId::new(95),
            "module example;\nconst value: u16 = 1;\n",
            BoundLiteralKind::Integer,
        );
        let expected = representation(&seed, &seed_context, RepresentationRole::ScalarU16);
        let (_, seed_result) = evaluate(&seed, seed_root, &seed_context, expected, None);
        let referenced_value = *seed_result.value();
        let (unit, root, context) = reference_unit(BoundUnitId::new(96), expected);

        let result = evaluate_reference(
            &unit,
            root,
            &context,
            expected,
            ConstantReferenceResolution::Cycle,
        );
        let value = constant_value(*result.value());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingCyclicConstantDefinition)
                .count(),
            1
        );
        assert_eq!(value.kind(), &ConstantValueKind::Error);

        let (unit, root, context) = reference_unit(BoundUnitId::new(97), expected);
        let result = evaluate_reference(
            &unit,
            root,
            &context,
            expected,
            ConstantReferenceResolution::Value(referenced_value),
        );

        assert!(result.diagnostics().is_empty());
        assert_eq!(*result.value(), referenced_value);

        let types = checked_types(&unit, root, &context, expected);
        let input = ConstantEvaluationInput::new(&types).with_references([
            (root, ConstantReferenceResolution::Cycle),
            (root, ConstantReferenceResolution::Value(referenced_value)),
        ]);
        let entry = checker_entry(&unit);
        let request = match UnitCheckRequest::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker request must be valid: {error:?}"),
        };
        let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

        assert_eq!(
            outcome,
            CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput
            )
        );
    }

    fn reference_unit(
        unit: BoundUnitId,
        expected: TypeId,
    ) -> (BoundUnit, BoundExpressionId, TestCheckerContext) {
        let source = "module example;\nconst value: u16 = other;\n";

        expression_unit(unit, source, |tree, _, origin| {
            let target = BoundReferenceTarget::Surface(AnySymbolId::from(
                ConstantSymbolId::from_symbol_id(SymbolId::new(1)),
            ));

            push_expression(
                tree,
                BoundExpression::Name(BoundNameExpression::new(
                    origin,
                    target,
                    Some(expected),
                    false,
                )),
            )
        })
    }

    fn evaluate_reference(
        unit: &BoundUnit,
        root: BoundExpressionId,
        context: &TestCheckerContext,
        expected: TypeId,
        resolution: ConstantReferenceResolution,
    ) -> bray_diagnostics::DiagnosticResult<bray_symbols::ConstantValueId> {
        let types = checked_types(unit, root, context, expected);
        let input = ConstantEvaluationInput::new(&types).with_references([(root, resolution)]);
        let entry = checker_entry(unit);
        let request = match UnitCheckRequest::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker request must be valid: {error:?}"),
        };
        let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);
        let Some(result) = outcome.into_result() else {
            panic!("constant-reference evaluation must complete");
        };

        result
    }

    fn literal_unit(
        unit: BoundUnitId,
        source: &str,
        kind: BoundLiteralKind,
    ) -> (BoundUnit, BoundExpressionId, TestCheckerContext) {
        expression_unit(unit, source, |tree, origins, _| {
            let literal = first_literal(origins);

            push_expression(
                tree,
                BoundExpression::Literal(BoundLiteralExpression::new(
                    literal.origin,
                    literal.spelling_range,
                    kind,
                    None,
                    false,
                )),
            )
        })
    }

    fn expression_unit(
        unit: BoundUnitId,
        source_text: &str,
        build: impl FnOnce(
            &mut BoundTreeBuilder,
            &[LiteralSource],
            BoundNodeOrigin,
        ) -> BoundExpressionId,
    ) -> (BoundUnit, BoundExpressionId, TestCheckerContext) {
        let source = snapshot(source_text);
        let parsed = parse_source_unit(&source);

        assert!(parsed.diagnostics().is_empty());

        let literals =
            bray_testing::syntax_descendants::<LiteralExpressionSyntax>(parsed.source_unit());
        let origins = literals
            .iter()
            .map(|literal| {
                let origin = BoundNodeOrigin::source(bray_bound_tree::BoundSourceAnchor::new(
                    SyntaxAnchor::from_node(literal),
                    source.version(),
                ));
                let Some(token) = literal.literal_token() else {
                    panic!("parsed literal expression must contain its token");
                };

                LiteralSource {
                    origin,
                    spelling_range: token.range(),
                }
            })
            .collect::<Vec<_>>();
        let key = constant_key(&source, parsed.source_unit());
        let mut tree = BoundTreeBuilder::new(unit);
        let root = build(&mut tree, &origins, BoundNodeOrigin::source(key.source()));
        let local_symbols = local_symbols(unit, &key);
        let unit = BoundUnit::try_new(
            key,
            tree.finish(),
            local_symbols,
            [],
            BoundUnitRoot::Expression(root),
        );
        let Ok(unit) = unit else {
            panic!("constant test unit must be valid");
        };

        (unit, root, TestCheckerContext::with_source(source))
    }

    fn snapshot(text: &str) -> SourceSnapshot {
        let source = SourceSnapshot::new(
            SourceId::new(0),
            SourceIdentity::new(0),
            SourceOrigin::virtual_source("constant-evaluation-test"),
            SourceVersion::new(1),
            text,
        );

        match source {
            Ok(source) => source,
            Err(error) => panic!("test source must fit: {error:?}"),
        }
    }

    fn constant_key(
        source: &SourceSnapshot,
        source_unit: &bray_syntax::SourceUnitSyntax,
    ) -> BoundUnitKey {
        let declarations = discover_source_unit_declarations(source_unit);

        assert!(declarations.diagnostics().is_empty());

        let [part] = declarations.chunk().module_parts() else {
            panic!("test source must contain one module part");
        };
        let source_anchor =
            bray_bound_tree::BoundSourceAnchor::new(part.syntax_anchor(), source.version());
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity must be non-empty");
        };
        let Some(path) = ModulePathKey::try_new(["example"]) else {
            panic!("test module path must be non-empty");
        };

        let module = SymbolKey::module(SymbolRootKey::Package(package), path);
        let Some(owner) =
            SymbolKey::source_declaration(module, SymbolKind::Constant, DeclarationId::new(0))
        else {
            panic!("constant symbols must be source-declared");
        };
        let Some(key) = BoundUnitKey::constant_template(owner, source_anchor) else {
            panic!("constants must support constant-template units");
        };

        key
    }

    fn local_symbols(unit: BoundUnitId, key: &BoundUnitKey) -> bray_symbols::LocalSymbolSnapshot {
        let region = LocalSymbolRegionId::new(unit.raw());
        let region_key = LocalSymbolRegionKey::try_new(
            key.declared_owner().clone(),
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
            [key.source().syntax()],
            None,
        );
        let Some(region_key) = region_key else {
            panic!("constant test key must form a local symbol region");
        };
        let mut symbols = LocalSymbolSnapshotBuilder::new(region, region_key);

        if let Err(error) = symbols.push_scope(
            None,
            LocalScopeBoundary::Root,
            key.source().syntax(),
            key.source().syntax().full_range().start(),
        ) {
            panic!("constant test root scope must validate: {error:?}");
        }

        match symbols.finish() {
            Ok(symbols) => symbols,
            Err(error) => panic!("constant test symbols must validate: {error:?}"),
        }
    }

    fn evaluate(
        unit: &BoundUnit,
        root: BoundExpressionId,
        context: &TestCheckerContext,
        expected: TypeId,
        limits: Option<ConstantEvaluationLimits>,
    ) -> (
        bray_bound_tree::CheckedExpressionTypes,
        bray_diagnostics::DiagnosticResult<bray_symbols::ConstantValueId>,
    ) {
        let types = checked_types(unit, root, context, expected);
        let mut input = ConstantEvaluationInput::new(&types);

        if let Some(limits) = limits {
            input = input.with_limits(limits);
        }

        let entry = checker_entry(unit);
        let request = match UnitCheckRequest::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker request must be valid: {error:?}"),
        };
        let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);
        let Some(result) = outcome.into_result() else {
            panic!("constant evaluation must complete");
        };

        (types, result)
    }

    fn checked_types(
        unit: &BoundUnit,
        root: BoundExpressionId,
        context: &TestCheckerContext,
        expected: TypeId,
    ) -> bray_bound_tree::CheckedExpressionTypes {
        let input = ExpressionTypeInput::new()
            .with_expectations([ExpressionTypeExpectation::new(root, expected)]);
        let entry = checker_entry(unit);
        let request = match UnitCheckRequest::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker request must be valid: {error:?}"),
        };
        let outcome = DefaultExpressionTypeChecker.check_expression_types(request, &input);
        let Some(result) = outcome.into_result() else {
            panic!("constant expression typing must complete");
        };

        assert!(result.diagnostics().is_empty());

        result.into_parts().0
    }

    fn checker_entry(unit: &BoundUnit) -> UnitCheckEntryContext {
        let owner = AnySymbolId::from(ConstantSymbolId::from_symbol_id(SymbolId::new(0)));

        UnitCheckEntryContext::ConstantTemplate(DeclaredUnitCheckEntry::new(
            unit.key().clone(),
            owner,
            owner,
        ))
    }

    fn representation(
        unit: &BoundUnit,
        context: &TestCheckerContext,
        role: RepresentationRole,
    ) -> TypeId {
        let entry = checker_entry(unit);
        let request = match UnitCheckRequest::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker request must be valid: {error:?}"),
        };

        match representation_type(request, role) {
            Ok(ty) => ty,
            Err(error) => panic!("test representation must be available: {error:?}"),
        }
    }

    fn constant_value(id: bray_symbols::ConstantValueId) -> std::sync::Arc<ConstantValueData> {
        match semantic_values().constant_value_data(id) {
            Ok(value) => value,
            Err(error) => panic!("test constant value must be available: {error:?}"),
        }
    }

    fn first_literal(origins: &[LiteralSource]) -> LiteralSource {
        let Some(origin) = origins.first() else {
            panic!("test source must contain a literal origin");
        };

        *origin
    }

    fn first_origin(origins: &[LiteralSource]) -> BoundNodeOrigin {
        first_literal(origins).origin
    }

    #[derive(Clone, Copy)]
    struct LiteralSource {
        origin: BoundNodeOrigin,
        spelling_range: bray_source::TextRange,
    }
}
