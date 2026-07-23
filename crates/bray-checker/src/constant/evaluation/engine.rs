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
        .finalize_closed_value(evaluated.term, evaluated.root)
    {
        Ok(value) => value,
        Err(EvaluationFailure::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(EvaluationFailure::Cancelled) => return CheckerOutcome::Cancelled,
        Err(EvaluationFailure::Source { expression, kind }) => {
            let value = match evaluated.evaluator.recovery_value(evaluated.root) {
                Ok(value) => value,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            let mut diagnostics = evaluated.evaluator.diagnostics;

            let span = match expression_span(request, expression) {
                Ok(span) => span,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            diagnostics.add(
                Diagnostic::new(diagnostic_id(diagnostics.len()), kind, SeverityKind::Error)
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

    let root = match (input.root(), request.root()) {
        (Some(root), _) if request.view().expression(root).is_some() => root,
        (None, CheckerUnitRoot::Expression(root)) => root,
        _ => {
            return Err(EvaluationAbort::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        }
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
                self.evaluate_structured(expression, structured, ty)
            }
            BoundExpression::MemberAccess(member) => {
                self.evaluate_member_projection(expression, member, ty)
            }
            BoundExpression::Call(_) => self.evaluate_construction(expression, ty),
            BoundExpression::StructConstruction(_) | BoundExpression::LeadingDotVariant(_) => {
                self.evaluate_construction(expression, ty)
            }
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
                // TODO(BRA-246): Extend constant checking when this expression category gains constant semantics.
                Err(EvaluationFailure::invalid_expression(expression))
            }
        }
    }

    pub(super) const fn retain_open_terms(&self) -> bool {
        self.retain_target_literals
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
        structured: &bray_bound_tree::BoundStructuredExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let operands = structured.operands();

        match structured.kind() {
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
                self.evaluate_index(expression, structured, ty)
            }
            BoundStructuredExpressionKind::SliceIndex => {
                self.evaluate_index(expression, structured, ty)
            }
            BoundStructuredExpressionKind::TypeFormConstruction => {
                self.evaluate_construction(expression, ty)
            }
            BoundStructuredExpressionKind::NullablePropagation
            | BoundStructuredExpressionKind::Conditional
            | BoundStructuredExpressionKind::While
            | BoundStructuredExpressionKind::Loop
            | BoundStructuredExpressionKind::With
            | BoundStructuredExpressionKind::Borrow
            | BoundStructuredExpressionKind::TrustBoundary
            | BoundStructuredExpressionKind::Assertion
            | BoundStructuredExpressionKind::ResultPropagation
            | BoundStructuredExpressionKind::Catch
            | BoundStructuredExpressionKind::BooleanFold
            | BoundStructuredExpressionKind::Panic => {
                // TODO(BRA-246): Extend constant checking when this structured form gains constant semantics.
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

                // TODO(BRA-246): Add aggregate constant terms before accepting open aggregates.
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

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32};
    use std::sync::Mutex;

    use bray_bound_tree::{
        BoundBinaryExpression, BoundConversionExpression, BoundExpression, BoundExpressionId,
        BoundLiteralExpression, BoundLiteralKind, BoundNameExpression, BoundNodeOrigin,
        BoundOperator, BoundReferenceTarget, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTreeBuilder, BoundUnit, BoundUnitId, BoundUnitKey,
        BoundUnitRoot, ConversionTarget, OperatorTarget, SelectedConversion, SelectedOperation,
        SemanticSelection, SemanticSelectionEntry,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
    use bray_diagnostics::DiagnosticKind;
    use bray_parser::parse_source_unit;
    use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
    use bray_symbols::{
        AnySymbolId, ConstantSymbolId, ConstantTermData, ConstantValueData, ConstantValueKind,
        GenericConstParameterSymbolId, LocalScopeBoundary, LocalSymbolRegionId,
        LocalSymbolRegionKey, LocalSymbolRegionRole, LocalSymbolSnapshotBuilder, ModulePathKey,
        PackageIdentity, RealConstantBits, SymbolFactKind, SymbolId, SymbolKey, SymbolKind,
        SymbolRootKey, TargetSizedIntegerType, TraitCallableFulfillmentSymbolId,
        TraitCallableMemberSymbolId, TraitSymbolId, TypeId,
    };
    use bray_syntax::LiteralExpressionSyntax;
    use bray_target::{
        Endianness, ObjectFormat, TargetArchitecture, TargetIdentity, TargetMachineProperties,
        TargetProfile,
    };

    use crate::representation::representation_type;
    use crate::test_support::{
        TestCheckerContext, callable_instance, compiler_known_symbol, push_expression,
        semantic_values, trait_callable_instance, tuple_type,
    };
    use crate::{
        CheckerFactResult, CheckerInfrastructureError, CheckerOutcome, CheckerUnitView,
        ConstantCallRequest, ConstantCallResolution, ConstantCallResolver, ConstantChecker,
        ConstantEvaluationInput, ConstantEvaluationLimits, ConstantEvaluator,
        ConstantReferenceResolution, DeclaredUnitContext, DefaultConstantChecker,
        DefaultConstantEvaluator, DefaultExpressionTypeChecker, ExpressionTypeChecker,
        ExpressionTypeExpectation, ExpressionTypeInput, SemanticUnitContext,
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
        assert_eq!(context.target_observations(), 0);
    }

    #[test]
    fn evaluation_uses_the_selected_target_integer_width() {
        let (unit, root, context) = literal_unit(
            BoundUnitId::new(101),
            "module example;\nconst value: usize = 4294967296;\n",
            BoundLiteralKind::Integer,
        );

        let context = context.with_selected_target(target_profile_32());
        let expected = representation(&unit, &context, RepresentationRole::ScalarUsize);

        let (_, result) = evaluate(&unit, root, &context, expected, None);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingConstantLiteralNotRepresentable)
                .count(),
            1
        );

        assert_eq!(context.target_observations(), 1);
    }

    #[test]
    fn open_checking_retains_target_sized_literals_without_demanding_target_facts() {
        let (unit, root, context) = literal_unit(
            BoundUnitId::new(104),
            "module example;\nconst value: usize = 4294967296;\n",
            BoundLiteralKind::Integer,
        );

        let expected = representation(&unit, &context, RepresentationRole::ScalarUsize);
        let term = check_term(&unit, root, &context, expected);
        let data = semantic_values().constant_term_data(*term.value());

        let Ok(data) = data else {
            panic!("checked constant term must be available");
        };

        assert!(term.diagnostics().is_empty());

        assert!(matches!(
            data.as_ref(),
            ConstantTermData::IntegerLiteral {
                ty: TargetSizedIntegerType::Usize,
                ..
            }
        ));

        assert_eq!(context.target_observations(), 0);
    }

    #[test]
    fn exact_integer_intermediates_can_return_to_the_selected_range() {
        let (seed, _, seed_context) = literal_unit(
            BoundUnitId::new(105),
            "module example;\nconst value: u8 = 0;\n",
            BoundLiteralKind::Integer,
        );

        let expected = representation(&seed, &seed_context, RepresentationRole::ScalarU8);
        let mut addition = None;

        let (unit, root, context) = expression_unit(
            BoundUnitId::new(106),
            "module example;\nconst value: u8 = 255 + 1 - 1;\n",
            |tree, origins, origin| {
                let literals = origins
                    .iter()
                    .map(|literal| {
                        push_expression(
                            tree,
                            BoundExpression::Literal(BoundLiteralExpression::new(
                                literal.origin,
                                literal.spelling_range,
                                BoundLiteralKind::Integer,
                                Some(expected),
                                false,
                            )),
                        )
                    })
                    .collect::<Vec<_>>();

                let [left, middle, right] = literals.as_slice() else {
                    panic!("regression source must contain three literals");
                };

                let add = push_expression(
                    tree,
                    BoundExpression::Binary(BoundBinaryExpression::new(
                        origin,
                        BoundOperator::Add,
                        [*left, *middle],
                        Some(expected),
                        false,
                    )),
                );

                addition = Some(add);

                push_expression(
                    tree,
                    BoundExpression::Binary(BoundBinaryExpression::new(
                        origin,
                        BoundOperator::Subtract,
                        [add, *right],
                        Some(expected),
                        false,
                    )),
                )
            },
        );

        let Some(addition) = addition else {
            panic!("regression unit must contain the addition");
        };

        let types = checked_types(&unit, root, &context, expected);

        let selections = bray_bound_tree::CheckedSemanticSelections::try_new(
            &unit,
            &types,
            [
                SemanticSelectionEntry::new(
                    addition,
                    SemanticSelection::Operation(SelectedOperation::Operator {
                        target: OperatorTarget::BuiltIn(BoundOperator::Add),
                        result_type: expected,
                    }),
                ),
                SemanticSelectionEntry::new(
                    root,
                    SemanticSelection::Operation(SelectedOperation::Operator {
                        target: OperatorTarget::BuiltIn(BoundOperator::Subtract),
                        result_type: expected,
                    }),
                ),
            ],
        );

        let Ok(selections) = selections else {
            panic!("regression selections must be valid");
        };

        let input = ConstantEvaluationInput::new(&types, &selections);
        let entry = checker_entry(&unit);

        let request = match CheckerUnitView::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let Some(result) = DefaultConstantEvaluator
            .evaluate_constant(request, &input)
            .into_result()
        else {
            panic!("regression evaluation must complete");
        };

        assert!(
            result.diagnostics().is_empty(),
            "{:?}",
            result.diagnostics()
        );

        let value = constant_value(*result.value());

        assert_eq!(
            value.kind(),
            &ConstantValueKind::Integer(bray_symbols::IntegerConstant::new(
                bray_symbols::IntegerSign::NonNegative,
                [0xff],
            ))
        );
    }

    #[test]
    fn selected_trait_operations_preserve_and_evaluate_the_exact_fulfillment() {
        let (seed, _, seed_context) = literal_unit(
            BoundUnitId::new(109),
            "module example;\nconst value: i32 = 0;\n",
            BoundLiteralKind::Integer,
        );

        let expected = representation(&seed, &seed_context, RepresentationRole::ScalarI32);

        let (unit, root, context) = expression_unit(
            BoundUnitId::new(110),
            "module example;\nconst value: i32 = 1 + 2;\n",
            |tree, origins, origin| {
                let operands = origins
                    .iter()
                    .map(|literal| {
                        push_expression(
                            tree,
                            BoundExpression::Literal(BoundLiteralExpression::new(
                                literal.origin,
                                literal.spelling_range,
                                BoundLiteralKind::Integer,
                                Some(expected),
                                false,
                            )),
                        )
                    })
                    .collect::<Vec<_>>();

                push_expression(
                    tree,
                    BoundExpression::Binary(BoundBinaryExpression::new(
                        origin,
                        BoundOperator::Add,
                        operands,
                        Some(expected),
                        false,
                    )),
                )
            },
        );

        let member = trait_callable_instance(compiler_known_symbol::<TraitCallableMemberSymbolId>(
            "AddCall",
        ));

        let fulfillment = callable_instance(
            TraitCallableFulfillmentSymbolId::from_symbol_id(SymbolId::new(75)).into(),
        );

        let requirement = bray_symbols::testing::implementation_requirement(
            semantic_values(),
            compiler_known_symbol::<TraitSymbolId>("Add"),
            expected,
            expected,
        );
        let witness = bray_symbols::testing::implementation_instance(semantic_values(), 76);
        let types = checked_types(&unit, root, &context, expected);

        let selections = bray_bound_tree::CheckedSemanticSelections::try_new(
            &unit,
            &types,
            [SemanticSelectionEntry::new(
                root,
                SemanticSelection::Operation(SelectedOperation::Operator {
                    target: OperatorTarget::Trait {
                        operator: BoundOperator::Add,
                        member,
                        fulfillment,
                        requirement,
                        witness,
                    },
                    result_type: expected,
                }),
            )],
        )
        .unwrap_or_else(|error| panic!("trait operation selection must be valid: {error:?}"));

        let result_value = semantic_values()
            .intern_constant_value(ConstantValueData::new(
                expected,
                ConstantValueKind::Integer(bray_symbols::IntegerConstant::new(
                    bray_symbols::IntegerSign::NonNegative,
                    [3],
                )),
            ))
            .unwrap_or_else(|error| panic!("selected call result must intern: {error:?}"));

        let resolver = CapturingCallResolver::new(result_value);
        let input = ConstantEvaluationInput::new(&types, &selections).with_call_resolver(&resolver);
        let entry = checker_entry(&unit);

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("constant checker unit view must be valid: {error:?}"));

        let result = DefaultConstantEvaluator
            .evaluate_constant(request, &input)
            .into_result()
            .unwrap_or_else(|| panic!("selected trait operation must evaluate"));

        assert!(result.diagnostics().is_empty());
        assert_eq!(*result.value(), result_value);

        let requests = resolver.requests();

        let [request] = requests.as_slice() else {
            panic!("selected trait operation must request exactly one call");
        };

        assert_eq!(request.callable(), fulfillment);
        assert_eq!(request.selected_implementation(), Some(witness));
        assert_eq!(request.arguments().len(), 2);
        assert_eq!(request.result_type(), expected);

        let cycle_resolver = CycleCallResolver;

        let cycle_input =
            ConstantEvaluationInput::new(&types, &selections).with_call_resolver(&cycle_resolver);

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("constant checker unit view must be valid: {error:?}"));

        let cycle = DefaultConstantEvaluator
            .evaluate_constant(request, &cycle_input)
            .into_result()
            .unwrap_or_else(|| panic!("recursive selected call must recover"));

        assert_eq!(
            cycle
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingCyclicConstantDefinition)
                .count(),
            1
        );

        let limited_input = ConstantEvaluationInput::new(&types, &selections)
            .with_call_resolver(&resolver)
            .with_limits(ConstantEvaluationLimits::default().with_call_depth(0));

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("constant checker unit view must be valid: {error:?}"));

        let limited = DefaultConstantEvaluator
            .evaluate_constant(request, &limited_input)
            .into_result()
            .unwrap_or_else(|| panic!("exhausted selected call must recover"));

        assert_eq!(
            limited
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded)
                .count(),
            1
        );

        assert_eq!(resolver.requests().len(), 1);

        let ineligible_resolver = IneligibleCallResolver;
        let ineligible_input = ConstantEvaluationInput::new(&types, &selections)
            .with_call_resolver(&ineligible_resolver);

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("constant checker unit view must be valid: {error:?}"));

        let ineligible = DefaultConstantChecker
            .check_constant_term(request, &ineligible_input)
            .into_result()
            .unwrap_or_else(|| panic!("ineligible selected call must recover"));

        assert_eq!(
            ineligible
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingInvalidConstantExpression)
                .count(),
            1
        );
    }

    struct CapturingCallResolver {
        result: bray_symbols::ConstantValueId,
        requests: Mutex<Vec<ConstantCallRequest>>,
    }

    impl CapturingCallResolver {
        fn new(result: bray_symbols::ConstantValueId) -> Self {
            Self {
                result,
                requests: Mutex::new(Vec::new()),
            }
        }

        fn requests(&self) -> Vec<ConstantCallRequest> {
            self.requests
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        }
    }

    impl ConstantCallResolver for CapturingCallResolver {
        fn is_constant_callable(
            &self,
            _callable: bray_symbols::CallableInstanceData,
        ) -> CheckerFactResult<bool> {
            Ok(true)
        }

        fn resolve(
            &self,
            request: &ConstantCallRequest,
        ) -> CheckerFactResult<ConstantCallResolution> {
            self.requests
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(request.clone());

            Ok(ConstantCallResolution::Evaluated(
                bray_diagnostics::DiagnosticResult::without_diagnostics(self.result),
            ))
        }
    }

    struct CycleCallResolver;

    impl ConstantCallResolver for CycleCallResolver {
        fn is_constant_callable(
            &self,
            _callable: bray_symbols::CallableInstanceData,
        ) -> CheckerFactResult<bool> {
            Ok(true)
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerFactResult<ConstantCallResolution> {
            Ok(ConstantCallResolution::Cycle)
        }
    }

    struct IneligibleCallResolver;

    impl ConstantCallResolver for IneligibleCallResolver {
        fn is_constant_callable(
            &self,
            _callable: bray_symbols::CallableInstanceData,
        ) -> CheckerFactResult<bool> {
            Ok(false)
        }

        fn resolve(
            &self,
            _request: &ConstantCallRequest,
        ) -> CheckerFactResult<ConstantCallResolution> {
            panic!("ineligible calls must not be evaluated")
        }
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
        let selections = empty_selections(&unit, &types);

        let input = ConstantEvaluationInput::new(&types, &selections);
        let cancelled = TestCheckerContext::new(true);

        let entry = checker_entry(&unit);

        let request = match CheckerUnitView::new(&unit, &entry, &cancelled) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
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
        let selections = empty_selections(&unit, &types);

        let input = ConstantEvaluationInput::new(&types, &selections).with_references([
            (root, ConstantReferenceResolution::Cycle),
            (root, ConstantReferenceResolution::Value(referenced_value)),
        ]);

        let entry = checker_entry(&unit);

        let request = match CheckerUnitView::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

        assert_eq!(
            outcome,
            CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput
            )
        );
    }

    #[test]
    fn open_target_facts_are_valid_terms_but_not_closed_values() {
        let (seed, _, seed_context) = literal_unit(
            BoundUnitId::new(102),
            "module example;\nconst value: u16 = 1;\n",
            BoundLiteralKind::Integer,
        );

        let expected = representation(&seed, &seed_context, RepresentationRole::ScalarU16);
        let (unit, root, context) = reference_unit(BoundUnitId::new(103), expected);

        let target_fact = match semantic_values().intern_constant_term(
            ConstantTermData::TargetFact(ConstantSymbolId::from_symbol_id(SymbolId::new(2))),
        ) {
            Ok(term) => term,
            Err(error) => panic!("target-fact term must intern: {error:?}"),
        };

        let types = checked_types(&unit, root, &context, expected);
        let selections = empty_selections(&unit, &types);

        let input = ConstantEvaluationInput::new(&types, &selections)
            .with_references([(root, ConstantReferenceResolution::Term(target_fact))]);

        let entry = checker_entry(&unit);

        let request = match CheckerUnitView::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let Some(checked) = DefaultConstantChecker
            .check_constant_term(request, &input)
            .into_result()
        else {
            panic!("open constant checking must complete");
        };

        assert_eq!(*checked.value(), target_fact);
        assert!(checked.diagnostics().is_empty());

        let request = match CheckerUnitView::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let Some(closed) = DefaultConstantEvaluator
            .evaluate_constant(request, &input)
            .into_result()
        else {
            panic!("closed constant evaluation must complete with recovery");
        };

        assert_eq!(
            closed
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingInvalidConstantExpression)
                .count(),
            1
        );

        assert_eq!(
            constant_value(*closed.value()).kind(),
            &ConstantValueKind::Error
        );
    }

    #[test]
    fn open_scalar_conversions_publish_selected_conversion_terms() {
        let (seed, _, seed_context) = literal_unit(
            BoundUnitId::new(107),
            "module example;\nconst value: u32 = 1;\n",
            BoundLiteralKind::Integer,
        );

        let source_type = representation(&seed, &seed_context, RepresentationRole::ScalarU32);
        let target_type = representation(&seed, &seed_context, RepresentationRole::ScalarU64);

        let mut operand = None;

        let (unit, root, context) = expression_unit(
            BoundUnitId::new(108),
            "module example;\nconst value: u64 = 1;\n",
            |tree, _, origin| {
                let reference = push_expression(
                    tree,
                    BoundExpression::Name(BoundNameExpression::new(
                        origin,
                        BoundReferenceTarget::Surface(AnySymbolId::from(
                            ConstantSymbolId::from_symbol_id(SymbolId::new(1)),
                        )),
                        Some(source_type),
                        false,
                    )),
                );

                operand = Some(reference);

                push_expression(
                    tree,
                    BoundExpression::Conversion(BoundConversionExpression::new(
                        origin,
                        reference,
                        origin.source_anchor().syntax(),
                        Some(target_type),
                        Some(target_type),
                        false,
                    )),
                )
            },
        );

        let Some(operand) = operand else {
            panic!("conversion unit must contain its operand");
        };

        let types = checked_types(&unit, root, &context, target_type);

        let selections = bray_bound_tree::CheckedSemanticSelections::try_new(
            &unit,
            &types,
            [SemanticSelectionEntry::new(
                root,
                SemanticSelection::Operation(SelectedOperation::Conversion(
                    SelectedConversion::new(
                        source_type,
                        target_type,
                        ConversionTarget::BuiltInScalar,
                    ),
                )),
            )],
        )
        .unwrap_or_else(|error| panic!("conversion selection must be valid: {error:?}"));

        let parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(2));

        let parameter = semantic_values()
            .intern_constant_term(ConstantTermData::Parameter(parameter))
            .unwrap_or_else(|error| panic!("parameter term must intern: {error:?}"));

        let input = ConstantEvaluationInput::new(&types, &selections)
            .with_references([(operand, ConstantReferenceResolution::Term(parameter))]);

        let entry = checker_entry(&unit);

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("constant checker unit view must be valid: {error:?}"));

        let result = DefaultConstantChecker
            .check_constant_term(request, &input)
            .into_result()
            .unwrap_or_else(|| panic!("open conversion checking must complete"));

        let data = semantic_values()
            .constant_term_data(*result.value())
            .unwrap_or_else(|error| panic!("conversion term must be available: {error:?}"));

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            data.as_ref(),
            &ConstantTermData::Conversion {
                operand: parameter,
                target: target_type,
            }
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
        let selections = empty_selections(unit, &types);

        let input =
            ConstantEvaluationInput::new(&types, &selections).with_references([(root, resolution)]);

        let entry = checker_entry(unit);

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
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
        let selections = empty_selections(unit, &types);

        let mut input = ConstantEvaluationInput::new(&types, &selections);

        if let Some(limits) = limits {
            input = input.with_limits(limits);
        }

        let entry = checker_entry(unit);

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let outcome = DefaultConstantEvaluator.evaluate_constant(request, &input);

        let Some(result) = outcome.into_result() else {
            panic!("constant evaluation must complete");
        };

        (types, result)
    }

    fn check_term(
        unit: &BoundUnit,
        root: BoundExpressionId,
        context: &TestCheckerContext,
        expected: TypeId,
    ) -> bray_diagnostics::DiagnosticResult<bray_symbols::ConstantTermId> {
        let types = checked_types(unit, root, context, expected);
        let selections = empty_selections(unit, &types);
        let input = ConstantEvaluationInput::new(&types, &selections);
        let entry = checker_entry(unit);

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let outcome = DefaultConstantChecker.check_constant_term(request, &input);

        let Some(result) = outcome.into_result() else {
            panic!("constant checking must complete");
        };

        result
    }

    fn empty_selections(
        unit: &BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
    ) -> bray_bound_tree::CheckedSemanticSelections {
        match bray_bound_tree::CheckedSemanticSelections::try_new(unit, types, []) {
            Ok(selections) => selections,
            Err(error) => panic!("empty semantic selections must be valid: {error:?}"),
        }
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

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
        };

        let outcome = DefaultExpressionTypeChecker.check_expression_types(request, &input);

        let Some(result) = outcome.into_result() else {
            panic!("constant expression typing must complete");
        };

        assert!(result.diagnostics().is_empty());

        result.into_parts().0
    }

    fn checker_entry(unit: &BoundUnit) -> SemanticUnitContext {
        let owner = AnySymbolId::from(ConstantSymbolId::from_symbol_id(SymbolId::new(0)));

        SemanticUnitContext::ConstantTemplate(DeclaredUnitContext::new(
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

        let request = match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => request,
            Err(error) => panic!("constant checker unit view must be valid: {error:?}"),
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

    fn target_profile_32() -> TargetProfile {
        let Some(identity) = TargetIdentity::try_new("i686-unknown-linux-gnu") else {
            panic!("test target identity must be valid");
        };

        let pointer_width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);

        let Some(machine) = TargetMachineProperties::try_new(
            TargetArchitecture::X86,
            ObjectFormat::Elf,
            Endianness::Little,
            pointer_width,
            NonZeroU32::new(4).unwrap_or(NonZeroU32::MIN),
            NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN),
        ) else {
            panic!("test target machine must be valid");
        };

        match TargetProfile::try_new(
            identity,
            machine,
            match bray_target::TargetFacts::try_portable("unknown", "linux", "gnu", "gnu") {
                Some(facts) => facts,
                None => panic!("constant-evaluation test target facts must be valid"),
            },
        ) {
            Ok(profile) => profile,
            Err(error) => panic!("constant-evaluation test target must be valid: {error:?}"),
        }
    }

    #[derive(Clone, Copy)]
    struct LiteralSource {
        origin: BoundNodeOrigin,
        spelling_range: bray_source::TextRange,
    }
}
