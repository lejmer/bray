use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, CheckedExpressionTypes, OperatorTarget,
    SelectedCompoundAssignment, SelectedOperation, SemanticSelection, SemanticSelectionEntry,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{TypeData, TypeId};

use crate::representation::{representation_type, type_representation};
use crate::selection::representation_supports_operator;
use crate::type_check::ExpressionTypeSession;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy)]
pub(super) struct PreparedBuiltInOperator {
    expression: BoundExpressionId,
    operator: BoundOperator,
    kind: PreparedBuiltInOperatorKind,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum PreparedBuiltInOperatorKind {
    Ordinary,
    CompoundAssignment,
}

impl PreparedBuiltInOperator {
    pub(super) fn for_expression(
        expression: BoundExpressionId,
        bound: &BoundExpression,
    ) -> Option<Self> {
        let (operator, kind) = match bound {
            BoundExpression::Unary(expression) if expression.operands().len() == 1 => {
                (expression.operator(), PreparedBuiltInOperatorKind::Ordinary)
            }
            BoundExpression::Binary(expression) if expression.operands().len() == 2 => {
                (expression.operator(), PreparedBuiltInOperatorKind::Ordinary)
            }
            BoundExpression::Assignment(expression) if expression.operands().len() == 2 => (
                expression.operator().binary_operator()?,
                PreparedBuiltInOperatorKind::CompoundAssignment,
            ),
            _ => return None,
        };

        matches!(
            operator,
            BoundOperator::LogicalNot
                | BoundOperator::LogicalAnd
                | BoundOperator::LogicalOr
                | BoundOperator::Equal
                | BoundOperator::NotEqual
                | BoundOperator::Less
                | BoundOperator::LessEqual
                | BoundOperator::Greater
                | BoundOperator::GreaterEqual
                | BoundOperator::Add
                | BoundOperator::Subtract
                | BoundOperator::Multiply
                | BoundOperator::Divide
                | BoundOperator::Remainder
                | BoundOperator::Exponentiate
                | BoundOperator::BitwiseAnd
                | BoundOperator::BitwiseOr
                | BoundOperator::BitwiseXor
                | BoundOperator::ShiftLeft
                | BoundOperator::ShiftRight
                | BoundOperator::BitwiseNot
        )
        .then_some(Self {
            expression,
            operator,
            kind,
        })
    }

    pub(super) const fn expression(self) -> BoundExpressionId {
        self.expression
    }
}

pub(super) fn apply_evidence<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &[PreparedBuiltInOperator],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let boolean = representation_type(request, RepresentationRole::ScalarBool)?;

    for operation in prepared {
        let Some(expression) = request.view().expression(operation.expression) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if operation.kind == PreparedBuiltInOperatorKind::CompoundAssignment {
            if let Some(ty) = numeric_operation_type(
                request,
                operation.expression,
                expression,
                operation.operator,
                session,
            )? {
                for operand in expression.child_expressions() {
                    add_operand_expectation(request, session, operand, ty)?;
                }
            }

            continue;
        }

        if is_boolean_result_operator(operation.operator) {
            session.add_evidence(operation.expression, boolean)?;
        } else if let Some(ty) = numeric_operation_type(
            request,
            operation.expression,
            expression,
            operation.operator,
            session,
        )? {
            session.add_evidence(operation.expression, ty)?;

            for operand in expression.child_expressions() {
                session.add_expectation(operand, ty)?;
            }
        }

        if is_logical_operator(operation.operator) {
            for operand in expression.child_expressions() {
                session.add_expectation(operand, boolean)?;
            }
        }
    }

    Ok(())
}

pub(super) fn apply_operand_expectations<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    prepared: &[PreparedBuiltInOperator],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for operation in prepared {
        let Some(source) = request.view().expression(operation.expression) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let operands = match source {
            BoundExpression::Binary(expression) => expression.operands(),
            BoundExpression::Assignment(expression)
                if operation.kind == PreparedBuiltInOperatorKind::CompoundAssignment =>
            {
                expression.operands()
            }
            _ => continue,
        };

        let [left, right] = operands else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if let Some(ty) = built_in_operand_type(request, types, *left, operation.operator)? {
            add_operand_expectation(request, session, *right, ty)?;

            if operation.kind == PreparedBuiltInOperatorKind::Ordinary {
                session.add_evidence(
                    operation.expression,
                    result_type(request, operation.operator, ty)?,
                )?;
            }
        } else if let Some(ty) = built_in_operand_type(request, types, *right, operation.operator)?
        {
            add_operand_expectation(request, session, *left, ty)?;

            if operation.kind == PreparedBuiltInOperatorKind::Ordinary {
                session.add_evidence(
                    operation.expression,
                    result_type(request, operation.operator, ty)?,
                )?;
            }
        }
    }

    Ok(())
}

pub(super) fn selections<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    prepared: &[PreparedBuiltInOperator],
) -> Result<Vec<SemanticSelectionEntry>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut entries = Vec::with_capacity(prepared.len());

    for operation in prepared {
        let Some(result_type) = selected_result_type(request, types, operation)? else {
            continue;
        };

        let target = OperatorTarget::BuiltIn(operation.operator);

        let selection = if operation.kind == PreparedBuiltInOperatorKind::CompoundAssignment {
            let Some(assignment_type) = types
                .expression(operation.expression)
                .filter(|result| !result.is_recovered())
                .map(bray_bound_tree::ExpressionTypeResult::ty)
            else {
                continue;
            };

            SelectedOperation::CompoundAssignment(SelectedCompoundAssignment::new(
                target,
                result_type,
                assignment_type,
            ))
        } else {
            SelectedOperation::Operator {
                target,
                result_type,
            }
        };

        entries.push(SemanticSelectionEntry::new(
            operation.expression,
            SemanticSelection::Operation(selection),
        ));
    }

    Ok(entries)
}

fn built_in_operand_type<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
    operator: BoundOperator,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(result) = types
        .expression(expression)
        .filter(|result| !result.is_recovered())
    else {
        return Ok(None);
    };

    let ty = observed_type(request, result.ty())?;

    Ok(type_representation(request, ty)?
        .filter(|role| representation_supports_operator(*role, operator))
        .map(|_| ty))
}

fn selected_result_type<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    operation: &PreparedBuiltInOperator,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(operation.expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let operands = expression.child_expressions().collect::<Vec<_>>();

    if operands.is_empty() || operands.len() > 2 {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let Some(first) = types
        .expression(operands[0])
        .filter(|result| !result.is_recovered())
    else {
        return Ok(None);
    };

    if let Some(second) = operands.get(1) {
        let Some(second) = types
            .expression(*second)
            .filter(|result| !result.is_recovered())
        else {
            return Ok(None);
        };

        if observed_type(request, first.ty())? != observed_type(request, second.ty())? {
            return Ok(None);
        }
    }

    let operand = observed_type(request, first.ty())?;

    let Some(role) = type_representation(request, operand)? else {
        return Ok(None);
    };

    if !representation_supports_operator(role, operation.operator) {
        return Ok(None);
    }

    result_type(request, operation.operator, operand).map(Some)
}

fn numeric_operation_type<C>(
    request: CheckerUnitView<'_, C>,
    expression_id: BoundExpressionId,
    expression: &BoundExpression,
    operator: BoundOperator,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(result) = session
        .expression_type(expression_id)
        .filter(|result| !result.is_recovered())
        && type_representation(request, observed_type(request, result.ty())?)?
            .is_some_and(|role| representation_supports_operator(role, operator))
    {
        return observed_type(request, result.ty()).map(Some);
    }

    if let Some(expected) = session.unique_matching_expectation(expression_id, |ty| {
        Ok(type_representation(request, ty)?
            .is_some_and(|role| representation_supports_operator(role, operator)))
    })? {
        return Ok(Some(expected));
    }

    for operand in expression.child_expressions() {
        let Some(result) = session
            .expression_type(operand)
            .filter(|result| !result.is_recovered())
        else {
            continue;
        };

        let ty = observed_type(request, result.ty())?;

        if type_representation(request, ty)?
            .is_some_and(|role| representation_supports_operator(role, operator))
        {
            return Ok(Some(ty));
        }
    }

    Ok(None)
}

fn add_operand_expectation<C>(
    request: CheckerUnitView<'_, C>,
    session: &mut ExpressionTypeSession<'_, C>,
    expression: BoundExpressionId,
    expected: TypeId,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(actual) = session
        .expression_type(expression)
        .filter(|result| !result.is_recovered())
        && observed_type(request, actual.ty())? == expected
    {
        return Ok(());
    }

    session.add_expectation(expression, expected)
}

fn observed_type<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(match data.as_ref() {
        TypeData::Borrow { target, .. } => *target,
        _ => ty,
    })
}

fn result_type<C>(
    request: CheckerUnitView<'_, C>,
    operator: BoundOperator,
    operand: TypeId,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if is_boolean_result_operator(operator) {
        representation_type(request, RepresentationRole::ScalarBool)
    } else {
        Ok(operand)
    }
}

const fn is_boolean_result_operator(operator: BoundOperator) -> bool {
    is_logical_operator(operator) || is_comparison_operator(operator)
}

const fn is_logical_operator(operator: BoundOperator) -> bool {
    matches!(
        operator,
        BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr
    )
}

const fn is_comparison_operator(operator: BoundOperator) -> bool {
    matches!(
        operator,
        BoundOperator::Equal
            | BoundOperator::NotEqual
            | BoundOperator::Less
            | BoundOperator::LessEqual
            | BoundOperator::Greater
            | BoundOperator::GreaterEqual
    )
}
