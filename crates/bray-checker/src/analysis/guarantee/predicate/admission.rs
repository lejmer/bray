use bray_bound_tree::{
    BoundControlTransferKind, BoundExpression, BoundExpressionId, BoundOperator,
    BoundStructuredExpressionKind, BoundUnitKind, SelectedArgument, SelectedOperation,
    SemanticSelection, StorageAccessPurpose, StoragePlan,
};
use bray_symbols::BorrowKind;
use num_bigint::BigInt;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::IntegerRanges;
use crate::{CheckerRequestContext, CheckerUnitView, ExecutionCondition};

pub(in crate::analysis) fn expression_is_eligible<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    yield_targets: &std::collections::BTreeMap<BoundExpressionId, BoundExpressionId>,
) -> bool {
    let selections = semantics.selections();

    if let Some(SemanticSelection::Call(call)) = selections.expression(expression)
        && !matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::Predicate(_)
        )
        && call.constness() != bray_symbols::CallableConstness::Constant
    {
        return false;
    }

    if matches!(
        selections.expression(expression),
        Some(SemanticSelection::Propagation(
            bray_bound_tree::SelectedPropagation::CurrentRun { .. }
        ))
    ) {
        return false;
    }

    let callable_body = matches!(
        request.unit().key().kind(),
        BoundUnitKind::CallableBody
            | BoundUnitKind::AnonymousCallable
            | BoundUnitKind::RuntimeDefault
            | BoundUnitKind::ConstantTemplate
            | BoundUnitKind::EmbeddedConstant
    );

    if storage.expression_plans(expression).any(|plan| {
        matches!(
            plan.purpose(),
            StorageAccessPurpose::Write
                | StorageAccessPurpose::Assignment
                | StorageAccessPurpose::Borrow(BorrowKind::Mutable)
        ) || (!callable_body && plan.purpose() == StorageAccessPurpose::Move)
    }) {
        return false;
    }

    if matches!(
        selections.expression(expression),
        Some(SemanticSelection::ScopedUse(_) | SemanticSelection::StaticReference(_))
    ) {
        return false;
    }

    match request.view().expression(expression).expect("checked operation retains its expression") {
        BoundExpression::Literal(_)
        | BoundExpression::Name(_)
        | BoundExpression::PatternReference(_)
        | BoundExpression::Unary(_)
        | BoundExpression::Binary(_)
        | BoundExpression::Call(_)
        | BoundExpression::Conversion(_)
        | BoundExpression::StructConstruction(_)
        | BoundExpression::MemberAccess(_)
        | BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::TraitQualifiedMember(_) => true,
        BoundExpression::Block(_) | BoundExpression::Match(_) => callable_body,
        BoundExpression::AnonymousCallable(_) => semantics.types().expression(expression).is_some_and(|ty| matches!(request.semantic_values().type_data(ty.ty()).as_ref(),
                bray_symbols::TypeData::Callable(callable) if callable.constness() == bray_symbols::CallableConstness::Constant)),
        BoundExpression::ControlTransfer(transfer) => match transfer.kind() {
            BoundControlTransferKind::Return => callable_body,
            BoundControlTransferKind::Yield => callable_body || yield_targets.get(&expression)
                .is_some_and(|target| matches!(request.view().expression(*target),
                    Some(BoundExpression::Structured(operation)) if operation.kind() == BoundStructuredExpressionKind::Conditional)),
            _ => false,
        },
        BoundExpression::Structured(expression) => match expression.kind() {
            BoundStructuredExpressionKind::Unit
            | BoundStructuredExpressionKind::Absence
            | BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex
            | BoundStructuredExpressionKind::Tuple
            | BoundStructuredExpressionKind::Array
            | BoundStructuredExpressionKind::RepeatedArray
            | BoundStructuredExpressionKind::Range
            | BoundStructuredExpressionKind::Conditional
            | BoundStructuredExpressionKind::PatternTest
            | BoundStructuredExpressionKind::Condition => true,
            BoundStructuredExpressionKind::Borrow => expression.borrow_kind() == Some(BorrowKind::Shared),
            BoundStructuredExpressionKind::TrustBoundary => !callable_body,
            BoundStructuredExpressionKind::PatternBinding => callable_body,
            BoundStructuredExpressionKind::NullablePropagation
            | BoundStructuredExpressionKind::ResultPropagation => callable_body,
            BoundStructuredExpressionKind::ArrayGenerator
            | BoundStructuredExpressionKind::GeneralGenerator
            | BoundStructuredExpressionKind::While
            | BoundStructuredExpressionKind::Loop
            | BoundStructuredExpressionKind::With
            | BoundStructuredExpressionKind::Assertion
            | BoundStructuredExpressionKind::Catch
            | BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold
            | BoundStructuredExpressionKind::Panic => false,
        },
        BoundExpression::UnresolvedReference(_)
        | BoundExpression::Assignment(_)
        | BoundExpression::BoxConstruction(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::Await(_)
        | BoundExpression::For(_)
        | BoundExpression::Generator(_)
        | BoundExpression::Error(_) => false,
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn predicate_operation_is_valid(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        ranges: &mut IntegerRanges,
        maxima: &mut std::collections::BTreeMap<BoundExpressionId, u64>,
    ) -> bool {
        match self.semantics.selections().expression(expression) {
            Some(SemanticSelection::Call(call)) => {
                let behavior = call.phase_behaviors().invocation();

                behavior.effects().is_empty()
                    && behavior.capabilities().is_empty()
                    && behavior.trusted_capabilities().is_empty()
                    && call.arguments().iter().all(|argument| match argument {
                        SelectedArgument::Explicit {
                            expression,
                            conversion,
                            ..
                        } => self.predicate_value_fits(
                            state,
                            *expression,
                            conversion.target_type(),
                            ranges,
                        ),
                        // The selected provider is certified by the same dependency graph as callees.
                        SelectedArgument::Default { .. } => true,
                    })
            }
            Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) => {
                construction.inputs().iter().all(|input| match input {
                    bray_bound_tree::SelectedConstructionInput::Explicit {
                        expression,
                        conversion,
                        ..
                    } => self.predicate_value_fits(
                        state,
                        *expression,
                        conversion.target_type(),
                        ranges,
                    ),
                    bray_bound_tree::SelectedConstructionInput::Default { .. } => true,
                })
            }
            Some(SemanticSelection::Operation(SelectedOperation::Operator {
                target: bray_bound_tree::OperatorTarget::BuiltIn(operator),
                ..
            })) => self.predicate_builtin_is_valid(state, expression, *operator, ranges, maxima),
            Some(SemanticSelection::Operation(SelectedOperation::Conversion(conversion))) => {
                let Some(BoundExpression::Conversion(operation)) =
                    self.request.view().expression(expression)
                else {
                    return true;
                };

                self.predicate_value_fits(
                    state,
                    operation.operand(),
                    conversion.target_type(),
                    ranges,
                )
            }
            Some(SemanticSelection::Operation(SelectedOperation::Index { .. })) => {
                self.index_in_bounds(state, expression)
            }
            _ => true,
        }
    }

    fn predicate_builtin_is_valid(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        operator: BoundOperator,
        ranges: &mut IntegerRanges,
        maxima: &mut std::collections::BTreeMap<BoundExpressionId, u64>,
    ) -> bool {
        let operands = match self
            .request
            .view()
            .expression(expression)
            .expect("checked operator exists")
        {
            BoundExpression::Unary(operation) => operation.operands(),
            BoundExpression::Binary(operation) => operation.operands(),
            _ => unreachable!("built-in operator selection retains its operation"),
        };

        let machine = matches!(
            self.request.unit().key().kind(),
            BoundUnitKind::CallableBody
                | BoundUnitKind::AnonymousCallable
                | BoundUnitKind::RuntimeDefault
        );

        if machine && !self.machine_operation_is_valid(state, operator, operands, ranges) {
            return false;
        }

        let domain = match (operator, operands) {
            (BoundOperator::Divide | BoundOperator::Remainder, [_, denominator]) => {
                self.nonzero_value(state, *denominator, ranges)
            }
            (
                BoundOperator::ShiftLeft | BoundOperator::ShiftRight | BoundOperator::Exponentiate,
                [_, count],
            ) => self
                .integer_range(state, *count, ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                })
                .is_some_and(|(lower, upper)| {
                    if lower < BigInt::from(0)
                        || upper
                            > BigInt::from(
                                crate::ConstantEvaluationLimits::default().integer_bits(),
                            )
                    {
                        return false;
                    }

                    let upper = u64::try_from(upper).expect("bounded nonnegative count fits u64");

                    maxima
                        .entry(*count)
                        .and_modify(|maximum| *maximum = (*maximum).max(upper))
                        .or_insert(upper);

                    true
                }),
            _ => true,
        };

        domain
            && (!machine
                || self.predicate_value_fits(
                    state,
                    expression,
                    self.semantics
                        .types()
                        .expression(expression)
                        .expect("checked operator has a type")
                        .ty(),
                    ranges,
                ))
    }

    fn machine_operation_is_valid(
        &self,
        state: &ExecutionState,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
        ranges: &mut IntegerRanges,
    ) -> bool {
        let [left, right] = operands else {
            return true;
        };

        let left_type = self
            .semantics
            .types()
            .expression(*left)
            .expect("checked operand has a type")
            .ty();

        let Some((minimum, maximum)) = self.integer_type_range(left_type) else {
            return true;
        };

        match operator {
            BoundOperator::ShiftLeft | BoundOperator::ShiftRight => {
                let width = maximum.bits() + u64::from(minimum < BigInt::from(0));

                self.integer_range(state, *right, ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                })
                .is_some_and(|(lower, upper)| {
                    lower >= BigInt::from(0) && upper < BigInt::from(width)
                })
            }
            BoundOperator::Divide | BoundOperator::Remainder if minimum < BigInt::from(0) => {
                let different_left = ExecutionCondition::operation(
                    BoundOperator::NotEqual,
                    vec![
                        self.value(state, *left),
                        self.integer_condition(minimum.clone(), *left),
                    ],
                );

                let different_right = ExecutionCondition::operation(
                    BoundOperator::NotEqual,
                    vec![
                        self.value(state, *right),
                        self.integer_condition(BigInt::from(-1), *right),
                    ],
                );

                let safe = ExecutionCondition::operation(
                    BoundOperator::LogicalOr,
                    vec![different_left, different_right],
                );

                if safe.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                    == Some(true)
                {
                    return true;
                }

                let left_range = self.integer_range(state, *left, ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                });

                let right_range = self.integer_range(state, *right, ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                });

                let includes_minimum =
                    left_range.is_none_or(|(lower, upper)| lower <= minimum && upper >= minimum);

                let includes_negative_one = right_range.is_none_or(|(lower, upper)| {
                    lower <= BigInt::from(-1) && upper >= BigInt::from(-1)
                });

                !(includes_minimum && includes_negative_one)
            }
            _ => true,
        }
    }

    fn nonzero_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        ranges: &mut IntegerRanges,
    ) -> bool {
        let ty = self
            .semantics
            .types()
            .expression(expression)
            .expect("checked denominator type")
            .ty();

        let Some(role) = crate::representation::type_representation(self.request, ty) else {
            return false;
        };

        let integer_zero = bray_symbols::IntegerConstant::from_u64(0);

        let kind = match role.numeric_kind() {
            Some(bray_compiler_known::NumericRepresentationKind::Integer) => {
                bray_symbols::ConstantValueKind::Integer(integer_zero)
            }
            Some(bray_compiler_known::NumericRepresentationKind::Real) => {
                bray_symbols::ConstantValueKind::Real(
                    crate::constant::floating::integer_to_real(&integer_zero, role)
                        .expect("zero is representable in every real format"),
                )
            }
            Some(bray_compiler_known::NumericRepresentationKind::Complex) => {
                let component = role
                    .complex_component()
                    .expect("complex representation has a component type");

                let zero = crate::constant::floating::integer_to_real(&integer_zero, component)
                    .expect("complex zero is representable");

                bray_symbols::ConstantValueKind::Complex {
                    real: zero,
                    imaginary: zero,
                }
            }
            None => return false,
        };

        let values = self.request.semantic_values();

        let zero = values
            .intern_constant_value(bray_symbols::ConstantValueData::new(ty, kind))
            .expect("typed numeric zero belongs to this store");

        let zero = ExecutionCondition::Literal(values.constant_value_data(zero));
        let value = self.value(state, expression);

        ExecutionCondition::operation(BoundOperator::NotEqual, vec![value, zero])
            .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
            == Some(true)
            || self
                .integer_range(state, expression, ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                })
                .is_some_and(|(lower, upper)| lower > BigInt::from(0) || upper < BigInt::from(0))
    }
}
