use bray_bound_tree::{BoundExpression, BoundOperator};
use bray_compiler_known::CompilerKnownOperationRole;

/// Returns the compiler-known contract role for one overloadable source operator.
pub const fn compiler_known_operation_role(
    expression: &BoundExpression,
    operator: BoundOperator,
) -> Option<CompilerKnownOperationRole> {
    match expression {
        BoundExpression::Unary(_) => unary_operation_role(operator),
        BoundExpression::Binary(_) => binary_operation_role(operator),
        _ => None,
    }
}

const fn unary_operation_role(operator: BoundOperator) -> Option<CompilerKnownOperationRole> {
    match operator {
        BoundOperator::Subtract => Some(CompilerKnownOperationRole::UnaryNegate),
        BoundOperator::BitwiseNot => Some(CompilerKnownOperationRole::UnaryBitNot),
        BoundOperator::Assign
        | BoundOperator::LogicalOr
        | BoundOperator::LogicalAnd
        | BoundOperator::Equal
        | BoundOperator::NotEqual
        | BoundOperator::Less
        | BoundOperator::LessEqual
        | BoundOperator::Greater
        | BoundOperator::GreaterEqual
        | BoundOperator::BitwiseOr
        | BoundOperator::BitwiseXor
        | BoundOperator::BitwiseAnd
        | BoundOperator::ShiftLeft
        | BoundOperator::ShiftRight
        | BoundOperator::Add
        | BoundOperator::Multiply
        | BoundOperator::Divide
        | BoundOperator::Remainder
        | BoundOperator::MatrixMultiply
        | BoundOperator::Exponentiate
        | BoundOperator::LogicalNot => None,
    }
}

const fn binary_operation_role(operator: BoundOperator) -> Option<CompilerKnownOperationRole> {
    match operator {
        BoundOperator::Equal | BoundOperator::NotEqual => {
            Some(CompilerKnownOperationRole::Equality)
        }
        BoundOperator::Less
        | BoundOperator::LessEqual
        | BoundOperator::Greater
        | BoundOperator::GreaterEqual => Some(CompilerKnownOperationRole::Comparison),
        BoundOperator::BitwiseOr => Some(CompilerKnownOperationRole::BinaryBitOr),
        BoundOperator::BitwiseXor => Some(CompilerKnownOperationRole::BinaryBitXor),
        BoundOperator::BitwiseAnd => Some(CompilerKnownOperationRole::BinaryBitAnd),
        BoundOperator::ShiftLeft => Some(CompilerKnownOperationRole::BinaryShiftLeft),
        BoundOperator::ShiftRight => Some(CompilerKnownOperationRole::BinaryShiftRight),
        BoundOperator::Add => Some(CompilerKnownOperationRole::BinaryAdd),
        BoundOperator::Subtract => Some(CompilerKnownOperationRole::BinarySubtract),
        BoundOperator::Multiply => Some(CompilerKnownOperationRole::BinaryMultiply),
        BoundOperator::Divide => Some(CompilerKnownOperationRole::BinaryDivide),
        BoundOperator::Remainder => Some(CompilerKnownOperationRole::BinaryRemainder),
        BoundOperator::MatrixMultiply => Some(CompilerKnownOperationRole::BinaryMatrixMultiply),
        BoundOperator::Exponentiate => Some(CompilerKnownOperationRole::BinaryExponentiate),
        BoundOperator::Assign
        | BoundOperator::LogicalOr
        | BoundOperator::LogicalAnd
        | BoundOperator::BitwiseNot
        | BoundOperator::LogicalNot => None,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundOperator;
    use bray_compiler_known::CompilerKnownOperationRole;

    use super::{binary_operation_role, unary_operation_role};

    #[test]
    fn overloadable_operators_map_to_closed_compiler_known_roles() {
        assert_eq!(
            unary_operation_role(BoundOperator::Subtract),
            Some(CompilerKnownOperationRole::UnaryNegate)
        );

        assert_eq!(
            unary_operation_role(BoundOperator::BitwiseNot),
            Some(CompilerKnownOperationRole::UnaryBitNot)
        );

        assert_eq!(unary_operation_role(BoundOperator::Add), None);
        assert_eq!(unary_operation_role(BoundOperator::LogicalNot), None);

        for (operator, role) in [
            (BoundOperator::Add, CompilerKnownOperationRole::BinaryAdd),
            (
                BoundOperator::Subtract,
                CompilerKnownOperationRole::BinarySubtract,
            ),
            (
                BoundOperator::Multiply,
                CompilerKnownOperationRole::BinaryMultiply,
            ),
            (
                BoundOperator::Divide,
                CompilerKnownOperationRole::BinaryDivide,
            ),
            (
                BoundOperator::Remainder,
                CompilerKnownOperationRole::BinaryRemainder,
            ),
            (
                BoundOperator::Exponentiate,
                CompilerKnownOperationRole::BinaryExponentiate,
            ),
            (
                BoundOperator::MatrixMultiply,
                CompilerKnownOperationRole::BinaryMatrixMultiply,
            ),
            (
                BoundOperator::BitwiseAnd,
                CompilerKnownOperationRole::BinaryBitAnd,
            ),
            (
                BoundOperator::BitwiseOr,
                CompilerKnownOperationRole::BinaryBitOr,
            ),
            (
                BoundOperator::BitwiseXor,
                CompilerKnownOperationRole::BinaryBitXor,
            ),
            (
                BoundOperator::ShiftLeft,
                CompilerKnownOperationRole::BinaryShiftLeft,
            ),
            (
                BoundOperator::ShiftRight,
                CompilerKnownOperationRole::BinaryShiftRight,
            ),
            (BoundOperator::Equal, CompilerKnownOperationRole::Equality),
            (
                BoundOperator::NotEqual,
                CompilerKnownOperationRole::Equality,
            ),
            (BoundOperator::Less, CompilerKnownOperationRole::Comparison),
            (
                BoundOperator::LessEqual,
                CompilerKnownOperationRole::Comparison,
            ),
            (
                BoundOperator::Greater,
                CompilerKnownOperationRole::Comparison,
            ),
            (
                BoundOperator::GreaterEqual,
                CompilerKnownOperationRole::Comparison,
            ),
        ] {
            assert_eq!(binary_operation_role(operator), Some(role));
        }

        assert_eq!(binary_operation_role(BoundOperator::Assign), None);
        assert_eq!(binary_operation_role(BoundOperator::LogicalAnd), None);
        assert_eq!(binary_operation_role(BoundOperator::LogicalOr), None);
    }
}
