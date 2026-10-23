use bray_bound_tree::{
    BoundControlTransferKind, BoundExpression, BoundExpressionId, BoundOperator,
    BoundStructuredExpressionKind, BoundUnitKind, SelectedOperation, SemanticSelection,
    StorageAccessPurpose, StoragePlan,
};
use bray_symbols::{BorrowKind, TypeId};
use num_bigint::BigInt;

use super::flow::ExecutionFlowDomain;
use super::integer::IntegerRanges;
use super::state::ExecutionState;
use crate::{CheckerRequestContext, CheckerUnitView, ExecutionCondition};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(in crate::analysis) enum ExpressionEligibility {
    Forbidden,
    Observation,
    Constant,
}

pub(in crate::analysis) fn expression_eligibility<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    flow: &bray_bound_tree::StorageFlow,
    yield_targets: &std::collections::BTreeMap<BoundExpressionId, BoundExpressionId>,
) -> ExpressionEligibility {
    if !expression_is_observational(request, expression, semantics, storage, flow, yield_targets) {
        return ExpressionEligibility::Forbidden;
    }

    if let Some(SemanticSelection::Call(call)) = semantics.selections().expression(expression)
        && !matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::Predicate(_)
        )
        && call.constness() != bray_symbols::CallableConstness::Constant
    {
        return ExpressionEligibility::Observation;
    }

    ExpressionEligibility::Constant
}

fn expression_is_observational<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    semantics: &bray_bound_tree::CheckedExpressionSemantics,
    storage: &StoragePlan,
    flow: &bray_bound_tree::StorageFlow,
    yield_targets: &std::collections::BTreeMap<BoundExpressionId, BoundExpressionId>,
) -> bool {
    let selections = semantics.selections();

    if request.unit().key().is_static_initializer()
        && storage
            .node_plans(expression.into())
            .any(|plan| !static_storage_is_address_only(request, storage, flow, plan))
    {
        return false;
    }

    if let Some(SemanticSelection::Call(call)) = selections.expression(expression)
        && matches!(
            call.implementation_hook(),
            Some(
                bray_compiler_known::ImplementationHook::AddressOf
                    | bray_compiler_known::ImplementationHook::AddressOfMut
            )
        )
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

    let owns_locals = callable_body || request.unit().key().is_static_initializer();

    let confined_assignment = flow.assignment_target(expression).is_some_and(|target| {
        target.mutation_scope(storage, request.semantic_values())
            != bray_bound_tree::StorageMutationScope::Unbounded
    });

    if storage
        .node_plans(expression.into())
        .any(|plan| match plan.purpose() {
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment => !confined_assignment,
            StorageAccessPurpose::Borrow(BorrowKind::Mutable) => false,
            StorageAccessPurpose::Move => !owns_locals,
            _ => false,
        })
    {
        return false;
    }

    if matches!(
        selections.expression(expression),
        Some(SemanticSelection::ScopedUse(_))
    ) {
        return false;
    }

    if matches!(
        selections.expression(expression),
        Some(SemanticSelection::StaticReference(_))
    ) && !request.unit().key().is_static_initializer()
    {
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
        BoundExpression::Block(_) | BoundExpression::Match(_) => owns_locals,
        BoundExpression::Assignment(_) => confined_assignment,
        BoundExpression::AnonymousCallable(_) => semantics.types().expression(expression).is_some_and(|ty| matches!(request.semantic_values().type_data(ty.ty()).as_ref(),
                bray_symbols::TypeData::Callable(callable) if callable.constness() == bray_symbols::CallableConstness::Constant)),
        BoundExpression::ControlTransfer(transfer) => match transfer.kind() {
            BoundControlTransferKind::Return => owns_locals,
            BoundControlTransferKind::Yield => owns_locals || yield_targets.get(&expression)
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
            BoundStructuredExpressionKind::PatternBinding => owns_locals,
            BoundStructuredExpressionKind::NullablePropagation
            | BoundStructuredExpressionKind::ResultPropagation => owns_locals,
            BoundStructuredExpressionKind::ArrayGenerator
            | BoundStructuredExpressionKind::GeneralGenerator
            | BoundStructuredExpressionKind::While
            | BoundStructuredExpressionKind::Loop => owns_locals,
            BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold => true,
            BoundStructuredExpressionKind::With
            | BoundStructuredExpressionKind::Assertion
            | BoundStructuredExpressionKind::Catch
            | BoundStructuredExpressionKind::Panic => false,
        },
        BoundExpression::UnresolvedReference(_)
        | BoundExpression::BoxConstruction(_)
        | BoundExpression::ErrorCall(_)
        | BoundExpression::ErrorConversion(_)
        | BoundExpression::Await(_)
        | BoundExpression::Error(_) => false,
        BoundExpression::For(_) | BoundExpression::Generator(_) => owns_locals,
    }
}

fn static_storage_is_address_only<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &bray_bound_tree::StorageFlow,
    plan: bray_bound_tree::StorageAccessPlan,
) -> bool {
    let root = storage
        .root_identity(plan.access())
        .expect("checked access retains its root");

    let direct = matches!(
        storage.identity(root),
        Some(bray_bound_tree::StorageIdentity::Static(_))
    );

    let referred = flow
        .access_target(plan)
        .and_then(bray_bound_tree::StorageTarget::storage_path)
        .is_some_and(|path| {
            matches!(
                storage.identity(path.root),
                Some(bray_bound_tree::StorageIdentity::Static(_))
            )
        });

    if !direct && !referred {
        return true;
    }

    let projections = storage
        .resolved_projections(plan.access())
        .expect("checked static access retains its projections");

    if !projections.is_empty() {
        return false;
    }

    // A formed borrow descriptor and a local reference value do not read the
    // referent. Reading a reference stored in the static still observes that storage.
    if storage.access(plan.access()).is_some_and(|access| {
        matches!(
        request.semantic_values().type_data(access.reached_type()).as_ref(),
        bray_symbols::TypeData::Borrow { kind: BorrowKind::Shared, target }
            if !direct || storage.storage_type(root) == Some(*target))
    }) {
        return true;
    }

    matches!(
        plan.purpose(),
        StorageAccessPurpose::Borrow(BorrowKind::Shared) | StorageAccessPurpose::Projection
    )
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn predicate_value_fits(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        target: TypeId,
        ranges: &mut IntegerRanges,
    ) -> bool {
        let Some((minimum, maximum)) = self.integer_type_range(target) else {
            return true;
        };

        self.integer_range(state, expression, ranges, &mut {
            ExecutionCondition::WORK_LIMIT
        })
        .is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum)
    }

    fn predicate_input_fits(
        &self,
        state: &ExecutionState,
        input: bray_bound_tree::BoundExecutionSite,
        target: TypeId,
        ranges: &mut IntegerRanges,
    ) -> bool {
        let Some((minimum, maximum)) = self.integer_type_range(target) else {
            return true;
        };

        let value = self.current_input(state, input);

        let range = self
            .observed_integer_range(state, &value, ranges, &mut {
                ExecutionCondition::WORK_LIMIT
            })
            .or_else(|| {
                // An opaque completed provider has its declared result type. Its
                // normal completion is certified independently at the input site.
                matches!(
                    value,
                    ExecutionCondition::Call(_, _) | ExecutionCondition::PostState(_, _)
                )
                .then(|| self.integer_type_range(target))
                .flatten()
            });

        range.is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum)
    }

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
                    && self.predicate_invocation_inputs_fit(state, expression, ranges)
            }
            Some(SemanticSelection::Operation(SelectedOperation::Construction(_))) => {
                self.predicate_invocation_inputs_fit(state, expression, ranges)
            }
            Some(SemanticSelection::Operation(SelectedOperation::Operator {
                target: bray_bound_tree::OperatorTarget::BuiltIn(operator),
                ..
            })) => self.predicate_builtin_is_valid(state, expression, *operator, ranges, maxima),
            Some(SemanticSelection::Operation(SelectedOperation::CompoundAssignment(selected))) => {
                let bray_bound_tree::OperatorTarget::BuiltIn(operator) = selected.target() else {
                    return false;
                };

                self.predicate_builtin_is_valid(state, expression, operator, ranges, maxima)
            }
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

    fn predicate_invocation_inputs_fit(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        ranges: &mut IntegerRanges,
    ) -> bool {
        // Ordinary bodies materialize machine values of the checked input type.
        // Only exact predicate arithmetic can exceed that representation before a call.
        if self.request.unit().key().kind().uses_machine_arithmetic() {
            return true;
        }

        let selection = self
            .semantics
            .selections()
            .expression(expression)
            .expect("checked invocation retains its input selection");

        selection.inputs().all(|input| {
            let ty = input.ty(self.request.semantic_values());

            if selection.evaluates_defaults() {
                let site = selection
                    .input_site(expression, input.root())
                    .expect("evaluated invocation retains every input slot");

                self.predicate_input_fits(state, site, ty, ranges)
            } else {
                input.expression().is_some_and(|expression| {
                    self.predicate_value_fits(state, expression, ty, ranges)
                })
            }
        })
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
            BoundExpression::Assignment(operation) => operation.operands(),
            _ => unreachable!("built-in operator selection retains its operation"),
        };

        let machine = self.request.unit().key().kind().uses_machine_arithmetic();

        if machine && !self.machine_operation_is_valid(state, operator, operands, ranges) {
            return false;
        }

        let domain = match (operator, operands) {
            (
                BoundOperator::ShiftLeft | BoundOperator::ShiftRight | BoundOperator::Exponentiate,
                _,
            ) if machine => true,
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
            BoundOperator::Exponentiate => self
                .integer_range(state, *right, ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                })
                .is_some_and(|(lower, _)| lower >= BigInt::from(0)),
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
