use bray_bound_tree::CheckedTemplateKind;
use bray_compiler_known::{IntegerRepresentation, RepresentationRole};
use bray_diagnostics::DiagnosticConstantOperation;
use bray_symbols::{
    AnyConstantDefinitionId, AnySymbolId, ConstantValueId, ConstantValueKind, SemanticValueStore,
    TypeId,
};

use super::super::diagnostic::ConstantDiagnostic;
use super::super::operation::ConstantOperationError;
use super::evaluator::TemplateEvaluator;
use crate::CheckerRequestContext;

pub(super) fn check_definition_materialization<C: CheckerRequestContext + ?Sized>(
    evaluator: &mut TemplateEvaluator<'_, C>,
    value: ConstantValueId,
) -> Result<(), TemplateEvaluationFailure> {
    if evaluator.template.kind() != CheckedTemplateKind::ConstantDefinition {
        return Ok(());
    }

    let result = crate::constant::materialization::nonmaterializable_value_tree(
        evaluator.context,
        value,
        &mut evaluator.checked_materialization,
        &mut evaluator.diagnostics,
    );

    if let Some(ty) = result.map_err(|error| evaluator.record_query_failure(error))? {
        return Err(TemplateEvaluationFailure::Diagnostic(
            ConstantDiagnostic::NonMaterializable(ty),
        ));
    }

    Ok(())
}

pub(super) enum TemplateEvaluationFailure {
    Cancelled,
    Upstream,
    Diagnostic(ConstantDiagnostic),
}

impl TemplateEvaluationFailure {
    pub(super) const fn invalid_expression(
        category: bray_diagnostics::DiagnosticExpressionCategory,
    ) -> Self {
        Self::Diagnostic(ConstantDiagnostic::InvalidExpression(Some(category)))
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

pub(super) fn recovery_value(values: &SemanticValueStore, ty: TypeId) -> ConstantValueId {
    values
        .intern_error_constant_value(ty)
        .unwrap_or_else(|error| {
            panic!("recovery_value must satisfy its checked construction contract: {error:?}")
        })
}

pub(super) fn integer_index(value: &ConstantValueKind) -> Option<usize> {
    let ConstantValueKind::Integer(value) = value else {
        return None;
    };

    super::super::integer::integer_to_usize(value)
}

pub(super) fn template_index(raw: u32) -> usize {
    usize::try_from(raw).unwrap_or_else(|error| {
        panic!("template_index must satisfy its checked construction contract: {error:?}")
    })
}
