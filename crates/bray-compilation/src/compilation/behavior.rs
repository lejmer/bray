use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    BodyBehaviorCall, BodyBehaviorContributions, BodyBehaviorPhase, BoundCallableTarget,
    BoundSourceAnchor, BoundUnitKey, CheckedBodyBehavior, TrustedCapabilityUse,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableCapabilityRequirement, CallableContractsQuery, CallableEffectRequirement,
    CallableExecution, CallableExecutionRequirement, CallableParameterDefaultQuery,
    CallableParameterDefaultTemplateQuery, CallableParameterDefaultValue, CallablePhaseBehavior,
    CallableSymbolId, CurrentRunCancellation, LifecycleObligationKind, RuntimeDefaultBehavior,
    StructFieldDefaultQuery, StructFieldDefaultTemplateQuery, StructFieldDefaultValue,
    SymbolOrigin, SymbolQueryRequest, TrustedCapabilitySymbolId, TypeData, TypeExpressionTemplate,
    UnevaluatedDefaultTemplate, UnionPayloadDefaultValue, UnionPayloadFieldDefaultQuery,
    UnionPayloadFieldDefaultTemplateQuery,
};

use super::binder::{CompilationBindingContext, bind_declared_trusted_capabilities};
use super::state::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

#[derive(Default)]
struct BodyBehaviorBuilder {
    effects: BTreeSet<CallableEffectRequirement>,
    capabilities: BTreeSet<CallableCapabilityRequirement>,
    trusted_capabilities: BTreeMap<TrustedCapabilitySymbolId, BTreeSet<BoundSourceAnchor>>,
    execution_requirements: BTreeSet<CallableExecutionRequirement>,
    lifecycle_obligations: BTreeSet<LifecycleObligationKind>,
    current_run_cancellation: bool,
    is_recovered: bool,
}

impl BodyBehaviorBuilder {
    fn merge_phase(&mut self, behavior: &CallablePhaseBehavior) {
        self.effects.extend(behavior.effects().iter().copied());

        self.capabilities
            .extend(behavior.capabilities().iter().copied());

        self.execution_requirements
            .extend(behavior.execution_requirements().iter().copied());

        self.lifecycle_obligations
            .extend(behavior.lifecycle_obligations().iter().copied());

        self.merge_cancellation(behavior.current_run_cancellation());
    }

    fn merge_default(&mut self, behavior: &RuntimeDefaultBehavior) {
        self.effects.extend(
            behavior
                .effects()
                .iter()
                .map(|requirement| CallableEffectRequirement::new(requirement.declaration())),
        );

        self.capabilities.extend(
            behavior
                .capabilities()
                .iter()
                .map(|requirement| CallableCapabilityRequirement::new(requirement.declaration())),
        );

        for obligation in behavior.trusted_obligations() {
            self.trusted_capabilities
                .entry(obligation.declaration())
                .or_default();
        }

        self.lifecycle_obligations
            .extend(behavior.lifecycle_obligations().iter().copied());
    }

    fn merge_contributions(&mut self, contributions: &BodyBehaviorContributions) {
        self.merge_cancellation(contributions.current_run_cancellation());
        self.is_recovered |= contributions.is_recovered();
    }

    fn merge_cancellation(&mut self, cancellation: CurrentRunCancellation) {
        self.current_run_cancellation |= cancellation == CurrentRunCancellation::MayEnter;
    }

    fn finish(self, key: &BoundUnitKey, unit: bray_bound_tree::BoundUnitId) -> CheckedBodyBehavior {
        let cancellation = if self.current_run_cancellation {
            CurrentRunCancellation::MayEnter
        } else {
            CurrentRunCancellation::NotEntered
        };

        let capability_uses = self
            .trusted_capabilities
            .iter()
            .map(|(capability, sources)| {
                TrustedCapabilityUse::new(*capability, sources.iter().copied())
            })
            .collect::<Vec<_>>();

        let trusted_capabilities = self
            .trusted_capabilities
            .keys()
            .copied()
            .collect::<Vec<_>>();

        CheckedBodyBehavior::new(unit, key.kind(), cancellation, self.is_recovered)
            .with_requirements(
                self.effects,
                self.capabilities,
                trusted_capabilities,
                self.execution_requirements,
                self.lifecycle_obligations,
            )
            .with_trusted_capability_uses(capability_uses)
    }
}

impl Compilation {
    /// Returns normalized effects and obligations for one checked semantic body.
    pub fn body_behavior(
        &self,
        key: BoundUnitKey,
    ) -> Result<Arc<DiagnosticResult<CheckedBodyBehavior>>, FactQueryError> {
        let published = self.body_behavior_with_cancellation(key, &self.state.cancellation)?;

        Ok(Arc::clone(published.result()))
    }

    pub(in crate::compilation) fn body_behavior_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedBodyBehavior>>, FactQueryError> {
        self.unit_query(
            &self.state.checked_body_behaviors,
            CompilationFactKey::CheckedBodyBehavior(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let (behavior, diagnostics) =
                    self.compute_reachable_body_behavior(&key, cancellation)?;

                Ok((DiagnosticResult::new(behavior, diagnostics), Box::new([])))
            },
        )
    }

    fn direct_trusted_capability_use_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<
        (
            BTreeMap<TrustedCapabilitySymbolId, BTreeSet<BoundSourceAnchor>>,
            DiagnosticBag,
            bool,
        ),
        FactQueryError,
    > {
        let contributions = self.body_semantics_with_cancellation(key, cancellation)?;

        let mut capabilities = BTreeMap::<_, BTreeSet<_>>::new();
        let mut diagnostics = contributions.result().diagnostics().clone();
        let mut is_recovered = contributions.result().value().behavior().is_recovered();
        let binding_context = self.binding_context(cancellation)?;
        let symbols = self.symbol_graph()?;

        for call in contributions.result().value().behavior().calls() {
            let BoundCallableTarget::Declaration(instance) = call.target() else {
                continue;
            };

            let callable = instance.definition().callable_symbol();

            let imported_symbols = binding_context
                .imported_symbols()
                .map_err(binding_query_error)?;

            let origin = symbols.callable_origin(callable).or_else(|| {
                imported_symbols
                    .and_then(|symbols| symbols.symbol_key(callable.into_any()))
                    .map(|_| SymbolOrigin::Imported)
            });

            match origin {
                Some(SymbolOrigin::Source) => {
                    let CallableSymbolId::Function(function) = callable else {
                        continue;
                    };

                    let foreign =
                        self.foreign_callable_contract_with_cancellation(function, cancellation)?;

                    diagnostics = diagnostics.merged(foreign.diagnostics());

                    if foreign.value().is_none() {
                        continue;
                    }

                    let declared = bind_declared_trusted_capabilities(
                        &binding_context,
                        CallableSymbolId::Function(function),
                    )
                    .map_err(binding_query_error)?;

                    diagnostics = diagnostics.merged(declared.diagnostics());
                    is_recovered |= declared.diagnostics().has_errors();

                    record_trusted_capability_use(
                        &mut capabilities,
                        call.source(),
                        declared
                            .value()
                            .iter()
                            .map(|capability| capability.requirement().capability()),
                    );
                }
                Some(
                    SymbolOrigin::CompilerKnown
                    | SymbolOrigin::CompilerProvided
                    | SymbolOrigin::Imported,
                ) => {
                    let contract = binding_context
                        .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                            callable,
                        ))
                        .map_err(binding_query_error)?;

                    diagnostics = diagnostics.merged(contract.diagnostics());
                    is_recovered |= contract.diagnostics().has_errors();

                    if let Some(behavior) = phase_behavior(contract.value(), call.phase()) {
                        record_trusted_capability_use(
                            &mut capabilities,
                            call.source(),
                            behavior
                                .trusted_capabilities()
                                .iter()
                                .map(|requirement| requirement.capability()),
                        );
                    }
                }
                Some(SymbolOrigin::Synthesized) => {}
                None => return Err(FactQueryError::InfrastructureFailure),
            }
        }

        Ok((capabilities, diagnostics, is_recovered))
    }

    fn compute_reachable_body_behavior(
        &self,
        root: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<(CheckedBodyBehavior, DiagnosticBag), FactQueryError> {
        let root_bound = self.bound_unit_with_cancellation(root.clone(), cancellation)?;
        let mut pending = vec![root.clone()];
        let mut visited = BTreeSet::new();
        let mut builder = BodyBehaviorBuilder::default();
        let mut diagnostics = DiagnosticBag::new();
        let binding_context = self.binding_context_for(root, cancellation)?;

        while let Some(key) = pending.pop() {
            cancellation.check()?;

            if !visited.insert(key.clone()) {
                continue;
            }

            let contributions = self.body_semantics_with_cancellation(key.clone(), cancellation)?;
            let behavior = contributions.result().value().behavior();

            diagnostics = diagnostics.merged(contributions.result().diagnostics());

            builder.merge_contributions(behavior);

            for call in behavior.calls() {
                self.merge_call_behavior(
                    &binding_context,
                    call,
                    &mut pending,
                    &mut builder,
                    &mut diagnostics,
                )?;
            }

            for provider in behavior.defaults() {
                self.merge_default_behavior(
                    &binding_context,
                    *provider,
                    &mut pending,
                    &mut builder,
                    &mut diagnostics,
                )?;
            }
        }

        let (trusted_capabilities, capability_diagnostics, capability_recovered) =
            self.direct_trusted_capability_use_with_cancellation(root.clone(), cancellation)?;

        diagnostics = diagnostics.merged(&capability_diagnostics);
        builder.trusted_capabilities = trusted_capabilities;
        builder.is_recovered |= capability_recovered;

        Ok((
            builder.finish(root, root_bound.result().value().unit()),
            diagnostics,
        ))
    }

    fn merge_call_behavior(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        call: &BodyBehaviorCall,
        pending: &mut Vec<BoundUnitKey>,
        builder: &mut BodyBehaviorBuilder,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        match call.target() {
            BoundCallableTarget::Declaration(instance) => {
                let body = self.callable_body_key(instance.definition())?;

                if let Some(body) = body {
                    let callable = instance.definition().callable_symbol();
                    let execution = callable_execution(binding_context, callable)?;

                    if phase_executes_body(call.phase(), execution) {
                        pending.push(body);
                    }

                    return Ok(());
                }

                let contract = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableContractsQuery>::new(
                        instance.definition().callable_symbol(),
                    ))
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(contract.diagnostics());

                if let Some(phase) = phase_behavior(contract.value(), call.phase()) {
                    builder.merge_phase(phase);
                }
            }
            BoundCallableTarget::Predicate(_) => {}
            BoundCallableTarget::Anonymous(_) => match call.anonymous_unit() {
                Some(unit) => pending.push(unit.clone()),
                None => builder.is_recovered = true,
            },
            BoundCallableTarget::Indirect(ty) => {
                let data = binding_context
                    .semantic_values()
                    .type_data(ty)
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                let TypeData::Callable(callable) = data.as_ref() else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                if let Some(phase) = phase_behavior_for(callable.phase_behaviors(), call.phase()) {
                    builder.merge_phase(phase);
                }
            }
        }

        Ok(())
    }

    fn merge_default_behavior(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        provider: bray_bound_tree::ConstructionDefaultProvider,
        pending: &mut Vec<BoundUnitKey>,
        builder: &mut BodyBehaviorBuilder,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        match provider {
            bray_bound_tree::ConstructionDefaultProvider::CallableParameter(provider) => {
                let Some(AnySymbolId::CallableParameter(owner)) = binding_context
                    .runtime_default_subject(provider.into())
                    .map_err(binding_query_error)?
                else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let template = binding_context
                    .resolve_symbol_query(
                        SymbolQueryRequest::<CallableParameterDefaultTemplateQuery>::new(owner),
                    )
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(template.diagnostics());

                if self.queue_source_default(provider.into(), template.value(), pending, builder)? {
                    return Ok(());
                }

                let default = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<CallableParameterDefaultQuery>::new(
                        owner,
                    ))
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(default.diagnostics());

                match default.value().value() {
                    CallableParameterDefaultValue::Valid(surface) => {
                        builder.merge_default(surface.behavior());
                    }
                    CallableParameterDefaultValue::Error(_) => builder.is_recovered = true,
                }
            }
            bray_bound_tree::ConstructionDefaultProvider::StructField(provider) => {
                let Some(AnySymbolId::StructField(owner)) = binding_context
                    .runtime_default_subject(provider.into())
                    .map_err(binding_query_error)?
                else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let template = binding_context
                    .resolve_symbol_query(
                        SymbolQueryRequest::<StructFieldDefaultTemplateQuery>::new(owner),
                    )
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(template.diagnostics());

                if self.queue_source_default(provider.into(), template.value(), pending, builder)? {
                    return Ok(());
                }

                let default = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<StructFieldDefaultQuery>::new(owner))
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(default.diagnostics());

                match default.value().value() {
                    StructFieldDefaultValue::Valid(surface) => {
                        builder.merge_default(surface.behavior());
                    }
                    StructFieldDefaultValue::Error(_) => builder.is_recovered = true,
                }
            }
            bray_bound_tree::ConstructionDefaultProvider::UnionPayload(provider) => {
                let Some(AnySymbolId::UnionPayloadField(owner)) = binding_context
                    .runtime_default_subject(provider.into())
                    .map_err(binding_query_error)?
                else {
                    return Err(FactQueryError::InfrastructureFailure);
                };

                let template = binding_context
                    .resolve_symbol_query(
                        SymbolQueryRequest::<UnionPayloadFieldDefaultTemplateQuery>::new(owner),
                    )
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(template.diagnostics());

                if self.queue_source_default(provider.into(), template.value(), pending, builder)? {
                    return Ok(());
                }

                let default = binding_context
                    .resolve_symbol_query(SymbolQueryRequest::<UnionPayloadFieldDefaultQuery>::new(
                        owner,
                    ))
                    .map_err(binding_query_error)?;

                *diagnostics = diagnostics.merged(default.diagnostics());

                match default.value().value() {
                    UnionPayloadDefaultValue::Valid(surface) => {
                        builder.merge_default(surface.behavior());
                    }
                    UnionPayloadDefaultValue::Error(_) => builder.is_recovered = true,
                }
            }
        }

        Ok(())
    }

    fn queue_source_default(
        &self,
        provider: AnySymbolId,
        template: &UnevaluatedDefaultTemplate,
        pending: &mut Vec<BoundUnitKey>,
        builder: &mut BodyBehaviorBuilder,
    ) -> Result<bool, FactQueryError> {
        match template {
            UnevaluatedDefaultTemplate::Present(expression) => {
                pending.push(self.source_runtime_default_key(provider, expression.syntax())?);

                Ok(true)
            }
            UnevaluatedDefaultTemplate::Absent => {
                builder.is_recovered = true;

                Ok(true)
            }
            UnevaluatedDefaultTemplate::Resolved => Ok(false),
        }
    }
}

fn record_trusted_capability_use(
    capabilities: &mut BTreeMap<TrustedCapabilitySymbolId, BTreeSet<BoundSourceAnchor>>,
    source: Option<BoundSourceAnchor>,
    used: impl IntoIterator<Item = TrustedCapabilitySymbolId>,
) {
    for capability in used {
        let origins = capabilities.entry(capability).or_default();

        if let Some(source) = source {
            origins.insert(source);
        }
    }
}

fn callable_execution(
    binding_context: &CompilationBindingContext<'_>,
    callable: CallableSymbolId,
) -> Result<CallableExecution, FactQueryError> {
    let signature = binding_context
        .resolve_symbol_query(
            SymbolQueryRequest::<bray_symbols::CallableSignatureQuery>::new(callable),
        )
        .map_err(binding_query_error)?;

    match signature.value().callable_type() {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.execution()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = binding_context
                .semantic_values()
                .type_data(*ty)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            match data.as_ref() {
                TypeData::Callable(callable) => Ok(callable.execution()),
                _ => Err(FactQueryError::InfrastructureFailure),
            }
        }
        _ => Err(FactQueryError::InfrastructureFailure),
    }
}

const fn phase_executes_body(phase: BodyBehaviorPhase, execution: CallableExecution) -> bool {
    matches!(
        (phase, execution),
        (
            BodyBehaviorPhase::Invocation,
            CallableExecution::Synchronous
        ) | (
            BodyBehaviorPhase::DeferredExecution,
            CallableExecution::Asynchronous
        )
    )
}

fn phase_behavior(
    contract: &bray_symbols::CallableContractSet,
    phase: BodyBehaviorPhase,
) -> Option<&CallablePhaseBehavior> {
    phase_behavior_for(contract.phase_behaviors(), phase)
}

fn phase_behavior_for(
    behaviors: &bray_symbols::CallablePhaseBehaviors,
    phase: BodyBehaviorPhase,
) -> Option<&CallablePhaseBehavior> {
    match phase {
        BodyBehaviorPhase::Invocation => Some(behaviors.invocation()),
        BodyBehaviorPhase::DeferredExecution => behaviors.deferred_execution(),
    }
}

const fn binding_query_error(error: bray_binder::BindingQueryError) -> FactQueryError {
    match error {
        bray_binder::BindingQueryError::Cancelled => FactQueryError::Cancelled,
        bray_binder::BindingQueryError::DependencyUnavailable => {
            FactQueryError::InfrastructureFailure
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::Compilation;
    use crate::test_support::{compilation, source_callable_body_key};

    #[test]
    fn body_behavior_is_demanded_lazily_and_published_once() {
        let compilation = callable_compilation("");
        let key = source_callable_body_key(&compilation);

        assert_eq!(
            compilation.state.checked_body_behaviors.is_published(&key),
            Ok(false)
        );

        let first = compilation
            .body_behavior(key.clone())
            .unwrap_or_else(|error| panic!("body behavior must be available: {error:?}"));

        let second = compilation
            .body_behavior(key.clone())
            .unwrap_or_else(|error| panic!("body behavior must remain available: {error:?}"));

        assert!(Arc::ptr_eq(&first, &second));

        assert_eq!(
            compilation.state.checked_body_behaviors.is_published(&key),
            Ok(true)
        );
    }

    #[test]
    fn recursive_calls_converge_to_one_body_behavior_summary() {
        let compilation = callable_compilation("    recurse();\n");
        let key = source_callable_body_key(&compilation);

        let behavior = compilation
            .body_behavior(key)
            .unwrap_or_else(|error| panic!("recursive behavior must converge: {error:?}"));

        assert!(behavior.diagnostics().is_empty());
        assert!(behavior.value().effects().is_empty());
        assert!(!behavior.value().is_recovered());
    }

    #[test]
    fn source_runtime_defaults_participate_without_eager_default_queries() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func invoke()\n",
            "{\n",
            "    configured();\n",
            "}\n",
            "func configured(value: i32 = default_value())\n",
            "{\n",
            "}\n",
            "func default_value() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let behavior = compilation
            .body_behavior(key)
            .unwrap_or_else(|error| panic!("source defaults must participate: {error:?}"));

        assert!(behavior.diagnostics().is_empty());
        assert!(!behavior.value().is_recovered());
    }

    #[test]
    fn indirect_calls_use_the_callable_type_contract_without_recovery() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func apply(op: func(pos value: i32) -> i32, value: i32) -> i32\n",
            "{\n",
            "    return op(value);\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let behavior = compilation
            .body_behavior(key)
            .unwrap_or_else(|error| panic!("indirect behavior must summarize: {error:?}"));

        assert!(behavior.diagnostics().is_empty());
        assert!(!behavior.value().is_recovered());
    }

    #[test]
    fn recursive_runtime_defaults_converge_without_query_cycles() {
        let compilation = compilation(concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    recurse();\n",
            "}\n",
            "func recurse(value: i32 = recurse()) -> i32\n",
            "{\n",
            "    return value;\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let behavior = compilation
            .body_behavior(key)
            .unwrap_or_else(|error| panic!("recursive defaults must converge: {error:?}"));

        assert!(behavior.diagnostics().is_empty());
        assert!(!behavior.value().is_recovered());
    }

    #[test]
    fn source_calls_do_not_propagate_implementation_capabilities() {
        let compilation = compilation(concat!(
            "trusted module app;\n",
            "trusted func outer()\n",
            "{\n",
            "    inner();\n",
            "}\n",
            "trusted func inner()\n",
            "    uses(foreign_call)\n",
            "{\n",
            "}\n",
        ));

        let key = source_callable_body_key(&compilation);

        let behavior = compilation
            .body_behavior(key)
            .unwrap_or_else(|error| panic!("called behavior must summarize: {error:?}"));

        assert!(behavior.value().trusted_capabilities().is_empty());
    }

    #[test]
    fn async_body_behavior_is_deferred_until_direct_await() {
        let invoked = async_caller_compilation("    delayed();\n");
        let invoked_key = source_callable_body_key(&invoked);

        let invocation = invoked
            .body_behavior(invoked_key)
            .unwrap_or_else(|error| panic!("async invocation must summarize: {error:?}"));

        assert!(invocation.value().trusted_capabilities().is_empty());

        let awaited = async_caller_compilation("    await delayed();\n");
        let awaited_key = source_callable_body_key(&awaited);

        let direct_await = awaited
            .body_behavior(awaited_key)
            .unwrap_or_else(|error| panic!("direct await must summarize: {error:?}"));

        assert!(direct_await.value().trusted_capabilities().is_empty());
    }

    #[test]
    fn async_anonymous_body_behavior_is_deferred_until_direct_await() {
        let invoked = async_anonymous_caller_compilation(false);
        let invoked_key = source_callable_body_key(&invoked);

        let invocation = invoked
            .body_behavior(invoked_key)
            .unwrap_or_else(|error| panic!("async lambda invocation must summarize: {error:?}"));

        assert!(invocation.value().trusted_capabilities().is_empty());

        let awaited = async_anonymous_caller_compilation(true);
        let awaited_key = source_callable_body_key(&awaited);

        let direct_await = awaited
            .body_behavior(awaited_key)
            .unwrap_or_else(|error| panic!("direct lambda await must summarize: {error:?}"));

        assert!(direct_await.value().trusted_capabilities().is_empty());
    }

    fn callable_compilation(body: &str) -> Compilation {
        compilation(&format!("module app;\nfunc recurse()\n{{\n{body}}}\n"))
    }

    fn async_caller_compilation(body: &str) -> Compilation {
        compilation(&format!(
            concat!(
                "trusted module app;\n",
                "trusted async func caller()\n",
                "{{\n",
                "{body}",
                "}}\n",
                "trusted async func delayed()\n",
                "    uses(foreign_call)\n",
                "{{\n",
                "}}\n",
            ),
            body = body,
        ))
    }

    fn async_anonymous_caller_compilation(awaited: bool) -> Compilation {
        let expression = if awaited {
            "    await trusted async lambda()\n"
        } else {
            "    trusted async lambda()\n"
        };

        compilation(&format!(
            concat!(
                "trusted module app;\n",
                "trusted async func caller()\n",
                "{{\n",
                "{expression}",
                "    {{\n",
                "        await delayed();\n",
                "    }}();\n",
                "}}\n",
                "trusted async func delayed()\n",
                "    uses(foreign_call)\n",
                "{{\n",
                "}}\n",
            ),
            expression = expression,
        ))
    }
}
