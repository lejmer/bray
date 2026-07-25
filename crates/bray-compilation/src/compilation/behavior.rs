use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_bound_tree::{
    BodyBehaviorCall, BodyBehaviorCallKind, BodyBehaviorContributions, BodyBehaviorPhase,
    BoundCallableTarget, BoundUnitKey, CheckedBodyBehavior,
};
use bray_checker::{
    BodyBehaviorCollector, CheckerInfrastructureError, CheckerUnitView,
    DefaultBodyBehaviorCollector,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableCapabilityRequirement, CallableContractsFact, CallableEffectRequirement,
    CallableExecution, CallableExecutionRequirement, CallableParameterDefaultFact,
    CallableParameterDefaultTemplateFact, CallableParameterDefaultValue, CallablePhaseBehavior,
    CallableSymbolId, CurrentRunCancellation, LifecycleObligationKind, RuntimeDefaultBehavior,
    StructFieldDefaultFact, StructFieldDefaultTemplateFact, StructFieldDefaultValue,
    SymbolFactRequest, TypeData, TypeExpressionTemplate, UnevaluatedDefaultTemplate,
    UnionPayloadDefaultValue, UnionPayloadFieldDefaultFact, UnionPayloadFieldDefaultTemplateFact,
};

use super::binder::{CompilationBinderFacts, bind_declared_trusted_capabilities};
use super::checker::checker_result;
use super::facts::Compilation;
use super::unit::semantic_unit_context_for;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitFact};

#[derive(Default)]
struct BodyBehaviorBuilder {
    effects: BTreeSet<CallableEffectRequirement>,
    capabilities: BTreeSet<CallableCapabilityRequirement>,
    trusted_capabilities: BTreeSet<AnySymbolId>,
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

        self.trusted_capabilities.extend(
            behavior
                .trusted_capabilities()
                .iter()
                .map(|capability| capability.capability()),
        );

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

        self.trusted_capabilities.extend(
            behavior
                .trusted_obligations()
                .iter()
                .map(|obligation| obligation.declaration()),
        );

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

        CheckedBodyBehavior::new(unit, key.kind(), cancellation, self.is_recovered)
            .with_requirements(
                self.effects,
                self.capabilities,
                self.trusted_capabilities,
                self.execution_requirements,
                self.lifecycle_obligations,
            )
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
    ) -> Result<Arc<PublishedUnitFact<CheckedBodyBehavior>>, FactQueryError> {
        self.unit_fact(
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

    fn body_behavior_contributions_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitFact<BodyBehaviorContributions>>, FactQueryError> {
        self.unit_fact(
            &self.state.body_behavior_contributions,
            CompilationFactKey::BodyBehaviorContributions(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;
                let control = self.control_flow_with_cancellation(key.clone(), cancellation)?;

                let selections =
                    self.semantic_selections_with_cancellation(key.clone(), cancellation)?;

                let semantic_context =
                    semantic_unit_context_for(self.symbol_graph()?, bound.result().value())?;

                let context = self.checker_context_for(&key, cancellation)?;

                let request =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                        .map_err(|error| {
                            FactQueryError::CheckerInfrastructure(
                                CheckerInfrastructureError::InvalidUnitView(error),
                            )
                        })?;

                let result = checker_result(DefaultBodyBehaviorCollector.collect_body_behavior(
                    request,
                    control.result().value(),
                    selections.result().value(),
                ))?;

                let (contributions, contribution_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    control.result().diagnostics(),
                    selections.result().diagnostics(),
                    &contribution_diagnostics,
                ]);

                Ok((
                    DiagnosticResult::new(contributions, diagnostics),
                    Box::new([]),
                ))
            },
        )
    }

    pub(in crate::compilation) fn direct_trusted_capability_use_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<(BTreeSet<AnySymbolId>, DiagnosticBag, bool), FactQueryError> {
        let contributions =
            self.body_behavior_contributions_with_cancellation(key, cancellation)?;

        let mut capabilities = BTreeSet::new();
        let mut diagnostics = contributions.result().diagnostics().clone();
        let mut is_recovered = contributions.result().value().is_recovered();
        let facts = self.binder_facts(cancellation)?;

        for call in contributions.result().value().calls() {
            if call.kind() != BodyBehaviorCallKind::SourceCall
                || call.phase() != BodyBehaviorPhase::Invocation
            {
                continue;
            }

            let BoundCallableTarget::Declaration(instance) = call.target() else {
                continue;
            };

            let bray_symbols::CallableSymbolId::Function(function) =
                instance.definition().callable_symbol()
            else {
                continue;
            };

            let foreign =
                self.foreign_callable_contract_with_cancellation(function, cancellation)?;

            diagnostics = diagnostics.merged(foreign.diagnostics());

            if foreign.value().is_none() {
                continue;
            }

            let declared =
                bind_declared_trusted_capabilities(&facts, CallableSymbolId::Function(function))
                    .map_err(binder_fact_error)?;

            diagnostics = diagnostics.merged(declared.diagnostics());
            is_recovered |= declared.diagnostics().has_errors();

            capabilities.extend(
                declared
                    .value()
                    .iter()
                    .map(|requirement| requirement.capability()),
            );
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
        let facts = self.binder_facts_for(root, cancellation)?;

        while let Some(key) = pending.pop() {
            cancellation.check()?;

            if !visited.insert(key.clone()) {
                continue;
            }

            let contributions =
                self.body_behavior_contributions_with_cancellation(key, cancellation)?;

            diagnostics = diagnostics.merged(contributions.result().diagnostics());

            builder.merge_contributions(contributions.result().value());

            for call in contributions.result().value().calls() {
                self.merge_call_behavior(
                    &facts,
                    call,
                    &mut pending,
                    &mut builder,
                    &mut diagnostics,
                )?;
            }

            for provider in contributions.result().value().defaults() {
                self.merge_default_behavior(
                    &facts,
                    *provider,
                    &mut pending,
                    &mut builder,
                    &mut diagnostics,
                )?;
            }
        }

        Ok((
            builder.finish(root, root_bound.result().value().unit()),
            diagnostics,
        ))
    }

    fn merge_call_behavior(
        &self,
        facts: &CompilationBinderFacts<'_>,
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
                    let execution = callable_execution(facts, callable)?;

                    if phase_executes_body(call.phase(), execution) {
                        let capabilities = bind_declared_trusted_capabilities(facts, callable)
                            .map_err(binder_fact_error)?;

                        *diagnostics = diagnostics.merged(capabilities.diagnostics());

                        builder.trusted_capabilities.extend(
                            capabilities
                                .value()
                                .iter()
                                .map(|capability| capability.capability()),
                        );

                        pending.push(body);
                    }

                    return Ok(());
                }

                let contract = facts
                    .symbol_fact(SymbolFactRequest::<CallableContractsFact>::new(
                        instance.definition().callable_symbol(),
                    ))
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(contract.diagnostics());

                if let Some(phase) = phase_behavior(contract.value(), call.phase()) {
                    builder.merge_phase(phase);
                }
            }
            BoundCallableTarget::Anonymous(_) => match call.anonymous_unit() {
                Some(unit) => pending.push(unit.clone()),
                None => builder.is_recovered = true,
            },
            BoundCallableTarget::Indirect(_) => {
                // TODO(BRA-267): Merge phase behavior carried by indirect callable contracts.
                builder.is_recovered = true;
            }
        }

        Ok(())
    }

    fn merge_default_behavior(
        &self,
        facts: &CompilationBinderFacts<'_>,
        provider: bray_bound_tree::ConstructionDefaultProvider,
        pending: &mut Vec<BoundUnitKey>,
        builder: &mut BodyBehaviorBuilder,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<(), FactQueryError> {
        let symbols = facts.symbols();

        match provider {
            bray_bound_tree::ConstructionDefaultProvider::CallableParameter(provider) => {
                let owner = symbols
                    .callable_parameter_default_provider(provider)
                    .map(bray_symbols::CallableParameterDefaultProviderSymbol::subject)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let template = facts
                    .symbol_fact(
                        SymbolFactRequest::<CallableParameterDefaultTemplateFact>::new(owner),
                    )
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(template.diagnostics());

                if self.queue_source_default(provider.into(), template.value(), pending, builder)? {
                    return Ok(());
                }

                let default = facts
                    .symbol_fact(SymbolFactRequest::<CallableParameterDefaultFact>::new(
                        owner,
                    ))
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(default.diagnostics());

                match default.value().value() {
                    CallableParameterDefaultValue::Valid(surface) => {
                        builder.merge_default(surface.behavior());
                    }
                    CallableParameterDefaultValue::Error(_) => builder.is_recovered = true,
                }
            }
            bray_bound_tree::ConstructionDefaultProvider::StructField(provider) => {
                let owner = symbols
                    .struct_field_default_provider(provider)
                    .map(bray_symbols::StructFieldDefaultProviderSymbol::subject)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let template = facts
                    .symbol_fact(SymbolFactRequest::<StructFieldDefaultTemplateFact>::new(
                        owner,
                    ))
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(template.diagnostics());

                if self.queue_source_default(provider.into(), template.value(), pending, builder)? {
                    return Ok(());
                }

                let default = facts
                    .symbol_fact(SymbolFactRequest::<StructFieldDefaultFact>::new(owner))
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(default.diagnostics());

                match default.value().value() {
                    StructFieldDefaultValue::Valid(surface) => {
                        builder.merge_default(surface.behavior());
                    }
                    StructFieldDefaultValue::Error(_) => builder.is_recovered = true,
                }
            }
            bray_bound_tree::ConstructionDefaultProvider::UnionPayload(provider) => {
                let owner = symbols
                    .union_payload_default_provider(provider)
                    .map(bray_symbols::UnionPayloadDefaultProviderSymbol::subject)
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let template = facts
                    .symbol_fact(
                        SymbolFactRequest::<UnionPayloadFieldDefaultTemplateFact>::new(owner),
                    )
                    .map_err(binder_fact_error)?;

                *diagnostics = diagnostics.merged(template.diagnostics());

                if self.queue_source_default(provider.into(), template.value(), pending, builder)? {
                    return Ok(());
                }

                let default = facts
                    .symbol_fact(SymbolFactRequest::<UnionPayloadFieldDefaultFact>::new(
                        owner,
                    ))
                    .map_err(binder_fact_error)?;

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

fn callable_execution(
    facts: &CompilationBinderFacts<'_>,
    callable: bray_symbols::CallableSymbolId,
) -> Result<CallableExecution, FactQueryError> {
    let signature = facts
        .symbol_fact(SymbolFactRequest::<bray_symbols::CallableSignatureFact>::new(callable))
        .map_err(binder_fact_error)?;

    match signature.value().callable_type() {
        TypeExpressionTemplate::Callable(callable) => Ok(callable.execution()),
        TypeExpressionTemplate::Resolved(ty) => {
            let data = facts
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
    match phase {
        BodyBehaviorPhase::Invocation => Some(contract.invocation_behavior()),
        BodyBehaviorPhase::DeferredExecution => contract.deferred_execution_behavior(),
    }
}

const fn binder_fact_error(error: bray_binder::BinderFactError) -> FactQueryError {
    match error {
        bray_binder::BinderFactError::Cancelled => FactQueryError::Cancelled,
        bray_binder::BinderFactError::DependencyUnavailable => {
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
    fn source_runtime_defaults_participate_without_eager_default_facts() {
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
    fn recursive_runtime_defaults_converge_without_fact_cycles() {
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
    fn source_calls_propagate_exact_trusted_capability_identities() {
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
            .unwrap_or_else(|error| panic!("called behavior must propagate: {error:?}"));

        let [capability] = behavior.value().trusted_capabilities() else {
            panic!("called trusted capability must propagate");
        };

        assert_eq!(
            compilation
                .symbol_graph()
                .unwrap_or_else(|error| panic!("symbols must be available: {error:?}"))
                .member_name(*capability)
                .map(bray_symbols::SymbolName::as_str),
            Some("foreign_call")
        );
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

        assert_eq!(direct_await.value().trusted_capabilities().len(), 1);
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

        assert_eq!(direct_await.value().trusted_capabilities().len(), 1);
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
