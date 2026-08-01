use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, CheckedExpressionTypes, OperatorTarget,
    SelectedOperation, SemanticSelection, SemanticSelectionEntry,
};
use bray_compiler_known::{NumericRepresentationKind, RepresentationRole};
use bray_symbols::TypeId;

use crate::representation::{representation_type, type_representation};
use crate::type_check::ExpressionTypeSession;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy)]
pub(super) struct PreparedBuiltInOperator {
    expression: BoundExpressionId,
    operator: BoundOperator,
}

impl PreparedBuiltInOperator {
    pub(super) fn for_expression(
        expression: BoundExpressionId,
        bound: &BoundExpression,
    ) -> Option<Self> {
        let operator = match bound {
            BoundExpression::Unary(expression) if expression.operands().len() == 1 => {
                expression.operator()
            }
            BoundExpression::Binary(expression) if expression.operands().len() == 2 => {
                expression.operator()
            }
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
                | BoundOperator::BitwiseNot
        )
        .then_some(Self {
            expression,
            operator,
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
        let Some(BoundExpression::Binary(expression)) =
            request.view().expression(operation.expression)
        else {
            continue;
        };

        let [left, right] = expression.operands() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if let Some(ty) = built_in_operand_type(request, types, *left, operation.operator)? {
            session.add_expectation(*right, ty)?;

            session.add_evidence(
                operation.expression,
                result_type(request, operation.operator, ty)?,
            )?;
        } else if let Some(ty) = built_in_operand_type(request, types, *right, operation.operator)?
        {
            session.add_expectation(*left, ty)?;

            session.add_evidence(
                operation.expression,
                result_type(request, operation.operator, ty)?,
            )?;
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

        entries.push(SemanticSelectionEntry::new(
            operation.expression,
            SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(operation.operator),
                result_type,
            }),
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

    Ok(type_representation(request, result.ty())?
        .filter(|role| representation_supports_operator(*role, operator))
        .map(|_| result.ty()))
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

        if first.ty() != second.ty() {
            return Ok(None);
        }
    }

    let Some(role) = type_representation(request, first.ty())? else {
        return Ok(None);
    };

    if !representation_supports_operator(role, operation.operator) {
        return Ok(None);
    }

    result_type(request, operation.operator, first.ty()).map(Some)
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
        && type_representation(request, result.ty())?
            .is_some_and(|role| representation_supports_operator(role, operator))
    {
        return Ok(Some(result.ty()));
    }

    for operand in expression.child_expressions() {
        let Some(result) = session
            .expression_type(operand)
            .filter(|result| !result.is_recovered())
        else {
            continue;
        };

        if type_representation(request, result.ty())?
            .is_some_and(|role| representation_supports_operator(role, operator))
        {
            return Ok(Some(result.ty()));
        }
    }

    Ok(None)
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

const fn representation_supports_comparison(
    role: RepresentationRole,
    operator: BoundOperator,
) -> bool {
    match operator {
        BoundOperator::Equal | BoundOperator::NotEqual => {
            role.numeric_kind().is_some()
                || matches!(
                    role,
                    RepresentationRole::ScalarBool
                        | RepresentationRole::ScalarChar
                        | RepresentationRole::String
                )
        }
        BoundOperator::Less
        | BoundOperator::LessEqual
        | BoundOperator::Greater
        | BoundOperator::GreaterEqual => {
            matches!(
                role.numeric_kind(),
                Some(NumericRepresentationKind::Integer | NumericRepresentationKind::Real)
            ) || matches!(
                role,
                RepresentationRole::ScalarChar | RepresentationRole::String
            )
        }
        _ => false,
    }
}

const fn representation_supports_operator(
    role: RepresentationRole,
    operator: BoundOperator,
) -> bool {
    match operator {
        BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr => {
            matches!(role, RepresentationRole::ScalarBool)
        }
        BoundOperator::Add | BoundOperator::Subtract => role.numeric_kind().is_some(),
        BoundOperator::BitwiseNot => {
            matches!(
                role.numeric_kind(),
                Some(NumericRepresentationKind::Integer)
            )
        }
        _ => representation_supports_comparison(role, operator),
    }
}
