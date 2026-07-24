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

        session.add_evidence(operation.expression, boolean)?;

        if is_logical_operator(operation.operator) {
            for operand in expression.child_expressions() {
                session.add_expectation(operand, boolean)?;
            }
        }
    }

    Ok(())
}

pub(super) fn apply_comparison_expectations<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    prepared: &[PreparedBuiltInOperator],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for operation in prepared
        .iter()
        .filter(|operation| is_comparison_operator(operation.operator))
    {
        let Some(BoundExpression::Binary(expression)) =
            request.view().expression(operation.expression)
        else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let [left, right] = expression.operands() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if let Some(ty) = comparable_operand_type(request, types, *left, operation.operator)? {
            session.add_expectation(*right, ty)?;
        } else if let Some(ty) =
            comparable_operand_type(request, types, *right, operation.operator)?
        {
            session.add_expectation(*left, ty)?;
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
    let boolean = representation_type(request, RepresentationRole::ScalarBool)?;
    let mut entries = Vec::with_capacity(prepared.len());

    for operation in prepared {
        if is_comparison_operator(operation.operator)
            && !comparison_is_selected(request, types, operation)?
        {
            continue;
        }

        entries.push(SemanticSelectionEntry::new(
            operation.expression,
            SemanticSelection::Operation(SelectedOperation::Operator {
                target: OperatorTarget::BuiltIn(operation.operator),
                result_type: boolean,
            }),
        ));
    }

    Ok(entries)
}

fn comparable_operand_type<C>(
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
        .filter(|role| representation_supports_comparison(*role, operator))
        .map(|_| result.ty()))
}

fn comparison_is_selected<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    operation: &PreparedBuiltInOperator,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Binary(expression)) = request.view().expression(operation.expression)
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let [left, right] = expression.operands() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(left) = types
        .expression(*left)
        .filter(|result| !result.is_recovered())
    else {
        return Ok(false);
    };

    let Some(right) = types
        .expression(*right)
        .filter(|result| !result.is_recovered())
    else {
        return Ok(false);
    };

    if left.ty() != right.ty() {
        return Ok(false);
    }

    Ok(type_representation(request, left.ty())?
        .is_some_and(|role| representation_supports_comparison(role, operation.operator)))
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
