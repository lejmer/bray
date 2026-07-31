use bray_binder::BinderFactContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{AnySymbolId, NamedTypeSymbolId, TypeId};

use super::super::super::Compilation;
use super::super::super::binder::CompilationBinderFacts;
use super::super::model::OperationResolution;
use super::super::query::construction_operands;
use crate::fact::{CancellationToken, FactQueryError, OperationSelectionFactKey};

impl Compilation {
    pub(in crate::compilation::operation) fn resolve_construction_operation(
        &self,
        key: &OperationSelectionFactKey,
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let expression = unit
            .view()
            .expression(key.expression())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let Some(result_type) =
            self.construction_result_type(facts, unit, types, key.expression())?
        else {
            return Ok(None);
        };

        let candidate = match expression {
            BoundExpression::StructConstruction(_) => {
                self.struct_construction_candidate(facts, result_type, diagnostics)?
            }
            BoundExpression::LeadingDotVariant(variant) => self.leading_dot_variant_candidate(
                facts,
                result_type,
                variant.selector(),
                diagnostics,
            )?,
            BoundExpression::Call(call) => {
                let candidate = self.union_variant_construction_candidate(
                    facts,
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
                    facts,
                    result_type,
                    cancellation,
                    diagnostics,
                )?
            }
            _ => return Err(FactQueryError::InfrastructureFailure),
        };

        let selected = self.select_operation(
            key,
            facts,
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
        facts: &CompilationBinderFacts<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        expression: BoundExpressionId,
    ) -> Result<Option<TypeId>, FactQueryError> {
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
            facts.semantic_values(),
            NamedTypeSymbolId::Struct(structure),
        )
        .map(Some)
    }
}
