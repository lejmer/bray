use bray_binder::BindingQueryContext;
use bray_bound_tree::BoundExpression;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::TypeId;

use super::super::Compilation;
use super::super::binder::CompilationBindingContext;
use super::super::unit::semantic_unit_context_for;
use crate::compilation::operation::OperationSubject;
use crate::compilation::{SemanticDataKind, SemanticQueryViolation};
use crate::fact::{CancellationToken, FactQueryError};

use super::model::{ConversionPlan, OperationResolution, TraitOperation};
use super::query::{expression_type, operation_contract_failure, unit_contract_failure};

impl Compilation {
    pub(super) fn resolve_conversion_operation(
        &self,
        key: &OperationSubject,
        binding_context: &CompilationBindingContext<'_>,
        unit: &bray_bound_tree::BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<OperationResolution>, FactQueryError> {
        let Some(BoundExpression::Conversion(conversion)) =
            unit.view().expression(key.expression())
        else {
            return Err(operation_contract_failure(
                key,
                SemanticQueryViolation::Unsupported(SemanticDataKind::OperationSelection),
            ));
        };

        let source_type = expression_type(types, conversion.operand())?;
        let target_type = expression_type(types, key.expression())?;

        let context = self.checker_context_for(key.unit(), cancellation)?;
        let semantic_context = semantic_unit_context_for(binding_context.symbols(), unit)?;

        let request =
            crate::compilation::unit::checker_unit_view(unit, &semantic_context, &context)?;

        let candidate = self
            .resolve_conversion_plan(
                key,
                request,
                binding_context,
                binding_context
                    .symbols()
                    .symbol_for_key(unit.key().declared_owner())
                    .ok_or_else(|| {
                        unit_contract_failure(
                            unit.key(),
                            SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                        )
                    })?,
                source_type,
                target_type,
                cancellation,
                diagnostics,
            )?
            .map(|plan| plan.into_candidate(source_type));

        self.select_operation(
            key,
            binding_context,
            unit,
            types,
            [conversion.operand()],
            candidate,
            cancellation,
            diagnostics,
        )
    }

    fn resolve_conversion_plan(
        &self,
        key: &OperationSubject,
        request: bray_checker::CheckerUnitView<
            '_,
            super::super::checker::CompilationCheckerContext<'_>,
        >,
        binding_context: &CompilationBindingContext<'_>,
        owner: bray_symbols::AnySymbolId,
        source: TypeId,
        target: TypeId,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<ConversionPlan>, FactQueryError> {
        if let Some(conversion) = bray_checker::built_in_conversion_plan(request, source, target)
            .map_err(FactQueryError::from)?
        {
            return Ok(Some(ConversionPlan::built_in(conversion)));
        }

        if let Some(children) = bray_checker::composite_conversion_children(request, source, target)
            .map_err(FactQueryError::from)?
        {
            let mut plans = Vec::with_capacity(children.len());

            for (source, target) in children {
                let Some(plan) = self.resolve_conversion_plan(
                    key,
                    request,
                    binding_context,
                    owner,
                    source,
                    target,
                    cancellation,
                    diagnostics,
                )?
                else {
                    return Ok(None);
                };

                plans.push(plan);
            }

            return Ok(Some(ConversionPlan::composite(source, target, plans)));
        }

        let Some(candidate) = self.trait_operation_candidate_data(
            binding_context,
            owner,
            bray_compiler_known::CompilerKnownOperationRole::PlainConversion,
            source,
            &[target],
            &[],
            &[],
            TraitOperation::Conversion(target),
            cancellation,
            diagnostics,
        )?
        else {
            return Ok(None);
        };

        ConversionPlan::trait_backed(candidate, key).map(Some)
    }
}
