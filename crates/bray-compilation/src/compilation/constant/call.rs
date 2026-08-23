use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_checker::{
    CheckerInfrastructureError, CheckerQueryError, CheckerQueryResult, CheckerRequestContext,
    CheckerUnitView, ConstantCallRequest, ConstantCallResolution, ConstantCallResolver,
    ConstantEvaluationInput, ConstantEvaluationUsage, ConstantEvaluator,
    ConstantReferenceResolution, ConstantTemplateResolver, DefaultConstantEvaluator,
    EvaluatedConstantCall, evaluate_constant_callable_template,
    resolve_callable_signature_template,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableConstness, CallableSignatureQuery, ConstantValueData, ConstantValueKind,
    ImportedSymbolSkeleton, SymbolKey, SymbolKeyData, SymbolQueryRequest, TypeData,
};

use super::super::Compilation;
use super::super::binder::binding_query_error;
use super::super::checker::checker_result;
use super::super::unit::semantic_unit_context_for;
use super::definition::{
    call_parameter_values, constant_callable_root, substitute_expression_types,
};
use crate::fact::{CancellationToken, CompilationFactKey, ConstantCallQueryKey, FactQueryError};

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
    ) -> CheckerQueryResult<bool> {
        self.compilation
            .is_constant_callable(callable, self.cancellation)
            .map_err(checker_call_query_error)
    }

    fn resolve(&self, request: &ConstantCallRequest) -> CheckerQueryResult<ConstantCallResolution> {
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
            Err(error) => Err(checker_call_query_error(error)),
        }
    }
}

pub(in crate::compilation) struct CompilationConstantTemplateResolver<'compilation> {
    calls: CompilationConstantCallResolver<'compilation>,
    symbols: &'compilation ImportedSymbolSkeleton,
}

impl<'compilation> CompilationConstantTemplateResolver<'compilation> {
    pub(in crate::compilation) const fn new(
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
    ) -> CheckerQueryResult<bool> {
        self.calls.is_constant_callable(callable)
    }

    fn resolve(&self, request: &ConstantCallRequest) -> CheckerQueryResult<ConstantCallResolution> {
        self.calls.resolve(request)
    }
}

impl ConstantTemplateResolver for CompilationConstantTemplateResolver<'_> {
    fn symbol(&self, key: &SymbolKey) -> Option<bray_symbols::AnySymbolId> {
        match key.data() {
            SymbolKeyData::External(key) => self.symbols.symbol_by_external_key(key),
            _ => self
                .calls
                .compilation
                .symbol_graph()
                .ok()?
                .symbol_for_key(key),
        }
    }

    fn resolve_constant(
        &self,
        instance: bray_symbols::ConstantInstanceKey,
        limits: bray_checker::ConstantEvaluationLimits,
    ) -> CheckerQueryResult<DiagnosticResult<ConstantReferenceResolution>> {
        match self.calls.compilation.constant_instance_with_limits(
            instance,
            limits,
            self.calls.cancellation,
        ) {
            Ok(result) => Ok(DiagnosticResult::new(
                ConstantReferenceResolution::Evaluated(*result.value()),
                result.diagnostics().clone(),
            )),
            Err(FactQueryError::Cycle(_)) => Ok(DiagnosticResult::without_diagnostics(
                ConstantReferenceResolution::Cycle {
                    definition: self
                        .calls
                        .compilation
                        .constant_definition_span(instance.definition())
                        .map_err(checker_call_query_error)?,
                },
            )),
            Err(error) => Err(checker_call_query_error(error)),
        }
    }

    fn resolve_static(
        &self,
        declaration: bray_symbols::StaticSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> CheckerQueryResult<DiagnosticResult<bray_symbols::StaticReferenceSelection>> {
        let binding_context = self
            .calls
            .compilation
            .binding_context(self.calls.cancellation)
            .map_err(checker_call_query_error)?;

        let initializer = self
            .calls
            .compilation
            .static_initializer_key(declaration)
            .map_err(checker_call_query_error)?
            .ok_or(CheckerQueryError::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ))?;

        let (selection, diagnostics) = self
            .calls
            .compilation
            .static_reference_selection(
                declaration,
                substitution,
                self.calls.cancellation,
                &binding_context,
                initializer.source().syntax(),
            )
            .map_err(checker_call_query_error)?;

        if selection.closed_instance().is_none() {
            return Err(CheckerQueryError::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        }

        Ok(DiagnosticResult::new(selection, diagnostics))
    }
}

fn checker_call_query_error(error: FactQueryError) -> CheckerQueryError {
    match error {
        FactQueryError::Cancelled => CheckerQueryError::Cancelled,
        FactQueryError::CheckerInfrastructure(error) => CheckerQueryError::Infrastructure(error),
        FactQueryError::Cycle(_)
        | FactQueryError::InfrastructureFailure
        | FactQueryError::SemanticUnitContext(_) => CheckerQueryError::Infrastructure(
            CheckerInfrastructureError::InvalidConstantEvaluationInput,
        ),
    }
}

impl Compilation {
    pub(in crate::compilation) fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binding_query_error)?;

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

        let key = ConstantCallQueryKey::new(
            callable,
            request.selected_implementation(),
            Arc::from(request.arguments()),
            request.result_type(),
            self.requested_target().profile().clone(),
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
        key: &ConstantCallQueryKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<Option<EvaluatedConstantCall>>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let callable = values
            .callable_instance_data(key.callable())
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let binding_context = self.binding_context(cancellation)?;

        let signature_result = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                callable.definition().callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        let checked_terms = self.checked_constant_terms_for_templates_with_cancellation(
            [
                signature_result.value().callable_type(),
                signature_result.value().result(),
            ],
            cancellation,
        )?;

        let signature = resolve_callable_signature_template(
            values,
            signature_result.value(),
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

        if let Some(evaluated) = self.compiler_known_constant_call(
            *callable,
            key.arguments(),
            key.result_type(),
            cancellation,
        )? {
            return Ok(DiagnosticResult::new(
                Some(evaluated),
                DiagnosticBag::merged_all([
                    signature_result.diagnostics(),
                    checked_terms.diagnostics(),
                ]),
            ));
        }

        let Some(body_key) = self.callable_body_key(callable.definition())? else {
            let address = binding_context
                .imported_semantic_address(callable.definition().callable_symbol().into_any())
                .map_err(binding_query_error)?;

            let Some(address) = address else {
                return Ok(DiagnosticResult::without_diagnostics(None));
            };

            let body =
                self.imported_constant_callable_body_with_cancellation(address, cancellation)?;

            let Some(template) = body.value() else {
                return Ok(DiagnosticResult::new(
                    None,
                    DiagnosticBag::merged_all([
                        signature_result.diagnostics(),
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
                    signature_result.diagnostics(),
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
                    signature_result.diagnostics(),
                    checked_terms.diagnostics(),
                    bound.result().diagnostics(),
                ]),
            ));
        };

        // Independently cached body binding_context each retain the Arc-backed unit key.
        let semantics =
            self.expression_semantics_with_cancellation(body_key.clone(), cancellation)?;

        let patterns = self.patterns_with_cancellation(body_key.clone(), cancellation)?;

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
            key.limits(),
            cancellation,
        )?;

        let resolver = CompilationConstantCallResolver::new(self, cancellation);

        let input = ConstantEvaluationInput::new(&types, &semantics.result().value().1)
            .with_block_root(root, key.result_type())
            .with_patterns(patterns.result().value())
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
            signature_result.diagnostics(),
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

    fn compiler_known_constant_call(
        &self,
        callable: bray_symbols::CallableInstanceData,
        arguments: &[bray_symbols::ConstantValueId],
        result_type: bray_symbols::TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Option<EvaluatedConstantCall>, FactQueryError> {
        let context = self.checker_context(cancellation)?;
        let hook = context
            .implementation_hook(callable.definition().callable_symbol().into_any())
            .map_err(checker_constant_query_error)?;

        let Some(hook) = hook.filter(|hook| hook.is_available()) else {
            return Ok(None);
        };

        let (ImplementationHook::AtomicInitialize, [argument]) = (hook.hook(), arguments) else {
            return Ok(None);
        };

        let values = self.semantic_value_store()?;
        let argument = values
            .constant_value_data(*argument)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        if !matches!(
            argument.kind(),
            ConstantValueKind::Boolean(_)
                | ConstantValueKind::Integer(_)
                | ConstantValueKind::StaticAddress(_)
        ) {
            return Ok(None);
        }

        let value = values
            .intern_constant_value(ConstantValueData::new(result_type, argument.kind().clone()))
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(Some(EvaluatedConstantCall::new(
            value,
            ConstantEvaluationUsage::default(),
        )))
    }
}

fn checker_constant_query_error(error: CheckerQueryError) -> FactQueryError {
    match error {
        CheckerQueryError::Cancelled => FactQueryError::Cancelled,
        CheckerQueryError::Infrastructure(error) => FactQueryError::CheckerInfrastructure(error),
    }
}
