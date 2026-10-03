use std::sync::Arc;

use bray_binder::semantic_unit_context;
use bray_bound_tree::{BoundUnitKey, CheckedBodySemantics, StoragePlan};
use bray_checker::{BodySemanticChecker, DefaultBodySemanticChecker};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};

use super::support::plan_storage;
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError, PublishedUnitResult};

impl Compilation {
    pub(in crate::compilation) fn storage_plan_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<StoragePlan>>, FactQueryError> {
        self.unit_query(
            &self.state.storage_plans,
            CompilationFactKey::StoragePlan(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let declared = self
                    .declared_value_type_templates_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;

                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let result = plan_storage(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                    declared.result().value(),
                    expressions.result().value().types(),
                    patterns.result().value(),
                    expressions.result().value().selections(),
                )?;

                let (plan, plan_diagnostics) = result.into_parts();

                let diagnostics = DiagnosticBag::merged_all([
                    bound.result().diagnostics(),
                    declared.result().diagnostics(),
                    expressions.result().diagnostics(),
                    patterns.result().diagnostics(),
                    &plan_diagnostics,
                ]);

                Ok((DiagnosticResult::new(plan, diagnostics), Box::new([])))
            },
        )
    }

    pub(in crate::compilation) fn body_semantics_with_cancellation(
        &self,
        key: BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<Arc<PublishedUnitResult<CheckedBodySemantics>>, FactQueryError> {
        self.unit_query(
            &self.state.body_semantics,
            CompilationFactKey::BodySemantics(key.clone()),
            key.clone(),
            cancellation,
            |cancellation| {
                let bound = self.bound_unit_with_cancellation(key.clone(), cancellation)?;

                let control_flow =
                    self.control_flow_with_cancellation(key.clone(), cancellation)?;

                let expressions =
                    self.expression_semantics_with_cancellation(key.clone(), cancellation)?;

                let patterns = self.patterns_with_cancellation(key.clone(), cancellation)?;
                let storage = self.storage_plan_with_cancellation(key.clone(), cancellation)?;
                let memory = self.memory_operations_with_cancellation(key.clone(), cancellation)?;
                let context = self.checker_context_for(&key, cancellation)?;

                let semantic_context =
                    semantic_unit_context(context.symbols(), bound.result().value());

                let unit = bray_checker::CheckerUnitView::new(
                    bound.result().value(),
                    &semantic_context,
                    &context,
                );

                let result = checker_result(DefaultBodySemanticChecker.check_body_semantics(
                    unit,
                    control_flow.result().value(),
                    expressions.result().value(),
                    patterns.result().value(),
                    storage.result().value(),
                    memory.result().value(),
                ))?;

                let (semantics, semantic_diagnostics) = result.into_parts();

                Ok((
                    DiagnosticResult::new(semantics, semantic_diagnostics),
                    Box::new([]),
                ))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write;

    use crate::test_support::{compilation, source_function_body_key};

    #[test]
    fn dense_scalar_assignments_retain_only_current_storage() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {
                    return input;
                }

                func caller() -> bool
                {
                    let mut value: bool = true;
                "#,
            );

            for _ in 0..count {
                writeln!(source, "value = identity(value);").unwrap();
            }

            source.push_str("\nreturn value;\n}\n");

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .expect("dense scalar body must check");

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
            assert_eq!(flow.value().replacements().len(), count);

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| exit.live().len() <= 8),
                "expired scalar temporaries must not accumulate in exit state"
            );
        }
    }

    #[test]
    fn scalar_temporary_borrows_survive_until_their_parent_consumes_them() {
        let compilation = compilation(
            r#"
            module example;

            func identity(pos input: bool) -> bool
            {
                return input;
            }

            func read(pos input: &bool) -> bool
            {
                return input;
            }

            func caller() -> bool
            {
                let borrowed = &identity(true);

                return read(borrowed);
            }
        "#,
        );

        let flow = compilation
            .storage_flow(source_function_body_key(&compilation, "caller"))
            .expect("borrowed scalar temporary must check");

        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
    }
    #[test]
    fn scalar_temporaries_expire_across_branches_and_loop_iterations() {
        let compilation = compilation(
            r#"
            module example;

            func identity(pos input: bool) -> bool
            {
                return input;
            }

            func caller(pos condition: bool) -> bool
            {
                let mut value: bool = true;

                if condition
                {
                    value = identity(false);
                }
                else
                {
                    value = identity(true);
                }

                while value
                {
                    value = identity(false);
                }

                return value;
            }
        "#,
        );

        let flow = compilation
            .storage_flow(source_function_body_key(&compilation, "caller"))
            .expect("joined and looped scalar body must check");

        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());
        assert_eq!(flow.value().replacements().len(), 3);

        assert!(
            flow.value()
                .exits()
                .iter()
                .all(|exit| exit.live().len() <= 8),
            "expired branch and loop temporaries must not accumulate"
        );
    }

    #[test]
    fn unused_temporary_destructors_remain_scope_exit_obligations() {
        let compilation = compilation(
            r#"
            module example;

            struct Guard
            {
                value: bool;
                destruct()
                {
                }
            }

            func create() -> Guard
            {
                return Guard
                {
                    value = true,
                };
            }

            func caller()
            {
                create();
            }
        "#,
        );

        let key = source_function_body_key(&compilation, "caller");

        let bound = compilation
            .bound_unit(key.clone())
            .expect("guard body must bind");

        let call = bound
            .value()
            .tree()
            .expressions()
            .find_map(|(id, expression)| {
                matches!(expression, bray_bound_tree::BoundExpression::Call(_)).then_some(id)
            })
            .expect("guard creation must retain its call");

        let storage = compilation
            .storage_plan(key.clone())
            .expect("guard storage must plan");

        let identity = storage.value().identity_entries().find_map(|(id, identity)| matches!(identity, bray_bound_tree::StorageIdentity::Temporary(expression) if expression == call).then_some(id)).expect("guard result must retain a temporary identity");

        let flow = compilation
            .storage_flow(key)
            .expect("guard cleanup must check");

        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

        assert!(
            flow.value()
                .exits()
                .iter()
                .any(|exit| exit.live().contains(&identity)
                    && exit.initialized().contains(&identity)),
            "unused cleanup-bearing temporary must survive to scope exit"
        );
    }

    #[test]
    fn unused_scalar_locals_retain_bounded_storage() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {
                    return input;
                }

                func caller()
                {
                "#,
            );

            for index in 0..count {
                writeln!(source, "let value_{index}: bool = identity(true);").unwrap();
            }

            source.push_str("\n}\n");

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .expect("unused scalar locals must check");

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| exit.live().len() <= 8),
                "unused scalar locals must not accumulate in later flow states"
            );
        }
    }

    #[test]
    fn unused_scalar_parameters_do_not_enter_flow_state() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from("module example;\nfunc caller(\n");

            for index in 0..count {
                writeln!(source, "pos value_{index}: bool,").unwrap();
            }

            source.push_str(
                r#") -> bool
                {
                    return true;
                }
                "#,
            );

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "caller");

            let storage = compilation
                .storage_plan(key.clone())
                .expect("parameters must plan");

            let parameters = storage
                .value()
                .identity_entries()
                .filter_map(|(id, identity)| {
                    matches!(identity, bray_bound_tree::StorageIdentity::Parameter(_)).then_some(id)
                })
                .collect::<Vec<_>>();

            let flow = compilation
                .storage_flow(key)
                .expect("unused parameters must check");

            assert_eq!(parameters.len(), count);
            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| parameters.iter().all(|id| !exit.live().contains(id))),
                "unused cleanup-free parameters must not multiply flow availability"
            );
        }
    }

    #[test]
    fn recurrent_parameter_uses_without_a_final_use_remain_available() {
        for body in [
            "loop\n{\nidentity(input);\n}\n",
            "loop\n{\nidentity(input);\ncontinue;\n}\n",
        ] {
            let source = format!(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {{
                    return input;
                }}

                func caller(pos input: bool)
                {{
                    {body}
                }}
                "#,
            );

            let compilation = compilation(&source);
            let diagnostics = compilation.check_diagnostics();

            assert!(!diagnostics.has_errors(), "{body}: {diagnostics:?}");
        }
    }

    #[test]
    fn returned_parameter_borrows_preserve_owner_provenance() {
        let compilation = compilation(
            r#"
            module example;

            func caller(pos input: &bool) -> &bool
            {
                let local = input;

                return local;
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn unused_owned_parameters_and_locals_preserve_cleanup_obligations() {
        let compilation = compilation(
            r#"
            module example;

            struct Guard
            {
                value: bool;
                destruct()
                {
                }
            }

            func caller(pos input: Guard)
            {
                let local = Guard
                {
                    value = true,
                };
            }
            "#,
        );

        let key = source_function_body_key(&compilation, "caller");

        let storage = compilation
            .storage_plan(key.clone())
            .expect("owned values must plan");

        let obligations = storage
            .value()
            .identity_entries()
            .filter_map(|(id, identity)| {
                matches!(
                    identity,
                    bray_bound_tree::StorageIdentity::Parameter(_)
                        | bray_bound_tree::StorageIdentity::LocalOwned(_)
                )
                .then_some(id)
            })
            .collect::<Vec<_>>();

        let flow = compilation
            .storage_flow(key)
            .expect("owned cleanup must check");

        assert_eq!(obligations.len(), 2);
        assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

        assert!(
            flow.value().exits().iter().any(|exit| obligations
                .iter()
                .all(|id| exit.live().contains(id) && exit.initialized().contains(id))),
            "unused owned parameters and locals must survive until cleanup"
        );
    }

    #[test]
    fn optionally_used_scalar_locals_expire_on_skipped_paths() {
        for count in [64, 128, 256, 1024] {
            for short_circuit in [false, true] {
                let mut source = String::from(
                    r#"
                    module example;

                    func identity(pos input: bool) -> bool
                    {
                        return input;
                    }

                    func caller(pos condition: bool)
                    {
                    "#,
                );

                for index in 0..count {
                    writeln!(source, "let value_{index}: bool = identity(true);").unwrap();

                    if short_circuit {
                        writeln!(
                            source,
                            "let discarded_{index}: bool = condition && identity(value_{index});"
                        )
                        .unwrap();
                    } else {
                        writeln!(source, "if condition\n{{\nidentity(value_{index});\n}}").unwrap();
                    }
                }

                source.push_str("\n}\n");

                let compilation = compilation(&source);

                let flow = compilation
                    .storage_flow(source_function_body_key(&compilation, "caller"))
                    .expect("optional uses must check");

                assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

                assert!(
                    flow.value()
                        .exits()
                        .iter()
                        .all(|exit| exit.live().len() <= 8),
                    "skipped branches and short-circuit operands must not retain dead locals"
                );
            }
        }
    }

    #[test]
    fn failed_call_inputs_expire_on_caught_paths() {
        for count in [64, 128, 256] {
            let mut source = String::from(
                r#"
                module example;

                func identity(pos input: bool) -> bool
                {
                    if input
                    {
                        panic("failed input");
                    }

                    return input;
                }

                func caller()
                {
                "#,
            );

            for index in 0..count {
                writeln!(
                    source,
                    "let value_{index}: bool = true;\ncatch\n{{\nidentity(value_{index});\n}};"
                )
                .unwrap();
            }

            source.push_str("\n}\n");

            let compilation = compilation(&source);
            let key = source_function_body_key(&compilation, "caller");

            let storage = compilation
                .storage_plan(key.clone())
                .expect("caught inputs must plan");

            let inputs = storage
                .value()
                .identity_entries()
                .filter_map(|(id, identity)| {
                    matches!(identity, bray_bound_tree::StorageIdentity::LocalOwned(_))
                        .then_some(id)
                })
                .collect::<Vec<_>>();

            let flow = compilation
                .storage_flow(key)
                .expect("caught calls must check");

            assert_eq!(inputs.len(), count);

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value().exits().iter().all(|exit| exit
                    .live()
                    .iter()
                    .filter(|id| inputs.contains(id))
                    .count()
                    <= 1),
                "call failure must retire dead scalar inputs without discarding owned catch results"
            );
        }
    }

    #[test]
    fn discarded_borrow_results_release_their_scalar_owners() {
        for count in [64, 128, 256, 1024] {
            let mut source = String::from(
                r#"
                module example;

                func echo(pos input: &bool) -> &bool
                {
                    return input;
                }

                func caller()
                {
                "#,
            );

            for index in 0..count {
                writeln!(
                    source,
                    "let value_{index}: bool = true;\necho(&value_{index});"
                )
                .unwrap();
            }

            source.push_str("\n}\n");

            let compilation = compilation(&source);

            let flow = compilation
                .storage_flow(source_function_body_key(&compilation, "caller"))
                .expect("discarded borrow results must check");

            assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

            assert!(
                flow.value()
                    .exits()
                    .iter()
                    .all(|exit| exit.live().len() <= 8),
                "an expired borrow result must not retain a dead scalar owner"
            );
        }
    }

    #[test]
    fn retained_borrow_results_keep_storage_until_consumption() {
        let compilation = compilation(
            r#"
            module example;

            func echo(pos input: &bool) -> &bool
            {
                return input;
            }

            func observe(pos input: &bool) -> bool
            {
                return true;
            }

            func caller() -> bool
            {
                let value: bool = true;
                let retained = echo(&value);
                observe(&false);

                return observe(retained);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");
    }

    #[test]
    fn exact_lifetime_retirement_preserves_escaping_borrow_diagnostics() {
        let compilation = compilation(
            r#"
            module example;

            func echo(pos input: &bool) -> &bool
            {
                return input;
            }

            func caller() -> &bool
            {
                let value: bool = true;

                return echo(&value);
            }
            "#,
        );

        let diagnostics = compilation.check_diagnostics();

        assert!(
            diagnostics.iter().any(|diagnostic| diagnostic.kind()
                == bray_diagnostics::DiagnosticKind::CheckingEscapingStorageDependency),
            "returning a local borrow must remain rejected: {diagnostics:?}"
        );
    }
}
