use std::sync::Arc;

use bray_bound_tree::{
    BoundExpression, BoundUnit, BoundUnitKey, BoundUnitKind, CheckedExpressionSemantics,
    SelectedArgument, SemanticSelection,
};
use bray_checker::{
    TargetAbiValue, TargetCallableAbiRequirement, TargetValidityRequest, TargetValidityRequirement,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticEmissionEvaluationFailure,
    DiagnosticEmissionFailure, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{AnySymbolId, NamedTypeSymbolId};

use super::publication::ordered_diagnostic_collections;
use crate::compilation::constant::constant_definition_id;
use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError, PublishedUnitResult, publish_diagnostics};

pub(super) fn semantic_query_failure_diagnostics(
    key: &BoundUnitKey,
    error: &crate::compilation::SemanticQueryError,
) -> DiagnosticBag {
    let anchor = key.source().syntax();

    let source = error
        .source()
        .unwrap_or_else(|| SourceSpan::new(anchor.source_id(), anchor.full_range()));

    let failure =
        DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::SemanticQuery(
            crate::fact::diagnostic_semantic_query_failure(error),
        ));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(anchor.full_range().start().bytes()),
        DiagnosticKind::CheckingCapacityExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::emission_failure(failure));

    DiagnosticBag::single(diagnostic.with_primary_span(source).with_label(
        DiagnosticLabel::primary(DiagnosticLabelKind::CompilerCapacityExceeded, source),
    ))
}

impl Compilation {
    pub(in crate::compilation) fn semantic_unit_diagnostic_work(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<(Vec<DiagnosticBag>, Vec<UnitDiagnosticOrder>), FactQueryError> {
        if let Some(output) = self.published_source_output(key, cancellation)? {
            // Diagnostic publication owns the shared bags and small nested-unit inventory.
            return Ok((output.diagnostics.to_vec(), output.nested.to_vec()));
        }

        let (bound, sources) =
            match self.semantic_unit_diagnostic_sources(key.clone(), cancellation) {
                Ok((bound, sources)) => (Some(bound), sources),
                Err(FactQueryError::SemanticQuery(error)) => {
                    let bound = self
                        .bound_unit_with_cancellation(key.clone(), cancellation)
                        .ok();

                    (bound, vec![semantic_query_failure_diagnostics(key, &error)])
                }
                Err(error) => return Err(error),
            };

        let nested = bound
            .iter()
            .flat_map(|bound| bound.result().value().nested_units().iter().cloned())
            .map(unit_order_key)
            .collect();

        Ok((sources, nested))
    }

    pub(in crate::compilation) fn semantic_unit_diagnostics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let (_, sources) = self.semantic_unit_diagnostic_sources(key, cancellation)?;

        Ok(publish_diagnostics(
            self.state.fact_runtime.profile(),
            ordered_diagnostic_collections(sources.iter()),
        ))
    }

    pub(super) fn semantic_unit_diagnostic_sources(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<(Arc<PublishedUnitResult<BoundUnit>>, Vec<DiagnosticBag>), FactQueryError> {
        // Each source request owns the same Arc-backed unit identity independently.
        let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

        let declared_types =
            self.declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

        let embedded_constants = self.checked_constant_terms_for_templates_with_cancellation(
            declared_types
                .result()
                .value()
                .evidence()
                .iter()
                .map(|evidence| evidence.template())
                .chain(declared_types.result().value().callable_type())
                .chain(declared_types.result().value().callable_result()),
            cancellation,
        )?;

        let expression_semantics =
            self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

        let control_flow = self.control_flow_with_cancellation(key.clone(), cancellation)?;
        let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
        let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
        let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
        let body_semantics = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
        let behavior = self.body_behavior_with_cancellation(key.clone(), cancellation)?;
        let cleanup = self.completed_unit_cleanup(&key, cancellation)?;

        let target_validity = self.semantic_unit_target_validity(
            bound.result().value(),
            expression_semantics.result().value(),
            cancellation,
        )?;

        // The batch retains bags without extending the lifetime of checked values.
        let mut sources = vec![
            bound.result().diagnostics().clone(),
            declared_types.result().diagnostics().clone(),
            embedded_constants.diagnostics().clone(),
            expression_semantics.result().diagnostics().clone(),
            control_flow.result().diagnostics().clone(),
            patterns.result().diagnostics().clone(),
            storage.result().diagnostics().clone(),
            memory.result().diagnostics().clone(),
            body_semantics.result().diagnostics().clone(),
            cleanup.result().diagnostics().clone(),
            behavior.result().diagnostics().clone(),
            target_validity,
        ];

        if key.kind() == BoundUnitKind::ConstantTemplate {
            let symbols = self.symbol_graph()?;

            let owner = symbols
                .symbol_for_key(key.declared_owner())
                .ok_or_else(|| {
                    FactQueryError::from(SemanticQueryFailure::contract(
                        SemanticQueryContext::SymbolKey(key.declared_owner().clone()),
                        SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                    ))
                })?;

            if let AnySymbolId::Static(declaration) = owner {
                sources.push(
                    self.static_instance_template(declaration)?
                        .diagnostics()
                        .clone(),
                );

                return Ok((bound, sources));
            }

            let definition = constant_definition_id(owner).ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::Symbol(owner),
                    SemanticQueryViolation::Missing(SemanticDataKind::ConstantDefinition),
                ))
            })?;

            let template = self.constant_definition(definition)?;

            sources.push(template.diagnostics().clone());

            if self.constant_can_evaluate_without_context(definition)? {
                let substitution = crate::compilation::substitution::empty_substitution(
                    self.semantic_value_store()?,
                    definition.into_any(),
                )?;

                let instance =
                    bray_symbols::ConstantInstanceKey::new(definition, substitution, None);

                let value = self.constant_instance_with_cancellation(instance, cancellation)?;

                sources.push(value.diagnostics().clone());
            }
        }

        Ok((bound, sources))
    }

    fn semantic_unit_target_validity(
        &self,
        unit: &BoundUnit,
        semantics: &CheckedExpressionSemantics,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let types = semantics.types();
        let selections = semantics.selections();

        let values = self.semantic_value_store()?;
        let mut diagnostics = DiagnosticBag::new();

        for entry in types.entries() {
            cancellation.check()?;

            if entry.result().is_recovered() {
                continue;
            }

            let data = values.type_data(entry.result().ty());

            let bray_symbols::TypeData::Named { definition, .. } = data.as_ref() else {
                continue;
            };

            let NamedTypeSymbolId::Struct(definition) = definition else {
                continue;
            };

            let Some(role) = self
                .available_compiler_known_symbols()
                .provider()
                .role_registry()
                .symbol_representation(*definition)
            else {
                continue;
            };

            let expression = unit.view().expression(entry.expression()).ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::BoundExpression {
                        unit: unit.unit(),
                        expression: entry.expression(),
                    },
                    SemanticQueryViolation::Missing(SemanticDataKind::BoundExpression),
                ))
            })?;

            let request = TargetValidityRequest::new(
                expression.origin().source_anchor(),
                TargetValidityRequirement::Representation(role),
            );

            let result = self.target_validity_with_cancellation(request, cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());
        }

        for entry in selections.entries() {
            cancellation.check()?;

            let SemanticSelection::Call(call) = entry.selection() else {
                continue;
            };

            if call.abi() == bray_symbols::CallableAbi::Bray {
                continue;
            }

            let Some(BoundExpression::Call(expression)) =
                unit.view().expression(entry.expression())
            else {
                return Err(SemanticQueryFailure::contract(
                    SemanticQueryContext::BoundExpression {
                        unit: unit.unit(),
                        expression: entry.expression(),
                    },
                    SemanticQueryViolation::Unsupported(SemanticDataKind::BoundExpression),
                )
                .into());
            };

            let callee = types.expression(expression.callee()).ok_or_else(|| {
                FactQueryError::from(SemanticQueryFailure::contract(
                    SemanticQueryContext::BoundExpression {
                        unit: unit.unit(),
                        expression: expression.callee(),
                    },
                    SemanticQueryViolation::Missing(SemanticDataKind::Type),
                ))
            })?;

            if callee.is_recovered() {
                continue;
            }

            let data = values.type_data(callee.ty());

            let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                panic!(
                    "checked foreign call has a callable callee type: unit={:?}, expression={:?}, callee={callee:?}, target={:?}, type={data:?}",
                    unit.key(),
                    entry.expression(),
                    call.target()
                );
            };

            let mut parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    crate::compilation::foreign::target_abi_value_from_type(
                        self,
                        parameter.ty(),
                        cancellation,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .map(|value| value.unwrap_or(TargetAbiValue::Unsupported))
                .collect::<Vec<_>>();

            if call.implementation_hook() == Some(ImplementationHook::NativeThreadExecution)
                && let Some(operation) = parameters.first_mut()
            {
                *operation = TargetAbiValue::RawPointer;
            }

            for argument in call.arguments() {
                let SelectedArgument::Explicit {
                    ordinal,
                    conversion,
                    ..
                } = argument
                else {
                    continue;
                };

                if usize::try_from(*ordinal)
                    .is_ok_and(|ordinal| ordinal < callable.parameters().len())
                {
                    continue;
                }

                let value = crate::compilation::foreign::target_abi_value_from_type(
                    self,
                    conversion.target_type(),
                    cancellation,
                )?
                .unwrap_or(TargetAbiValue::Unsupported);

                parameters.push(value);
            }

            let result = crate::compilation::foreign::target_abi_value_from_type(
                self,
                callable.result(),
                cancellation,
            )?;

            let request = TargetValidityRequest::new(
                expression.origin().source_anchor(),
                TargetValidityRequirement::CallableAbi(
                    TargetCallableAbiRequirement::new(call.abi(), parameters, result)
                        .with_variadic(callable.is_variadic()),
                ),
            );

            let result = self.target_validity_with_cancellation(request, cancellation)?;

            diagnostics.add_range(result.diagnostics().iter().cloned());
        }

        Ok(diagnostics)
    }
}

pub(in crate::compilation) type UnitDiagnosticOrder = (
    bray_source::SourceId,
    bray_source::TextRange,
    BoundUnitKind,
    BoundUnitKey,
);

pub(super) fn unit_order_key(key: BoundUnitKey) -> UnitDiagnosticOrder {
    let source = key.source().syntax();

    (source.source_id(), source.full_range(), key.kind(), key)
}
