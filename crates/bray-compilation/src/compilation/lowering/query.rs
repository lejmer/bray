use std::sync::Arc;

use bray_bound_tree::{
    BoundExpression, BoundReferenceTarget, BoundUnit, BoundUnitKey, CheckedExpressionTypes,
};
use bray_checker::ConstantReferenceResolution;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_ir::{MirOperand, MirTargetContract};
use bray_lowering::{
    CompileTimeUnit, LoweredUnit, LoweringError, LoweringInput, executable_unit_kind, lower_unit,
};
use bray_symbols::{AnySymbolId, ConstantTermData, StaticInstanceTemplateId};

use super::super::Compilation;
use crate::fact::{
    CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult, QueryPriority,
};

type LoweredUnitComputation = (
    DiagnosticResult<Option<LoweredUnit>>,
    Box<[bray_binder::BinderDependency]>,
);

impl Compilation {
    /// Returns the lowering result and dependency diagnostics for one checked semantic unit.
    pub fn lowered_unit(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<Option<LoweredUnit>>>, FactQueryError> {
        if let Some(result) = self.published_output_lowering(&key, &self.state.cancellation)? {
            return Ok(result);
        }

        let published = self.lowered_unit_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    /// Returns one lowering result for a cancellable prioritized request.
    pub fn lowered_unit_with_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<DiagnosticResult<Option<LoweredUnit>>>, FactQueryError> {
        if let Some(result) = self.published_output_lowering(&key, cancellation)? {
            return Ok(result);
        }

        let published =
            self.lowered_unit_with_cancellation_and_priority(key, cancellation, priority)?;

        Ok(Arc::clone(published.result()))
    }

    fn published_output_lowering(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Option<Arc<DiagnosticResult<Option<LoweredUnit>>>>, FactQueryError> {
        let Some(output) = self.state.source_outputs.published(key)? else {
            return Ok(None);
        };

        let Some(lowered) = &output.lowered else {
            return Ok(None);
        };

        let fact = CompilationFactKey::LoweredUnit(key.clone());

        self.state.fact_runtime.check_request_cycle(&fact)?;
        cancellation.check()?;
        self.state.fact_runtime.record_completed_request(&fact)?;

        Ok(Some(Arc::clone(lowered)))
    }

    fn lowered_unit_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<Option<LoweredUnit>>>, FactQueryError> {
        let priority = self
            .state
            .fact_runtime
            .current_priority()?
            .unwrap_or(QueryPriority::Normal);

        self.lowered_unit_with_cancellation_and_priority(key, cancellation, priority)
    }

    fn lowered_unit_with_cancellation_and_priority(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
        priority: QueryPriority,
    ) -> Result<Arc<PublishedUnitResult<Option<LoweredUnit>>>, FactQueryError> {
        self.unit_query_with_priority(
            &self.state.lowered_units,
            CompilationFactKey::LoweredUnit(key.clone()),
            key.clone(),
            cancellation,
            priority,
            |cancellation| self.compute_lowered_unit(&key, cancellation),
        )
    }

    fn compute_lowered_unit(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<LoweredUnitComputation, FactQueryError> {
        let unit = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
        let unit_diagnostics = unit.result().diagnostics().clone();

        if let Some(unit) = CompileTimeUnit::try_new(
            // The lowering result owns the same immutable unit identity independently of binding.
            key.clone(),
        ) {
            return Ok((
                DiagnosticResult::new(Some(LoweredUnit::CompileTime(unit)), unit_diagnostics),
                Box::new([]),
            ));
        }

        let control_flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;

        let expressions = self.expression_semantics_with_cancellation(key.clone(), cancellation)?;
        let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let body = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;

        let (constant_reference_operands, constant_reference_diagnostics) = self
            .constant_reference_operands(
                unit.result().value(),
                expressions.result().value().types(),
                cancellation,
            )?;

        let completed = self.completed_unit_cleanup(key, cancellation)?;

        let declaration_preconditions = self.lowering_declaration_preconditions(
            expressions.result().value().selections(),
            body.result().value().storage_flow(),
            cancellation,
        )?;

        let diagnostics = DiagnosticBag::merged_all([
            completed.result().diagnostics(),
            unit.result().diagnostics(),
            control_flow.result().diagnostics(),
            expressions.result().diagnostics(),
            patterns.result().diagnostics(),
            storage.result().diagnostics(),
            body.result().diagnostics(),
            behavior.result().diagnostics(),
            &constant_reference_diagnostics,
            declaration_preconditions.diagnostics(),
        ]);

        if diagnostics.has_errors() {
            return Ok((DiagnosticResult::new(None, diagnostics), Box::new([])));
        }

        cancellation.check()?;

        let selected_target = self.selected_target().target();

        let target = MirTargetContract::new(
            // MIR owns the immutable target profile independently of compilation state.
            selected_target.profile().clone(),
            selected_target.runtime_abi(),
        );

        let unit_kind = executable_unit_kind(unit.result().value(), &target);

        let native_static_templates =
            self.native_static_templates(expressions.result().value().selections(), cancellation)?;

        let semantic_values = self.semantic_value_store()?;

        let input = LoweringInput::new(
            unit.result().value(),
            control_flow.result().value(),
            expressions.result().value().types(),
            patterns.result().value(),
            expressions.result().value().literals(),
            body.result().value().refinements(),
            storage.result().value(),
            body.result().value().liveness(),
            body.result().value().storage_flow(),
            body.result().value().dependencies(),
            expressions.result().value().selections(),
            self.available_compiler_known_symbols(),
            body.result().value().asynchronous(),
            // Lowering owns this small selected-site set independently of the cached proof.
            completed.result().value().clone(),
            behavior.result().value(),
            semantic_values,
            &constant_reference_operands,
            unit_kind,
            target,
            declaration_preconditions.value(),
        );

        let runtime_calls =
            self.runtime_lowering_calls(expressions.result().value().selections(), cancellation)?;

        let input = input
            .with_unwind(body.result().value().unwind())
            .with_native_static_templates(&native_static_templates)
            .with_runtime_calls(&runtime_calls);

        let span = self
            .state
            .fact_runtime
            .profile()
            .map(|profile| profile.start(crate::profile::ProfileOperation::Lowering, None));

        let result = lower_unit(input);

        if let Some(span) = span {
            span.finish(crate::profile::result_outcome(&result));
        }

        let mir = result.map_err(|error| match error {
            LoweringError::SemanticValue(error) => FactQueryError::SemanticValueStore(error),
            LoweringError::MirCapacity(error) => FactQueryError::MirCapacity(error),
        })?;

        if let Some(profile) = self.state.fact_runtime.profile() {
            profile.record_metric(crate::profile::ProfileMetricKind::MirUnits, 1);

            profile.record_metric(
                crate::profile::ProfileMetricKind::MirBlocks,
                u64::try_from(mir.blocks().len()).unwrap_or(u64::MAX),
            );

            profile.record_metric(
                crate::profile::ProfileMetricKind::MirOperations,
                u64::try_from(mir.operations().len()).unwrap_or(u64::MAX),
            );
        }

        cancellation.check()?;

        Ok((
            DiagnosticResult::new(Some(LoweredUnit::Mir(Box::new(mir))), diagnostics),
            Box::new([]),
        ))
    }

    fn runtime_lowering_calls(
        &self,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        cancellation: &CancellationToken,
    ) -> Result<
        Vec<(
            bray_symbols::CallableDefinitionId,
            bray_runtime_interface::RuntimeAbiRole,
        )>,
        FactQueryError,
    > {
        let mut calls = Vec::new();

        if self.runtime_roles().is_empty() {
            return Ok(calls);
        }

        for entry in selections.entries() {
            let bray_bound_tree::SemanticSelection::Call(call) = entry.selection() else {
                continue;
            };

            let Some(definition) = call.target().declaration() else {
                continue;
            };

            let bray_symbols::CallableSymbolId::Function(function) = definition.callable_symbol()
            else {
                continue;
            };

            let Some(role) = super::super::foreign::runtime::runtime_role(self, function)? else {
                continue;
            };

            let contract =
                self.foreign_callable_contract_with_cancellation(function, cancellation)?;

            if contract.value().as_ref().is_some_and(|contract| {
                contract.direction() == bray_symbols::ForeignCallableDirection::Import
            }) {
                calls.push((definition, role));
            }
        }

        calls.sort_unstable();
        calls.dedup();

        Ok(calls)
    }

    fn native_static_templates(
        &self,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        cancellation: &CancellationToken,
    ) -> Result<Vec<StaticInstanceTemplateId>, FactQueryError> {
        let mut templates = Vec::new();

        for entry in selections.entries() {
            let bray_bound_tree::SemanticSelection::StaticReference(reference) = entry.selection()
            else {
                continue;
            };

            let declaration = reference.template().declaration();

            let native =
                self.foreign_static_contract_with_cancellation(declaration, cancellation)?;

            let source_address = native.value().as_ref().is_some_and(|contract| {
                contract.direction() == bray_symbols::ForeignCallableDirection::Import
                    || contract.is_mutable()
            });

            let imported_address = self
                .imported_native_boundary_with_cancellation(declaration.into(), cancellation)?
                .is_some_and(|boundary| {
                    matches!(
                        boundary.kind(),
                        bray_package_interface::InterfaceNativeBoundaryKind::Static {
                            mutable: true,
                            ..
                        }
                    ) || boundary.direction() == bray_symbols::ForeignCallableDirection::Import
                });

            if source_address || imported_address {
                templates.push(reference.template());
            }
        }

        templates.sort_unstable();
        templates.dedup();

        Ok(templates)
    }

    fn constant_reference_operands(
        &self,
        unit: &BoundUnit,
        types: &CheckedExpressionTypes,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            Vec<(bray_bound_tree::BoundExpressionId, MirOperand)>,
            DiagnosticBag,
        ),
        FactQueryError,
    > {
        let mut operands = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        for (expression, node) in unit.tree().expressions() {
            let BoundExpression::Name(name) = node else {
                continue;
            };

            let BoundReferenceTarget::Surface(symbol) = name.target() else {
                continue;
            };

            if let AnySymbolId::GenericConstParameter(parameter) = symbol {
                let Some(ty) = types.expression(expression).map(|result| result.ty()) else {
                    continue;
                };

                let term = self
                    .semantic_value_store()?
                    .intern_constant_term(ConstantTermData::Parameter(parameter))
                    .map_err(FactQueryError::SemanticValueStore)?;

                operands.push((expression, MirOperand::ConstantTerm { term, ty }));
                continue;
            }

            let resolved = self.resolve_surface_constant(symbol, cancellation, &mut diagnostics);

            let Some((ty, resolution)) = resolved? else {
                continue;
            };

            let operand = match resolution {
                ConstantReferenceResolution::Value(value) => MirOperand::Constant { value, ty },
                ConstantReferenceResolution::Evaluated(result) => MirOperand::Constant {
                    value: result.value(),
                    ty,
                },
                ConstantReferenceResolution::Term(term) => MirOperand::ConstantTerm { term, ty },
                ConstantReferenceResolution::Cycle { .. }
                | ConstantReferenceResolution::Invalid => continue,
            };

            operands.push((expression, operand));
        }

        operands.sort_unstable_by_key(|(expression, _)| *expression);

        Ok((operands, diagnostics))
    }
}
