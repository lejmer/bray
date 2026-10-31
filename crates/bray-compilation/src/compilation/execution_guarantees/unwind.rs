use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::{SymbolQueryProvider, semantic_unit_context};
use bray_bound_tree::{BoundCallableTarget, BoundUnitKey, UnwindBehavior};
use bray_checker::{CheckerUnitView, UnwindCandidate, check_unwind_candidate};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use crate::compilation::{Compilation, checker::checker_result};
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    fn unwind_candidate(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<UnwindCandidate>>, FactQueryError> {
        self.unit_query(
            &self.state.unwind_candidates,
            CompilationFactKey::UnwindCandidate(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let prepared =
                    self.prepared_body_analysis_with_cancellation(key.clone(), cancellation)?;

                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let static_dependencies = self.static_dependency_roots(
                    &key,
                    expressions.result().value().selections(),
                    cancellation,
                )?;

                let request =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context)
                        .with_static_dependencies(&static_dependencies);

                let (entry, entry_diagnostics) = self
                    .unwind_entry_conditions(&key, cancellation)?
                    .into_parts();

                let result = checker_result(check_unwind_candidate(
                    request,
                    prepared.result().value(),
                    expressions.result().value(),
                    storage.result().value(),
                    memory.result().value(),
                    entry,
                ))?;

                let (candidate, diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    &entry_diagnostics,
                    &diagnostics,
                    bound.result().diagnostics(),
                    expressions.result().diagnostics(),
                    prepared.result().diagnostics(),
                    storage.result().diagnostics(),
                    memory.result().diagnostics(),
                ]);

                Ok((DiagnosticResult::new(candidate, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn unwind_behavior(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<UnwindBehavior>>, FactQueryError> {
        self.unit_query(
            &self.state.unwind_behavior,
            CompilationFactKey::UnwindBehavior(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let mut pending = vec![key.clone()];
                let mut visited = BTreeSet::new();
                let mut diagnostics = DiagnosticBag::new();
                let mut behavior = UnwindBehavior::NONE;

                while let Some(key) = pending.pop() {
                    cancellation.check()?;

                    if !visited.insert(key.clone()) {
                        continue;
                    }

                    let candidate = self.unwind_candidate(key.clone(), cancellation)?;

                    diagnostics = diagnostics.merged(candidate.result().diagnostics());

                    behavior = behavior.merge(if candidate.result().diagnostics().has_errors() {
                        UnwindBehavior::ANY
                    } else {
                        candidate.result().value().behavior()
                    });

                    if behavior == UnwindBehavior::ANY {
                        break;
                    }

                    for target in candidate.result().value().dependencies() {
                        behavior = behavior.merge(self.queue_unwind_dependency(
                            *target,
                            candidate.result().value().entry_conditions(),
                            &mut pending,
                            cancellation,
                        )?);
                    }
                }

                Ok((DiagnosticResult::new(behavior, diagnostics), Box::new([])))
            },
        )
    }

    fn unwind_entry_conditions(
        &self,
        key: &BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticResult<BTreeSet<bray_checker::ExecutionCondition>>, FactQueryError> {
        if !matches!(
            key.kind(),
            bray_bound_tree::BoundUnitKind::CallableBody
                | bray_bound_tree::BoundUnitKind::AnonymousCallable
        ) {
            return Ok(DiagnosticResult::new(BTreeSet::new(), DiagnosticBag::new()));
        }

        let declared = self.execution_unit_declaration(key)?;

        let inputs =
            self.predicate_condition_inputs(key, declared.value().requirements(), cancellation)?;

        let (inputs, diagnostics) = inputs.into_parts();

        let entry = inputs
            .into_iter()
            .filter_map(|(condition, domain, _)| {
                let bray_checker::ExecutionCondition::Predicate(_, _, arguments) = &condition
                else {
                    return None;
                };

                (domain == bray_symbols::PredicateDomain::Execution && arguments.is_empty())
                    .then_some(condition)
            })
            .collect();

        Ok(DiagnosticResult::new(entry, diagnostics))
    }

    fn queue_unwind_dependency(
        &self,
        target: bray_checker::ExecutionDependencyTarget,
        entry: &BTreeSet<bray_checker::ExecutionCondition>,
        pending: &mut Vec<BoundUnitKey>,
        cancellation: &CancellationToken,
    ) -> Result<UnwindBehavior, FactQueryError> {
        let target = match target {
            bray_checker::ExecutionDependencyTarget::Callable(target) => target,
            bray_checker::ExecutionDependencyTarget::Requirement { callable, dispatch } => {
                let binding = self.binding_context(cancellation)?;

                let requirement =
                    crate::compilation::implementation::dispatch_requirement(&binding, dispatch)
                        .map_err(FactQueryError::from)?;

                let callable = self
                    .semantic_value_store()?
                    .intern_callable_instance(callable)?;

                let Some((callable, _, _)) = crate::compilation::implementation::witness_callable(
                    &binding,
                    callable,
                    requirement,
                )
                .map_err(FactQueryError::from)?
                else {
                    return Ok(UnwindBehavior::ANY);
                };

                BoundCallableTarget::Declaration(callable)
            }
            bray_checker::ExecutionDependencyTarget::Default(provider) => {
                if let Some(body) = self.declared_unit_key(
                    provider.symbol(),
                    bray_bound_tree::BoundUnitKind::RuntimeDefault,
                )? {
                    pending.push(body);

                    return Ok(UnwindBehavior::NONE);
                }

                let imported = self.checked_default_template(provider, cancellation)?;

                return Ok(imported
                    .value()
                    .as_ref()
                    .filter(|_| !imported.diagnostics().has_errors())
                    .map_or(UnwindBehavior::ANY, |template| {
                        let execution = template.behavior().execution();

                        UnwindBehavior::new(
                            execution.may_unwind(),
                            execution.current_run_cancellation(),
                        )
                    }));
            }
        };

        let BoundCallableTarget::Declaration(instance) = target else {
            if let BoundCallableTarget::Indirect(ty) | BoundCallableTarget::TraitView { ty, .. } =
                target
            {
                let values = self.semantic_value_store()?;
                let data = values.type_data(ty);

                let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
                    panic!("checked indirect invocation retains its callable type");
                };

                if callable.abi() != bray_symbols::CallableAbi::Bray
                    || callable.constness() == bray_symbols::CallableConstness::Constant
                    || callable
                        .phase_behaviors()
                        .invocation()
                        .execution_properties()
                        .contains(&bray_symbols::ExecutionProperty::Total)
                {
                    // Argument admission can still panic before this non-cancelling body.
                    return Ok(UnwindBehavior::PANIC);
                }
            }

            return Ok(UnwindBehavior::ANY);
        };

        let hook = self
            .available_compiler_known_symbols()
            .symbol_implementation(instance.definition().callable_symbol().into_any());

        let context = self.binding_context(cancellation)?;

        let predicates = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallablePredicateContractsQuery,
            >::new(instance.definition().callable_symbol()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        // This analysis has no caller path facts. Value checks can panic and execution
        // requirements need entry authority. Trusted evidence is checked separately.
        let values = self.semantic_value_store()?;
        let mut unproved_requirement = predicates.diagnostics().has_errors();

        for clause in predicates
            .value()
            .iter()
            .filter(|clause| clause.kind() == bray_symbols::CallableContractClauseKind::Requires)
        {
            let Some(predicate) = clause
                .predicate()
                .filter(|predicate| !predicate.is_trusted())
            else {
                continue;
            };

            let condition = predicate
                .condition()
                .map(|term| bray_checker::execution_condition_from_term(values, term, &[]))
                .transpose()
                .map_err(FactQueryError::SemanticValueStore)?;

            unproved_requirement |= condition.is_none_or(|condition| !entry.contains(&condition));
        }

        let entry = if unproved_requirement {
            UnwindBehavior::PANIC
        } else {
            UnwindBehavior::NONE
        };

        if let Some(hook) = hook.filter(|hook| *hook != ImplementationHook::AtomicWait) {
            let cancellation = if matches!(
                hook,
                ImplementationHook::CurrentRunCancellationPropagation
                    | ImplementationHook::TaskYield
                    | ImplementationHook::TaskEventWait
            ) {
                bray_symbols::CurrentRunCancellation::MayEnter
            } else {
                bray_symbols::CurrentRunCancellation::NotEntered
            };

            let may_unwind = hook == ImplementationHook::TestingFail
                || bray_checker::implementation_hook_may_propagate_synchronous_panic(Some(hook));

            return Ok(entry.merge(UnwindBehavior::new(may_unwind, cancellation)));
        }

        if hook == Some(ImplementationHook::AtomicWait) {
            return Ok(entry);
        }

        if let Some(body) = self.callable_body_key(instance.definition())? {
            pending.push(body);

            return Ok(entry);
        }

        let contract = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableContractsQuery,
            >::new(instance.definition().callable_symbol()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        Ok(entry.merge(if contract.diagnostics().has_errors() {
            UnwindBehavior::ANY
        } else {
            let behavior = contract.value().phase_behaviors().invocation();

            UnwindBehavior::new(behavior.may_unwind(), behavior.current_run_cancellation())
        }))
    }

    pub(in crate::compilation) fn checked_unwind(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<bray_bound_tree::CheckedUnwind, FactQueryError> {
        // Contract expressions describe conditions, not runtime ownership cleanup.
        if !matches!(
            key.kind(),
            bray_bound_tree::BoundUnitKind::CallableBody
                | bray_bound_tree::BoundUnitKind::AnonymousCallable
                | bray_bound_tree::BoundUnitKind::RuntimeDefault
                | bray_bound_tree::BoundUnitKind::ConstantTemplate
        ) {
            return Ok(bray_bound_tree::CheckedUnwind::default());
        }

        let candidate = self.unwind_candidate(key, cancellation)?;

        if candidate.result().diagnostics().has_errors() {
            return Ok(bray_bound_tree::CheckedUnwind::default());
        }

        let candidate = candidate.result().value();
        let mut behaviors = std::collections::BTreeMap::new();

        for target in candidate.dependencies() {
            let mut pending = Vec::new();

            let mut behavior = self.queue_unwind_dependency(
                *target,
                candidate.entry_conditions(),
                &mut pending,
                cancellation,
            )?;

            for key in pending {
                let dependency = self.unwind_behavior(key, cancellation)?;

                behavior = behavior.merge(if dependency.result().diagnostics().has_errors() {
                    UnwindBehavior::ANY
                } else {
                    *dependency.result().value()
                });
            }

            behaviors.insert(*target, behavior);
        }

        let calls = candidate
            .calls()
            .iter()
            .filter(|(_, call)| {
                call.locally_non_unwinding()
                    && call
                        .dependencies()
                        .iter()
                        .all(|target| !behaviors[target].may_unwind())
            })
            .map(|(expression, _)| *expression)
            .collect();

        let non_cancelling = candidate
            .calls()
            .iter()
            .filter(|(_, call)| {
                call.dependencies().iter().all(|target| {
                    behaviors[target].current_run_cancellation()
                        == bray_symbols::CurrentRunCancellation::NotEntered
                })
            })
            .map(|(expression, _)| *expression)
            .collect();

        let targets = behaviors
            .into_iter()
            .filter_map(|(target, behavior)| (!behavior.may_unwind()).then_some(target))
            .collect::<BTreeSet<_>>();

        let targets = targets
            .into_iter()
            .filter_map(|target| match target {
                bray_checker::ExecutionDependencyTarget::Callable(target) => Some(target),
                bray_checker::ExecutionDependencyTarget::Requirement { callable, .. } => {
                    Some(BoundCallableTarget::Declaration(callable))
                }
                bray_checker::ExecutionDependencyTarget::Default(_) => None,
            })
            .collect();

        Ok(bray_bound_tree::CheckedUnwind::new(
            calls,
            targets,
            non_cancelling,
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};

    #[test]
    fn unwind_inference_separates_nontermination_from_failure() {
        for (body, expected, cancellation) in [
            (
                "while true {}",
                false,
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
            (
                "root();",
                false,
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
            (
                "if false { let outcome: RunResult<unit> = Cancelled; try outcome; }",
                false,
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
            (
                "panic(\"stop\");",
                true,
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
            (
                "let x = divide(2);",
                true,
                bray_symbols::CurrentRunCancellation::NotEntered,
            ),
            (
                "let outcome: RunResult<unit> = Cancelled; try outcome;",
                true,
                bray_symbols::CurrentRunCancellation::MayEnter,
            ),
        ] {
            let compilation = compilation(&format!(
                r#"
                module example;

                func divide(pos divisor: i32) -> i32
                {{
                    return 1 / divisor;
                }}

                trusted func root()
                {{
                    {body}
                }}
                "#
            ));

            let key = source_function_body_key(&compilation, "root");

            let result = compilation
                .unwind_behavior(key.clone(), &compilation.state.cancellation)
                .expect("fixture callable must resolve its unwind behavior");

            assert!(
                !result.result().diagnostics().has_errors(),
                "{body}: {:?}",
                result.result().diagnostics()
            );

            assert_eq!(result.result().value().may_unwind(), expected, "{body}");

            assert_eq!(
                result.result().value().current_run_cancellation(),
                cancellation,
                "{body}"
            );

            assert_eq!(
                compilation.state.body_semantics.is_published(&key),
                Ok(false)
            );

            let repeated = compilation
                .unwind_behavior(key, &compilation.state.cancellation)
                .expect("fixture callable must resolve its unwind behavior");

            assert!(std::sync::Arc::ptr_eq(&result, &repeated));
        }
    }

    #[test]
    fn inherited_execution_context_proves_only_the_matching_requirement() {
        for (requirement, expected) in [
            ("requires(blocking_execution())", false),
            ("", true),
            ("requires(compute_execution())", true),
        ] {
            let source = format!(
                "module example; func wait() requires(blocking_execution()) {{ while true {{}} }} func root() {requirement} {{ wait(); }}"
            );

            let compilation = compilation(&source);

            let result = compilation
                .unwind_behavior(
                    source_function_body_key(&compilation, "root"),
                    &compilation.state.cancellation,
                )
                .expect("fixture callable must resolve its unwind behavior");

            assert!(
                !result.result().diagnostics().has_errors(),
                "{:?}",
                result.result().diagnostics()
            );

            assert_eq!(result.result().value().may_unwind(), expected, "{source}");
        }
    }

    #[test]
    fn atomic_wait_does_not_unwind_after_its_execution_requirement_is_proven() {
        for (requirement, argument, expected) in [
            ("requires(blocking_execution())", "0", false),
            ("", "0", true),
            ("requires(compute_execution())", "0", true),
            ("requires(blocking_execution())", "fail()", true),
        ] {
            let source = format!(
                r#"
                module app;

                using core.atomic;

                func fail() -> u32
                {{
                    panic("argument failed");
                }}

                func root(pos storage: &core.atomic.Atomic<u32>) {requirement}
                {{
                    core.atomic.wait<u32, 1>(storage, {argument});
                }}
            "#
            );

            let compilation = compilation(&source);

            let result = compilation
                .unwind_behavior(
                    source_function_body_key(&compilation, "root"),
                    &compilation.state.cancellation,
                )
                .expect("fixture callable must resolve its unwind behavior");

            assert!(
                !result.result().diagnostics().has_errors(),
                "{source}: {:?}",
                result.result().diagnostics()
            );

            assert_eq!(result.result().value().may_unwind(), expected, "{source}");
        }
    }

    #[test]
    fn unwind_inference_includes_implicit_execution() {
        for (declarations, body, expected) in [
            (
                "struct Value { value: bool; destruct() { panic(\"cleanup\"); } }",
                "let value: Value = { value = true };",
                true,
            ),
            (
                "struct Value { value: bool; destruct() {} }",
                "let value: Value = { value = true };",
                false,
            ),
            (
                "@abi(c) extern trusted func wait();",
                "trusted wait();",
                false,
            ),
            (
                "func checked(pos flag: bool) requires(flag) {}",
                "checked(false);",
                true,
            ),
            (
                "struct Value { value: bool; enter() -> bool { return true; } exit(pos lease: bool) { panic(\"exit\"); } }",
                "let value: Value = { value = true }; with lease = value {}",
                true,
            ),
            (
                "struct Value { value: bool; enter() -> bool { panic(\"entry\"); } exit(pos lease: bool) {} }",
                "let value: Value = { value = true }; with lease = value {}",
                true,
            ),
            (
                "struct Value { value: bool; enter() -> bool { return true; } exit(pos lease: bool) {} }",
                "let value: Value = { value = true }; with lease = value {}",
                false,
            ),
            (
                "func fail() -> bool { panic(\"default\"); } func accept(value: bool = fail()) {}",
                "accept();",
                true,
            ),
        ] {
            let compilation = compilation(&format!(
                "module example; {declarations} trusted func root() {{ {body} }}"
            ));

            let key = source_function_body_key(&compilation, "root");

            let result = compilation
                .unwind_behavior(key, &compilation.state.cancellation)
                .expect("fixture callable must resolve its unwind behavior");

            assert!(
                !result.result().diagnostics().has_errors(),
                "{declarations}: {:?}",
                result.result().diagnostics()
            );

            assert_eq!(
                result.result().value().may_unwind(),
                expected,
                "{declarations}"
            );
        }
    }
}
