use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, NamedTypeSymbolId, TypeId};

use super::super::super::Compilation;
use super::super::super::binder::CompilationBindingContext;
use super::super::model::OperationResolution;
use super::super::query::{construction_operands, operation_contract_failure};
use crate::compilation::{SemanticDataKind, SemanticQueryViolation};
use crate::fact::{CancellationToken, FactQueryError, OperationSelectionQueryKey};

impl Compilation {
    pub(in crate::compilation::operation) fn resolve_construction_operation(
        &self,
        key: &OperationSelectionQueryKey,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let expression = unit.view().expression(key.expression()).ok_or_else(|| {
            operation_contract_failure(
                key,
                SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
            )
        })?;

        let Some(result_type) =
            self.construction_result_type(binding_context, unit, types, key.expression())?
        else {
            if let Some(variant) = self.unqualified_variant_reference(unit, key.expression()) {
                diagnostics.add(super::union::unqualified_variant_diagnostic(
                    variant,
                    bray_diagnostics::DiagnosticKind::BindingUnresolvedName,
                ));
            }

            return Ok(None);
        };

        let candidate = match expression {
            BoundExpression::StructConstruction(_) => {
                self.struct_construction_candidate(binding_context, result_type, diagnostics)?
            }
            BoundExpression::LeadingDotVariant(variant) => self.leading_dot_variant_candidate(
                binding_context,
                result_type,
                variant.selector(),
                diagnostics,
            )?,
            BoundExpression::UnqualifiedVariant(variant) => self.unqualified_variant_candidate(
                binding_context,
                result_type,
                variant,
                diagnostics,
            )?,
            BoundExpression::MemberAccess(_) => self.union_variant_construction_candidate(
                binding_context,
                unit,
                key.expression(),
                result_type,
                diagnostics,
            )?,
            BoundExpression::Call(call) => {
                let candidate = self.union_variant_construction_candidate(
                    binding_context,
                    unit,
                    call.callee(),
                    result_type,
                    diagnostics,
                )?;

                let Some(candidate) = candidate else {
                    return Ok(None);
                };

                Some(candidate)
            }
            BoundExpression::Structured(structured)
                if structured.kind() == BoundStructuredExpressionKind::TypeFormConstruction =>
            {
                self.type_form_construction_candidate(
                    key,
                    binding_context,
                    result_type,
                    cancellation,
                    diagnostics,
                )?
            }
            _ => {
                return Err(operation_contract_failure(
                    key,
                    SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
                ));
            }
        };

        if candidate.is_none() && matches!(expression, BoundExpression::UnqualifiedVariant(_)) {
            return Ok(Some(OperationResolution::new(
                key.expression(),
                result_type,
                [],
                None,
            )));
        }

        let selected = self.select_operation(
            key,
            binding_context,
            unit,
            types,
            construction_operands(expression),
            candidate,
            cancellation,
            diagnostics,
        )?;

        Ok(Some(selected.unwrap_or_else(|| {
            OperationResolution::new(key.expression(), result_type, [], None)
        })))
    }

    fn construction_result_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
    ) -> Result<Option<TypeId>, FactQueryError> {
        if let Some(result) =
            self.qualified_union_result_type(binding_context, unit, types, expression)?
        {
            return Ok(Some(result));
        }

        if let Some(result) = types.expression(expression)
            && !result.is_recovered()
        {
            return Ok(Some(result.ty()));
        }

        if unit.tree().expressions().any(|(_, candidate)| {
            matches!(
                candidate,
                BoundExpression::ControlTransfer(transfer)
                    if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Return
                        && transfer.operand() == Some(expression)
            )
        }) {
            return Ok(types.callable_result_type());
        }

        let Some(BoundExpression::StructConstruction(construction)) =
            unit.view().expression(expression)
        else {
            return Ok(None);
        };

        let Some(head) = construction.head() else {
            return Ok(None);
        };

        let Some(BoundExpression::Name(name)) = unit.view().expression(head) else {
            return Ok(None);
        };

        let BoundReferenceTarget::Surface(AnySymbolId::Struct(structure)) = name.target() else {
            return Ok(None);
        };

        super::super::super::substitution::named_type(
            binding_context.semantic_values(),
            NamedTypeSymbolId::Struct(structure),
        )
        .map(Some)
    }

    fn qualified_union_result_type(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
    ) -> Result<Option<TypeId>, FactQueryError> {
        let member = match unit.view().expression(expression) {
            Some(BoundExpression::Call(call)) => {
                let Some(BoundExpression::MemberAccess(member)) =
                    unit.view().expression(call.callee())
                else {
                    return Ok(None);
                };

                member
            }
            Some(BoundExpression::MemberAccess(member)) => member,
            _ => return Ok(None),
        };

        let named_receiver = match unit.view().expression(member.receiver()) {
            Some(BoundExpression::Name(receiver)) => Some(receiver),
            _ => None,
        };

        if let Some(receiver) = named_receiver
            && receiver.generic_argument_list().is_none()
            && let BoundReferenceTarget::Surface(AnySymbolId::Union(union)) = receiver.target()
            && let Some(result) = types.expression(expression)
            && !result.is_recovered()
        {
            let data = binding_context
                .semantic_values()
                .type_data(result.ty())
                .map_err(FactQueryError::SemanticValueStore)?;

            if matches!(
                data.as_ref(),
                bray_symbols::TypeData::Named {
                    definition: NamedTypeSymbolId::Union(definition),
                    ..
                } if *definition == union
            ) {
                return Ok(Some(result.ty()));
            }
        }

        if let Some(result) = types.expression(member.receiver())
            && !result.is_recovered()
        {
            let data = binding_context
                .semantic_values()
                .type_data(result.ty())
                .map_err(FactQueryError::SemanticValueStore)?;

            if matches!(
                data.as_ref(),
                bray_symbols::TypeData::Named {
                    definition: NamedTypeSymbolId::Union(_),
                    ..
                }
            ) {
                return Ok(Some(result.ty()));
            }
        }

        let Some(receiver) = named_receiver else {
            return Ok(None);
        };

        let BoundReferenceTarget::Surface(AnySymbolId::Union(union)) = receiver.target() else {
            return Ok(None);
        };

        super::super::super::substitution::named_type(
            binding_context.semantic_values(),
            NamedTypeSymbolId::Union(union),
        )
        .map(Some)
    }
}
