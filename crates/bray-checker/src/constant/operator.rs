use bray_bound_tree::BoundOperator;
use bray_symbols::{ConstantBinaryOperation, ConstantUnaryOperation};

pub(crate) const fn unary_term_operation(
    operation: BoundOperator,
) -> Option<ConstantUnaryOperation> {
    match operation {
        BoundOperator::Add => Some(ConstantUnaryOperation::Identity),
        BoundOperator::Subtract => Some(ConstantUnaryOperation::Negate),
        BoundOperator::LogicalNot => Some(ConstantUnaryOperation::LogicalNot),
        BoundOperator::BitwiseNot => Some(ConstantUnaryOperation::BitwiseNot),
        BoundOperator::LogicalOr
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
        | BoundOperator::Multiply
        | BoundOperator::Divide
        | BoundOperator::Remainder
        | BoundOperator::MatrixMultiply
        | BoundOperator::Exponentiate => None,
    }
}

pub(crate) const fn binary_term_operation(
    operation: BoundOperator,
) -> Option<ConstantBinaryOperation> {
    match operation {
        BoundOperator::LogicalOr => Some(ConstantBinaryOperation::LogicalOr),
        BoundOperator::LogicalAnd => Some(ConstantBinaryOperation::LogicalAnd),
        BoundOperator::Equal => Some(ConstantBinaryOperation::Equal),
        BoundOperator::NotEqual => Some(ConstantBinaryOperation::NotEqual),
        BoundOperator::Less => Some(ConstantBinaryOperation::Less),
        BoundOperator::LessEqual => Some(ConstantBinaryOperation::LessOrEqual),
        BoundOperator::Greater => Some(ConstantBinaryOperation::Greater),
        BoundOperator::GreaterEqual => Some(ConstantBinaryOperation::GreaterOrEqual),
        BoundOperator::BitwiseOr => Some(ConstantBinaryOperation::BitwiseOr),
        BoundOperator::BitwiseXor => Some(ConstantBinaryOperation::BitwiseXor),
        BoundOperator::BitwiseAnd => Some(ConstantBinaryOperation::BitwiseAnd),
        BoundOperator::ShiftLeft => Some(ConstantBinaryOperation::ShiftLeft),
        BoundOperator::ShiftRight => Some(ConstantBinaryOperation::ShiftRight),
        BoundOperator::Add => Some(ConstantBinaryOperation::Add),
        BoundOperator::Subtract => Some(ConstantBinaryOperation::Subtract),
        BoundOperator::Multiply => Some(ConstantBinaryOperation::Multiply),
        BoundOperator::Divide => Some(ConstantBinaryOperation::Divide),
        BoundOperator::Remainder => Some(ConstantBinaryOperation::Remainder),
        BoundOperator::Exponentiate => Some(ConstantBinaryOperation::Exponentiate),
        BoundOperator::MatrixMultiply | BoundOperator::BitwiseNot | BoundOperator::LogicalNot => {
            None
        }
    }
}

pub(crate) fn unary_operator(operation: ConstantUnaryOperation) -> bray_bound_tree::BoundOperator {
    match operation {
        ConstantUnaryOperation::Identity => bray_bound_tree::BoundOperator::Add,
        ConstantUnaryOperation::Negate => bray_bound_tree::BoundOperator::Subtract,
        ConstantUnaryOperation::LogicalNot => bray_bound_tree::BoundOperator::LogicalNot,
        ConstantUnaryOperation::BitwiseNot => bray_bound_tree::BoundOperator::BitwiseNot,
    }
}

pub(crate) fn binary_operator(
    operation: ConstantBinaryOperation,
) -> bray_bound_tree::BoundOperator {
    match operation {
        ConstantBinaryOperation::Add => bray_bound_tree::BoundOperator::Add,
        ConstantBinaryOperation::Subtract => bray_bound_tree::BoundOperator::Subtract,
        ConstantBinaryOperation::Multiply => bray_bound_tree::BoundOperator::Multiply,
        ConstantBinaryOperation::Divide => bray_bound_tree::BoundOperator::Divide,
        ConstantBinaryOperation::Remainder => bray_bound_tree::BoundOperator::Remainder,
        ConstantBinaryOperation::Exponentiate => bray_bound_tree::BoundOperator::Exponentiate,
        ConstantBinaryOperation::LogicalAnd => bray_bound_tree::BoundOperator::LogicalAnd,
        ConstantBinaryOperation::LogicalOr => bray_bound_tree::BoundOperator::LogicalOr,
        ConstantBinaryOperation::BitwiseAnd => bray_bound_tree::BoundOperator::BitwiseAnd,
        ConstantBinaryOperation::BitwiseOr => bray_bound_tree::BoundOperator::BitwiseOr,
        ConstantBinaryOperation::BitwiseXor => bray_bound_tree::BoundOperator::BitwiseXor,
        ConstantBinaryOperation::ShiftLeft => bray_bound_tree::BoundOperator::ShiftLeft,
        ConstantBinaryOperation::ShiftRight => bray_bound_tree::BoundOperator::ShiftRight,
        ConstantBinaryOperation::Equal => bray_bound_tree::BoundOperator::Equal,
        ConstantBinaryOperation::NotEqual => bray_bound_tree::BoundOperator::NotEqual,
        ConstantBinaryOperation::Less => bray_bound_tree::BoundOperator::Less,
        ConstantBinaryOperation::LessOrEqual => bray_bound_tree::BoundOperator::LessEqual,
        ConstantBinaryOperation::Greater => bray_bound_tree::BoundOperator::Greater,
        ConstantBinaryOperation::GreaterOrEqual => bray_bound_tree::BoundOperator::GreaterEqual,
    }
}
