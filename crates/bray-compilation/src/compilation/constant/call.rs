use std::sync::Arc;

use bray_binder::SymbolQueryProvider;
use bray_checker::{
    CheckerRequestContext, ConstantCallRequest, ConstantCallResolution, ConstantCallResolver,
    ConstantEvaluationUsage, ConstantReferenceResolution, ConstantTemplateResolver,
    EvaluatedConstantCall, evaluate_constant_callable_template,
};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    CallableConstness, CallableSignatureQuery, ConstantValueData,
    ConstantValueKind, ImportedSymbolSkeleton, SymbolKey, SymbolKeyData, SymbolQueryRequest,
    TypeData,
};

use super::super::Compilation;
use super::super::binder::binding_query_error;
use super::super::checker::{checker_query_error, checker_result};

use crate::fact::{CancellationToken, CompilationFactKey, ConstantCallQueryKey, FactQueryError};

type CheckerQueryError = bray_checker::CheckerQueryError<FactQueryError>;
type CheckerQueryResult<T> = bray_checker::CheckerQueryResult<T, FactQueryError>;

pub(in crate::compilation) struct CompilationConstantCallResolver<'compilation> {
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
    substitution: Option<bray_symbols::GenericSubstitutionId>,
}

impl<'compilation> CompilationConstantCallResolver<'compilation> {
    pub(in crate::compilation) const fn new(
        compilation: &'compilation Compilation,
        cancellation: &'compilation CancellationToken,
    ) -> Self {
        Self {
            compilation,
            cancellation,
            substitution: None,
        }
    }
    pub(in crate::compilation) const fn with_substitution(
        mut self,
        substitution: bray_symbols::GenericSubstitutionId,
    ) -> Self {
        self.substitution = Some(substitution);

        self
    }
}

impl ConstantCallResolver for CompilationConstantCallResolver<'_> {
    type UpstreamError = FactQueryError;

    fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
    ) -> CheckerQueryResult<bool> {
        self.compilation
            .is_constant_callable(callable, self.cancellation)
            .map_err(checker_call_query_error)
    }

    fn contract(&self, target: &bray_checker::ConstantCallTarget) -> CheckerQueryResult<DiagnosticResult<Option<bray_checker::ConstantCallContract>>> {
        self.compilation.constant_call_contract(target, self.cancellation).map_err(checker_call_query_error)
    }

    fn requirement_origin(&self, target: &bray_checker::ConstantCallTarget, index: usize) -> CheckerQueryResult<Option<bray_source::SourceSpan>> {
        self.compilation.constant_requirement_origin(target, index, self.cancellation).map_err(checker_call_query_error)
    }

    fn resolve(&self, request: &ConstantCallRequest) -> CheckerQueryResult<ConstantCallResolution> {
        let values = self.compilation.semantic_value_store().map_err(checker_call_query_error)?;
        let selected = match request.target() {
            bray_checker::ConstantCallTarget::Anonymous(key, None) => {
                let symbols = self.compilation.symbol_graph().map_err(checker_call_query_error)?;
                let owner = symbols.symbol_for_key(key.declared_owner()).expect("anonymous body retains its enclosing declaration");
                let parameters = crate::compilation::binder::visible_generic_parameters(symbols, owner);
                crate::compilation::substitution::identity_substitution(values,
                    bray_symbols::GenericOwnerId::try_new(owner).expect("anonymous lexical owner has a generic context"), &parameters).map_err(checker_call_query_error)?
            }
            _ => request.substitution(),
        };
        let selected = self.substitution.map(|parent| values.substitute_generic_substitution(selected, parent)).transpose()
            .map_err(|error| checker_call_query_error(error.into()))?.unwrap_or(selected);
        let target = match request.target() {
            bray_checker::ConstantCallTarget::Declaration(callable) => {
                let callable = bray_symbols::CallableInstanceData::new(callable.definition(), selected);
                if !self.is_constant_callable(callable)? { return Ok(ConstantCallResolution::Ineligible(DiagnosticBag::new())); }
                bray_checker::ConstantCallTarget::Declaration(callable)
            }
            bray_checker::ConstantCallTarget::Anonymous(key, _) => {
                if !self.compilation.unit_requires_constant_evaluation(key, self.cancellation).map_err(checker_call_query_error)? {
                    return Ok(ConstantCallResolution::Ineligible(DiagnosticBag::new()));
                }
                bray_checker::ConstantCallTarget::Anonymous(key.clone(), Some(selected))
            }
            bray_checker::ConstantCallTarget::Default(provider, _) => bray_checker::ConstantCallTarget::Default(*provider, selected),
            bray_checker::ConstantCallTarget::Predicate(predicate) => bray_checker::ConstantCallTarget::Predicate(bray_symbols::PredicateInstanceData::new(predicate.definition(), selected)),
        };
        let witness = match self.substitution {
            Some(parent) => request.selected_implementation().map(|witness| values.substitute_implementation_instance(witness, parent)).transpose()
                .map_err(|error| checker_call_query_error(error.into()))?,
            None => request.selected_implementation(),
        };
        let request = ConstantCallRequest::new(target, witness, request.arguments().iter().copied(), request.result_type(), request.limits());
        match self.compilation.constant_call_with_cancellation(&request, self.cancellation) {
            Ok(result) => match result.value() {
                Some(value) => Ok(ConstantCallResolution::Evaluated(DiagnosticResult::new(*value, result.diagnostics().clone()))),
                None => Ok(ConstantCallResolution::Ineligible(result.diagnostics().clone())),
            },
            Err(FactQueryError::Cycle(_)) => Ok(ConstantCallResolution::Cycle),
            Err(error) => Err(checker_call_query_error(error)),
        }
    }
}

pub(in crate::compilation) struct CompilationConstantTemplateResolver<'compilation> {
    calls: CompilationConstantCallResolver<'compilation>,
    symbols: Option<&'compilation ImportedSymbolSkeleton>,
}

impl<'compilation> CompilationConstantTemplateResolver<'compilation> {
    pub(in crate::compilation) const fn new(
        compilation: &'compilation Compilation,
        cancellation: &'compilation CancellationToken,
        symbols: Option<&'compilation ImportedSymbolSkeleton>,
    ) -> Self {
        Self {
            calls: CompilationConstantCallResolver::new(compilation, cancellation),
            symbols,
        }
    }
}

impl ConstantCallResolver for CompilationConstantTemplateResolver<'_> {
    type UpstreamError = FactQueryError;

    fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
    ) -> CheckerQueryResult<bool> {
        self.calls.is_constant_callable(callable)
    }

    fn contract(&self, target: &bray_checker::ConstantCallTarget) -> CheckerQueryResult<DiagnosticResult<Option<bray_checker::ConstantCallContract>>> {
        self.calls.contract(target)
    }

    fn requirement_origin(&self, target: &bray_checker::ConstantCallTarget, index: usize) -> CheckerQueryResult<Option<bray_source::SourceSpan>> {
        self.calls.requirement_origin(target, index)
    }

    fn resolve(&self, request: &ConstantCallRequest) -> CheckerQueryResult<ConstantCallResolution> {
        self.calls.resolve(request)
    }
}

impl ConstantTemplateResolver for CompilationConstantTemplateResolver<'_> {
    fn requirement_template(&self, target: &bray_checker::ConstantCallTarget, index: usize)
        -> CheckerQueryResult<DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>> {
        self.calls.compilation.constant_requirement_template(target, index, self.calls.cancellation)
            .map_err(checker_call_query_error)
    }

    fn symbol(&self, key: &SymbolKey) -> Option<bray_symbols::AnySymbolId> {
        match key.data() {
            SymbolKeyData::External(key) => self.symbols?.symbol_by_external_key(key),
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
            .map_err(checker_call_query_error)?.unwrap_or_else(|| panic!("resolve_static requires retained checked input: self .calls .compilation .static_initializer_key(declaration) .map_err(checker_call_query_error)?, declaration: {declaration:?}, substitution: {substitution:?}"));

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
            panic!(
                "Constant-evaluation inputs do not describe the requested bound unit. in resolve_static"
            );
        }

        Ok(DiagnosticResult::new(selection, diagnostics))
    }
}

fn checker_call_query_error(error: FactQueryError) -> CheckerQueryError {
    checker_query_error(error)
}

impl Compilation {
    pub(in crate::compilation) fn is_constant_callable(
        &self,
        callable: bray_symbols::CallableInstanceData,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        self.declares_constant_callable(callable.definition(), cancellation)
    }

    pub(in crate::compilation) fn declares_constant_callable(
        &self,
        definition: bray_symbols::CallableDefinitionId,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let values = self.semantic_value_store()?;
        let binding_context = self.binding_context(cancellation)?;

        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
                definition.callable_symbol(),
            ))
            .map_err(binding_query_error)?;

        signature
            .value()
            .constness(values)
            .map(|constness| constness == CallableConstness::Constant)
            .map_err(FactQueryError::from)
    }

    pub(in crate::compilation) fn constant_call_with_cancellation(
        &self,
        request: &ConstantCallRequest,
        cancellation: &CancellationToken,
    ) -> Result<Arc<DiagnosticResult<Option<EvaluatedConstantCall>>>, FactQueryError> {
        let key = ConstantCallQueryKey::new(
            request.target().clone(),
            request.selected_implementation(),
            Arc::from(request.arguments()),
            request.result_type(),
            self.requested_target().profile().clone(),
            request.limits(),
        );

        let cell = self.state.constant_calls.cell(key.clone())?;

        let published = cell.get_or_compute_with_cycle_key(
            &self.state.fact_runtime,
            CompilationFactKey::ConstantCall(Arc::new(key.clone())),
            CompilationFactKey::ConstantCallCycle(Arc::new(key.dependency_key())),
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
        let target = key.invocation();
        let checked_contract = self.constant_call_contract(target, cancellation)?;
        let Some(contract) = checked_contract.value() else {
            return Ok(DiagnosticResult::new(None, checked_contract.diagnostics().clone()));
        };
        assert_eq!(contract.result(), key.result_type(), "checked constant invocation retains its selected result type for {target:?}");
        if let bray_checker::ConstantCallTarget::Declaration(callable) = target {
            if let Some(evaluated) = self.compiler_known_constant_call(*callable, key.arguments(), key.result_type(), cancellation)? {
                assert!(contract.requirements().is_empty(), "compiler-known constant fast path has a requirement-free catalog contract: {callable:?}");
                return Ok(DiagnosticResult::new(Some(evaluated), checked_contract.diagnostics().clone()));
            }
        }
        let (template, span) = self.constant_invocation_template(target, cancellation)?;
        let Some(body) = template.value() else { return Ok(DiagnosticResult::new(None, template.into_parts().1)); };
        if matches!(target, bray_checker::ConstantCallTarget::Default(_, _)) && !body.behavior().execution().constant_eligible() {
            return Ok(DiagnosticResult::new(None, template.into_parts().1));
        }
        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;
        let context = self.checker_context(cancellation)?;
        let resolver = CompilationConstantTemplateResolver::new(self, cancellation, imported.value().as_deref());
        let request = ConstantCallRequest::new(target.clone(), key.selected_implementation(), key.arguments().iter().copied(), key.result_type(), key.limits());
        let evaluated = checker_result(evaluate_constant_callable_template(&context, body, &request, &resolver, span, contract.requirements()))?;
        Ok(DiagnosticResult::new(Some(*evaluated.value()), DiagnosticBag::merged_all([
            checked_contract.diagnostics(), template.diagnostics(), imported.diagnostics(), evaluated.diagnostics(),
        ])))
    }

    fn constant_invocation_template(&self, target: &bray_checker::ConstantCallTarget, cancellation: &CancellationToken)
        -> Result<(DiagnosticResult<Option<bray_bound_tree::CheckedTemplate>>, Option<bray_source::SourceSpan>), FactQueryError> {
        let binding = self.binding_context(cancellation)?;
        let template = match target {
            bray_checker::ConstantCallTarget::Declaration(callable) => {
                if let Some(key) = self.callable_body_key(callable.definition())? {
                    let body = self.constant_body_template(key.clone(), cancellation)?;
                    return Ok((body.result().as_ref().clone(), Some(bray_source::SourceSpan::new(key.source().syntax().source_id(), key.source().syntax().full_range()))));
                }
                let address = binding.imported_semantic_address(callable.definition().symbol()).map_err(binding_query_error)?;
                let Some(address) = address else { return Ok((DiagnosticResult::without_diagnostics(None), None)); };
                let body = self.imported_constant_callable_body_with_cancellation(address, cancellation)?;
                let span = self.dependency_interface_input(address.interface()).and_then(crate::request::DependencyInterfaceInput::dependency_span);
                return Ok((body.as_ref().clone().map(|body| body.map(|body| body.as_ref().clone())), span));
            }
            bray_checker::ConstantCallTarget::Anonymous(key, _) => self.constant_body_template(key.clone(), cancellation)?.result().as_ref().clone(),
            bray_checker::ConstantCallTarget::Default(provider, _) => self.checked_default_template(*provider, cancellation)?,
            bray_checker::ConstantCallTarget::Predicate(predicate) => self.constant_predicate_template(predicate.definition(), cancellation)?,
        };
        Ok((template, None))
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

        let values = self.semantic_value_store()?;

        let value = match (hook.hook(), arguments) {
            (
                ImplementationHook::NullableIsPresent | ImplementationHook::NullableIsAbsent,
                [receiver],
            ) => {
                let receiver = values.constant_value_data(values.observed_constant_value(*receiver));

                let receiver_type = values.type_data(receiver.ty());

                if !matches!(receiver_type.as_ref(), TypeData::Nullable(_)) {
                    return Ok(None);
                }

                let is_present = match receiver.kind() {
                    ConstantValueKind::NullableAbsent => false,
                    ConstantValueKind::Error => return Ok(None),
                    _ => true,
                };

                let value = if hook.hook() == ImplementationHook::NullableIsPresent {
                    is_present
                } else {
                    !is_present
                };

                values
                    .intern_constant_value(ConstantValueData::new(
                        result_type,
                        ConstantValueKind::Boolean(value),
                    ))
                    .map_err(FactQueryError::SemanticValueStore)?
            }
            (ImplementationHook::SequenceLength | ImplementationHook::SequenceIsEmpty, [receiver]) => {
                let receiver = values.constant_value_data(values.observed_constant_value(*receiver));
                let count = match receiver.kind() {
                    ConstantValueKind::Array(elements) => u64::try_from(elements.len()).expect("bounded constant sequence length fits u64"),
                    ConstantValueKind::String(text) => u64::try_from(text.len()).expect("bounded UTF-8 byte length fits u64"),
                    ConstantValueKind::StaticAddress(_) => {
                        let ty = values.type_data(values.unborrowed_type(receiver.ty()));
                        let TypeData::Array { length, .. } = ty.as_ref() else { return Ok(None); };
                        let length = values.substitute_constant_term(*length, callable.substitution())?;
                        let bray_symbols::ConstantTermData::Value(length) = *values.constant_term_data(length) else { return Ok(None); };
                        let length = values.constant_value_data(length);
                        let ConstantValueKind::Integer(length) = length.kind() else { panic!("checked array extent is an integer"); };
                        length.to_u64().expect("checked target array extent fits u64")
                    }
                    _ => return Ok(None),
                };
                let kind = if hook.hook() == ImplementationHook::SequenceIsEmpty { ConstantValueKind::Boolean(count == 0) }
                    else { ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(count)) };
                values.intern_constant_value(ConstantValueData::new(result_type, kind))?
            }
            (ImplementationHook::AtomicInitialize, [argument]) => {
                let argument = values.constant_value_data(*argument);

                if !matches!(
                    argument.kind(),
                    ConstantValueKind::Boolean(_)
                        | ConstantValueKind::Integer(_)
                        | ConstantValueKind::StaticAddress(_)
                ) {
                    return Ok(None);
                }

                // The accepted variants contain only scalar values or one interned static address.
                values
                    .intern_constant_value(ConstantValueData::new(
                        result_type,
                        argument.kind().clone(),
                    ))
                    .map_err(FactQueryError::SemanticValueStore)?
            }
            (ImplementationHook::UninitFrom, [argument]) => {
                let argument = values.constant_value_data(*argument);

                // Protected storage retains the value's representation. Aggregate children
                // are interned IDs, so this copies no represented payload or ownership.
                values
                    .intern_constant_value(ConstantValueData::new(
                        result_type,
                        argument.kind().clone(),
                    ))
                    .map_err(FactQueryError::SemanticValueStore)?
            }
            (ImplementationHook::UninitNew, []) => values
                .intern_constant_value(ConstantValueData::new(
                    result_type,
                    ConstantValueKind::product([]),
                ))
                .map_err(FactQueryError::SemanticValueStore)?,
            _ => return Ok(None),
        };

        Ok(Some(EvaluatedConstantCall::new(
            value,
            ConstantEvaluationUsage::default(),
        )))
    }
}

fn checker_constant_query_error(error: CheckerQueryError) -> FactQueryError {
    error.into()
}

