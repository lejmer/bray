use std::sync::Arc;

use bray_binder::SymbolFactProvider;
use bray_checker::{
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerUnitView,
    ConstantCallRequest, ConstantCallResolution, ConstantCallResolver, ConstantEvaluationInput,
    ConstantEvaluator, ConstantReferenceResolution, ConstantTemplateResolver,
    DefaultConstantEvaluator, EvaluatedConstantCall, evaluate_constant_callable_template,
    resolve_callable_signature_template,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableConstness, CallableSignatureFact, ExternalSymbolKey, ImportedSymbolSkeleton,
    SymbolFactRequest, TypeData,
};

use super::super::Compilation;
use super::super::binder::binder_fact_error;
use super::super::checker::checker_result;
use super::super::unit::semantic_unit_context_for;
use super::definition::{
    call_parameter_values, constant_callable_root, substitute_expression_types,
};
use crate::fact::{CancellationToken, CompilationFactKey, ConstantCallFactKey, FactQueryError};

pub(in crate::compilation) struct CompilationConstantCallResolver<'compilation> {
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
}

impl<'compilation> CompilationConstantCallResolver<'compilation> {
    pub(in crate::compilation) const fn new(
        compilation: &'compilation Compilation,
        cancellation: &'compilation CancellationToken,
    ) -> Self {
        Self {
            compilation,
            cancellation,
        }
    }
}

impl ConstantCallResolver for CompilationConstantCallResolver<'_> {
    fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
    ) -> CheckerFactResult<bool> {
        self.compilation
            .is_constant_callable(callable, self.cancellation)
            .map_err(checker_call_fact_error)
    }

    fn resolve(&self, request: &ConstantCallRequest) -> CheckerFactResult<ConstantCallResolution> {
        match self
            .compilation
            .constant_call_with_cancellation(request, self.cancellation)
        {
            Ok(result) => match result.value() {
                Some(value) => Ok(ConstantCallResolution::Evaluated(DiagnosticResult::new(
                    *value,
                    result.diagnostics().clone(),
                ))),
                None => Ok(ConstantCallResolution::Ineligible(
                    result.diagnostics().clone(),
                )),
            },
            Err(FactQueryError::Cycle(_)) => Ok(ConstantCallResolution::Cycle),
            Err(error) => Err(checker_call_fact_error(error)),
        }
    }
}

struct CompilationConstantTemplateResolver<'compilation> {
    calls: CompilationConstantCallResolver<'compilation>,
    symbols: &'compilation ImportedSymbolSkeleton,
}

impl<'compilation> CompilationConstantTemplateResolver<'compilation> {
    const fn new(
        compilation: &'compilation Compilation,
        cancellation: &'compilation CancellationToken,
        symbols: &'compilation ImportedSymbolSkeleton,
    ) -> Self {
        Self {
            calls: CompilationConstantCallResolver::new(compilation, cancellation),
            symbols,
        }
    }
}

impl ConstantCallResolver for CompilationConstantTemplateResolver<'_> {
    fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
    ) -> CheckerFactResult<bool> {
        self.calls.is_constant_callable(callable)
    }

    fn resolve(&self, request: &ConstantCallRequest) -> CheckerFactResult<ConstantCallResolution> {
        self.calls.resolve(request)
    }
}

impl ConstantTemplateResolver for CompilationConstantTemplateResolver<'_> {
    fn symbol(&self, key: &ExternalSymbolKey) -> Option<bray_symbols::AnySymbolId> {
        self.symbols.symbol_by_external_key(key)
    }

    fn resolve_constant(
        &self,
        instance: bray_symbols::ConstantInstanceKey,
    ) -> CheckerFactResult<DiagnosticResult<ConstantReferenceResolution>> {
        match self
            .calls
            .compilation
            .constant_instance_with_cancellation(instance, self.calls.cancellation)
        {
            Ok(result) => Ok(DiagnosticResult::new(
                ConstantReferenceResolution::Value(*result.value()),
                result.diagnostics().clone(),
            )),
            Err(FactQueryError::Cycle(_)) => Ok(DiagnosticResult::without_diagnostics(
                ConstantReferenceResolution::Cycle,
            )),
            Err(error) => Err(checker_call_fact_error(error)),
        }
    }
}

fn checker_call_fact_error(error: FactQueryError) -> CheckerFactError {
    match error {
        FactQueryError::Cancelled => CheckerFactError::Cancelled,
        FactQueryError::CheckerInfrastructure(error) => CheckerFactError::Infrastructure(error),
        FactQueryError::Cycle(_)
        | FactQueryError::InfrastructureFailure
        | FactQueryError::SemanticUnitContext(_) => CheckerFactError::Infrastructure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput,
        ),
    }
}

impl Compilation {
    fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let values = self.semantic_value_store()?;
        let facts = self.binder_facts(cancellation)?;

        let signature = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binder_fact_error)?;

        signature
            .value()
            .constness(values)
            .map(|constness| constness == CallableConstness::Constant)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }

    pub(super) fn constant_call_with_cancellation(
        &self,
        request: &ConstantCallRequest,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<EvaluatedConstantCall>>>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let callable = values
            .intern_callable_instance(request.callable())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let key = ConstantCallFactKey::new(
            callable,
            request.selected_implementation(),
            Arc::from(request.arguments()),
            request.result_type(),
            self.options().selected_target().profile().clone(),
            request.limits(),
        );

        let cell = self.state.constant_calls.cell(key.clone())?;

        let published = cell.get_or_compute_with_cycle_key(
            &self.state.fact_runtime,
            CompilationFactKey::ConstantCall(key.clone()),
            CompilationFactKey::ConstantCallCycle(key.dependency_key()),
            cancellation,
            || self.compute_constant_call(&key, cancellation).map(Arc::new),
        )?;

        Ok(Arc::clone(published))
    }

    fn compute_constant_call(
        &self,
        key: &ConstantCallFactKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<EvaluatedConstantCall>>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let callable = values
            .callable_instance_data(key.callable())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let facts = self.binder_facts(cancellation)?;

        let signature_fact = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binder_fact_error)?;

        let checked_terms = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature_fact.value().callable_type(),
                signature_fact.value().result(),
            ],
            cancellation,
        )?;

        let signature = resolve_callable_signature_template(
            values,
            signature_fact.value(),
            callable.substitution(),
            checked_terms.value(),
        )
        .map_err(FactQueryError::CheckerInfrastructure)?
        .ok_or(FactQueryError::InfrastructureFailure)?;

        if signature.result() != key.result_type() {
            return Err(FactQueryError::InfrastructureFailure);
        }

        let callable_type = values
            .type_data(signature.callable_type())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::Callable(callable_type) = callable_type.as_ref() else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        if callable_type.constness() != CallableConstness::Constant {
            return Ok(DiagnosticResult::without_diagnostics(None));
        }

        let Some(body_key) = self.callable_body_key(callable.definition())? else {
            let address = facts
                .imported_fact_address(callable.definition().callable_symbol().into_any())
                .map_err(binder_fact_error)?;

            let Some(address) = address else {
                return Ok(DiagnosticResult::without_diagnostics(None));
            };

            let body =
                self.imported_constant_callable_body_with_cancellation(address, cancellation)?;

            let Some(template) = body.value() else {
                return Ok(DiagnosticResult::new(
                    None,
                    DiagnosticBag::merged_all([
                        signature_fact.diagnostics(),
                        checked_terms.diagnostics(),
                        body.diagnostics(),
                    ]),
                ));
            };

            let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

            let imported = imported
                .value()
                .as_deref()
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let context = self.checker_context(cancellation)?;
            let resolver = CompilationConstantTemplateResolver::new(self, cancellation, imported);

            let diagnostic_span = self
                .dependency_interface_input(address.interface())
                .and_then(crate::request::DependencyInterfaceInput::dependency_span);

            let request = ConstantCallRequest::new(
                *callable,
                key.selected_implementation(),
                key.arguments().iter().copied(),
                key.result_type(),
                key.limits(),
            );

            let evaluated = checker_result(evaluate_constant_callable_template(
                &context,
                template,
                &request,
                &resolver,
                diagnostic_span,
            ))?;

            return Ok(DiagnosticResult::new(
                Some(*evaluated.value()),
                DiagnosticBag::merged_all([
                    signature_fact.diagnostics(),
                    checked_terms.diagnostics(),
                    body.diagnostics(),
                    evaluated.diagnostics(),
                ]),
            ));
        };

        let bound = self.bound_unit_with_cancellation(body_key.clone(), cancellation)?;

        let Some(root) = constant_callable_root(bound.result().value()) else {
            return Ok(DiagnosticResult::new(
                None,
                DiagnosticBag::merged_all([
                    signature_fact.diagnostics(),
                    checked_terms.diagnostics(),
                    bound.result().diagnostics(),
                ]),
            ));
        };

        // Independently cached body facts each retain the Arc-backed unit key.
        let semantics =
            self.expression_semantics_with_cancellation(body_key.clone(), cancellation)?;

        let patterns = self.pattern_facts_with_cancellation(body_key.clone(), cancellation)?;

        let context = self.checker_context_for(&body_key, cancellation)?;

        let semantic_context =
            semantic_unit_context_for(context.symbols(), bound.result().value())?;

        let types = substitute_expression_types(
            values,
            &semantics.result().value().0,
            callable.substitution(),
        )?;

        let parameters = call_parameter_values(values, &signature, key.arguments())?;

        let (references, dependency_diagnostics) = self.concrete_call_references(
            bound.result().value(),
            &semantics.result().value().1,
            callable.substitution(),
            key.selected_implementation(),
            &parameters,
            cancellation,
        )?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(&types, &semantics.result().value().1)
            .with_block_root(root, key.result_type())
            .with_pattern_facts(patterns.result().value())
            .with_references(references)
            .with_call_resolver(&resolver)
            .with_limits(key.limits());

        let unit = CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
            .map_err(|error| {
                FactQueryError::CheckerInfrastructure(CheckerInfrastructureError::InvalidUnitView(
                    error,
                ))
            })?;

        let evaluated = checker_result(
            DefaultConstantEvaluator.evaluate_constant_with_references(unit, &input),
        )?;

        let diagnostics = DiagnosticBag::merged_all([
            signature_fact.diagnostics(),
            checked_terms.diagnostics(),
            bound.result().diagnostics(),
            semantics.result().diagnostics(),
            patterns.result().diagnostics(),
            &dependency_diagnostics,
            evaluated.diagnostics(),
        ]);

        Ok(DiagnosticResult::new(
            Some(EvaluatedConstantCall::new(
                evaluated.value().value(),
                evaluated.value().usage(),
            )),
            diagnostics,
        ))
    }
}
