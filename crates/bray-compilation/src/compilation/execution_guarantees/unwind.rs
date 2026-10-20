use std::collections::BTreeSet;
use std::sync::Arc;

use bray_binder::{SymbolQueryProvider, semantic_unit_context};
use bray_bound_tree::{BoundCallableTarget, BoundUnitKey};
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

                let request =
                    CheckerUnitView::new(bound.result().value(), &semantic_context, &context);

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
    ) -> Result<Arc<PublishedUnitResult<bool>>, FactQueryError> {
        self.unit_query(
            &self.state.unwind_behavior,
            CompilationFactKey::UnwindBehavior(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let mut pending = vec![key.clone()];
                let mut visited = BTreeSet::new();
                let mut diagnostics = DiagnosticBag::new();
                let mut may_unwind = false;

                while let Some(key) = pending.pop() {
                    cancellation.check()?;

                    if !visited.insert(key.clone()) {
                        continue;
                    }

                    let candidate = self.unwind_candidate(key.clone(), cancellation)?;

                    diagnostics = diagnostics.merged(candidate.result().diagnostics());

                    may_unwind |= candidate.result().diagnostics().has_errors()
                        || candidate.result().value().may_unwind();

                    if may_unwind {
                        break;
                    }

                    for target in candidate.result().value().dependencies() {
                        may_unwind |= self.queue_unwind_dependency(
                            *target,
                            candidate.result().value().entry_conditions(),
                            &mut pending,
                            cancellation,
                        )?;
                    }
                }

                Ok((DiagnosticResult::new(may_unwind, diagnostics), Box::new([])))
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

        let declared = self.execution_declaration(key.source().syntax())?;

        let inputs =
            self.predicate_condition_inputs(key, declared.value().requirements(), cancellation)?;

        let (inputs, diagnostics) = inputs.into_parts();

        let entry = inputs
            .into_iter()
            .filter_map(|(condition, trusted, _)| {
                let bray_checker::ExecutionCondition::Predicate(predicate, _, arguments) =
                    &condition
                else {
                    return None;
                };

                (!trusted
                    && arguments.is_empty()
                    && self
                        .available_compiler_known_symbols()
                        .symbol_implementation(predicate.into_any())
                        .is_some_and(ImplementationHook::is_execution_requirement))
                .then_some(condition)
            })
            .collect();

        Ok(DiagnosticResult::new(entry, diagnostics))
    }

    fn queue_unwind_dependency(
        &self,
        target: BoundCallableTarget,
        entry: &BTreeSet<bray_checker::ExecutionCondition>,
        pending: &mut Vec<BoundUnitKey>,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let BoundCallableTarget::Declaration(instance) = target else {
            return Ok(true);
        };

        let hook = self
            .available_compiler_known_symbols()
            .symbol_implementation(instance.definition().callable_symbol().into_any());

        if let Some(hook) = hook.filter(|hook| *hook != ImplementationHook::AtomicWait) {
            return Ok(matches!(
                hook,
                ImplementationHook::CurrentRunCancellationPropagation
                    | ImplementationHook::TestingFail
                    | ImplementationHook::TaskYield
                    | ImplementationHook::TaskEventWait
            ) || bray_checker::implementation_hook_may_propagate_synchronous_panic(
                Some(hook),
            ));
        }

        let context = self.binding_context(cancellation)?;

        let predicates = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallablePredicateContractsQuery,
            >::new(instance.definition().callable_symbol()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        // This analysis has no caller path facts. An ordinary requirement may need a
        // runtime check, whereas trusted obligations are checked before execution.
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

        if unproved_requirement {
            return Ok(true);
        }

        if hook == Some(ImplementationHook::AtomicWait) {
            return Ok(false);
        }

        if let Some(body) = self.callable_body_key(instance.definition())? {
            pending.push(body);

            return Ok(false);
        }

        let contract = context
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableContractsQuery,
            >::new(instance.definition().callable_symbol()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        Ok(contract.diagnostics().has_errors()
            || contract.value().phase_behaviors().invocation().may_unwind())
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
        let mut targets = BTreeSet::new();

        for target in candidate.dependencies() {
            let mut pending = Vec::new();

            let mut may_unwind = self.queue_unwind_dependency(
                *target,
                candidate.entry_conditions(),
                &mut pending,
                cancellation,
            )?;

            for key in pending {
                let behavior = self.unwind_behavior(key, cancellation)?;

                may_unwind |=
                    *behavior.result().value() || behavior.result().diagnostics().has_errors();
            }

            if !may_unwind {
                targets.insert(*target);
            }
        }

        let calls = candidate
            .calls()
            .iter()
            .filter(|(_, dependencies)| dependencies.iter().all(|target| targets.contains(target)))
            .map(|(expression, _)| *expression)
            .collect();

        Ok(bray_bound_tree::CheckedUnwind::new(calls, targets))
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::{compilation, source_function_body_key};

    #[test]
    fn unwind_inference_separates_nontermination_from_failure() {
        for (body, expected) in [
            ("while true {}", false),
            ("root();", false),
            ("panic(\"stop\");", true),
            ("let x = divide(2);", true),
        ] {
            let compilation = compilation(&format!(
                "module example; func divide(pos divisor: i32) -> i32 {{ return 1 / divisor; }} trusted func root() {{ {body} }}"
            ));

            let key = source_function_body_key(&compilation, "root");

            let result = compilation
                .unwind_behavior(key.clone(), &compilation.state.cancellation)
                .unwrap();

            assert!(
                !result.result().diagnostics().has_errors(),
                "{body}: {:?}",
                result.result().diagnostics()
            );

            assert_eq!(*result.result().value(), expected, "{body}");

            assert_eq!(
                compilation.state.body_semantics.is_published(&key),
                Ok(false)
            );

            let repeated = compilation
                .unwind_behavior(key, &compilation.state.cancellation)
                .unwrap();

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
                .unwrap();

            assert!(
                !result.result().diagnostics().has_errors(),
                "{:?}",
                result.result().diagnostics()
            );

            assert_eq!(*result.result().value(), expected, "{source}");
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
                func fail() -> u32 {{ panic("argument failed"); }}
                func root(pos storage: &core.atomic.Atomic<u32>) {requirement} {{
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
                .unwrap();

            assert!(
                !result.result().diagnostics().has_errors(),
                "{source}: {:?}",
                result.result().diagnostics()
            );

            assert_eq!(*result.result().value(), expected, "{source}");
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
                .unwrap();

            assert!(
                !result.result().diagnostics().has_errors(),
                "{declarations}: {:?}",
                result.result().diagnostics()
            );

            assert_eq!(*result.result().value(), expected, "{declarations}");
        }
    }
}
