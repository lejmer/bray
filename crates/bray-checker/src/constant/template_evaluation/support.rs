use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_diagnostics::DiagnosticConstantOperation;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, ConstantValueId, ConstantValueKind, SemanticValueStore,
    TypeId,
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

    pub(super) const fn invalid_expression(
        category: bray_diagnostics::DiagnosticExpressionCategory,
    ) -> Self {
        Self::Diagnostic(ConstantDiagnostic::InvalidExpression(Some(category)))
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

    super::super::integer::integer_to_usize(value)
}

pub(super) fn template_index(raw: u32) -> Result<usize, TemplateEvaluationFailure> {
    usize::try_from(raw).map_err(|_| TemplateEvaluationFailure::invalid_input())
}
