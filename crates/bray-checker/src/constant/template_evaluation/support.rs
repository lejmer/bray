use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_diagnostics::DiagnosticConstantOperation;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, ConstantBinaryOperation, ConstantUnaryOperation,
    ConstantValueId, ConstantValueKind, SemanticValueStore, TypeId,
};

use super::super::diagnostic::ConstantDiagnostic;
use super::super::operation::ConstantOperationError;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

pub(super) enum TemplateEvaluationFailure {
    Cancelled,
    Infrastructure(CheckerInfrastructureError),
    Upstream,
    Diagnostic(ConstantDiagnostic),
}

impl TemplateEvaluationFailure {
    pub(super) const fn invalid_input() -> Self {
        Self::Infrastructure(CheckerInfrastructureError::InvalidConstantEvaluationInput)
    }

    pub(super) const fn semantic_value(error: bray_symbols::SemanticValueStoreError) -> Self {
        Self::Infrastructure(CheckerInfrastructureError::SemanticValueStore(error))
    }
}

pub(super) fn operation_failure(
    operation: DiagnosticConstantOperation,
    error: ConstantOperationError,
) -> TemplateEvaluationFailure {
    TemplateEvaluationFailure::Diagnostic(ConstantDiagnostic::operation(operation, error))
}

pub(super) fn constant_definition(symbol: AnySymbolId) -> Option<AnyConstantDefinitionId> {
    match symbol {
        AnySymbolId::Constant(definition) => Some(definition.into()),
        AnySymbolId::TraitConstantMember(definition) => Some(definition.into()),
        AnySymbolId::TraitConstantFulfillment(definition) => Some(definition.into()),
        _ => None,
    }
}

pub(super) fn unary_operator(operation: ConstantUnaryOperation) -> bray_bound_tree::BoundOperator {
    match operation {
        ConstantUnaryOperation::Identity => bray_bound_tree::BoundOperator::Add,
        ConstantUnaryOperation::Negate => bray_bound_tree::BoundOperator::Subtract,
        ConstantUnaryOperation::LogicalNot => bray_bound_tree::BoundOperator::LogicalNot,
        ConstantUnaryOperation::BitwiseNot => bray_bound_tree::BoundOperator::BitwiseNot,
    }
}

pub(super) fn binary_operator(
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

pub(super) fn target_integer_width(
    context: &(impl CheckerRequestContext + ?Sized),
    representation: Option<RepresentationRole>,
) -> std::num::NonZeroU16 {
    if representation.is_some_and(|role| {
        matches!(
            role.integer_representation(),
            Some(IntegerRepresentation::TargetSigned | IntegerRepresentation::TargetUnsigned)
        )
    }) {
        return context.selected_target().machine().pointer_width_bits();
    }

    std::num::NonZeroU16::MIN
}

pub(super) fn recovery_value(
    values: &SemanticValueStore,
    ty: TypeId,
) -> Result<ConstantValueId, CheckerInfrastructureError> {
    values
        .intern_error_constant_value(ty)
        .map_err(CheckerInfrastructureError::SemanticValueStore)
}

pub(super) fn integer_index(value: &ConstantValueKind) -> Option<usize> {
    let ConstantValueKind::Integer(value) = value else {
        return None;
    };

    value.to_u64().and_then(|value| usize::try_from(value).ok())
}

pub(super) fn template_index(raw: u32) -> Result<usize, TemplateEvaluationFailure> {
    usize::try_from(raw).map_err(|_| TemplateEvaluationFailure::invalid_input())
}
